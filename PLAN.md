# AgentWatch implementation plan

## Engine: neo

AgentWatch is a TUI front-end to the [neo](https://github.com/matthart1983/neo) agent runtime. It reads neo's on-disk state and writes prompts/control messages into neo's inbox. **No neo internals are linked in** — the boundary is the JSON contract in `src/data/contract.rs`.

## What neo gives us today

- 8-agent enum (`Router/Planner/Coder/Reviewer/Debugger/Tester/Documenter/Oracle`) — `src/agents/types.rs`
- Thread JSON persistence at `dirs::data_dir()/neo/threads/T-*.json`
  - schema = `{id, created_at, updated_at, workspace, messages, cost_total, models_used, tags}` — directly usable for the Sessions tab
- `Orchestrator::handle_message` / `handle_pipeline` / `handle_command` — clean hook points
- `AgentExecutor::run` returns `AgentResult{content, model_used, tokens_in, tokens_out, cost_usd, tool_calls_made, iterations}` — every field we want in `Invocation`
- `SessionStats` per-session totals
- `PipelineResult` end-of-pipeline summary (with `models_used: Vec<(String, AgentId)>`)

## What neo is missing — upstream PRs to land

1. **`StateEmitter`** in `src/orchestrator/` — not started
   - writes `state.json` on every agent tick (~200ms debounce)
   - serialises `StateSnapshot` from `data/contract.rs`
   - fed by an `mpsc::Sender<AgentEvent>` injected into `AgentExecutor`
   - **blocked**: needs constructor changes in `orchestrator/mod.rs` (currently in user's WIP)
2. **`InvocationLog`** in `src/session/` — **DONE** (PR #1)
   - landed as `src/session/invocations.rs` + 45 lines in `src/agents/executor.rs`
   - uses `std::sync::OnceLock` for lazy global init, no new deps
   - hooks into both return paths in `AgentExecutor::run` (`success` and `max_iterations`)
3. **`ControlInbox`** — **DONE as module** (PR #3)
   - landed as `src/session/control_inbox.rs` (temporary home; conceptual home is `orchestrator/control_inbox.rs`)
   - exposes `ControlInbox::poll_once()` + `ack(path)`; no `notify` dep (sync polling)
   - **wiring still pending**: nobody calls `poll_once()` yet — needs to be hooked into the REPL idle loop or orchestrator main task once WIP settles
4. **Pipeline live state** — **DONE** (PR #4)
   - landed as `PipelineEvent` enum + `run_pipeline_with_events()` wrapper in `src/orchestrator/pipeline.rs`
   - existing `run_pipeline()` is now a thin delegate with `None` sender — zero behaviour change for current callers
   - **wiring still pending**: orchestrator's `handle_pipeline` doesn't create a sender yet — needs to switch to the `_with_events` variant once WIP settles
5. **Workflow presets** — not started
   - TOML files in `~/.local/share/neo/profiles/*.toml`; router accepts a profile selection
   - **blocked**: touches router + Cargo.toml (Cargo.toml is in WIP)

(Optional v2) **`neo-agentd`** — separate `[[bin]]` binding a Unix socket and proxying to the orchestrator. The inbox-file fallback in (3) is fine for v1.

## Follow-up gaps discovered during PR work

These are smaller items we deferred from the PRs above to keep each one minimal and additive. Pick them up after the main PRs land.

### From PR #1 (InvocationLog)

| Gap | Why deferred | Fix sketch |
|---|---|---|
| `thread: ""` always empty | `AgentExecutor` doesn't know session/thread context; threading it through means changing the constructor signature in `orchestrator/mod.rs` (currently in WIP) | Add `pub fn set_current_thread(&mut self, id: String)` on `AgentExecutor`. Orchestrator calls it before `executor.run(...)` in `handle_message` / `handle_pipeline` / `handle_command`. Field becomes `thread: Option<String>` or stays empty when not set. |
| `cost: 0.0` always zero | Matches existing `AgentResult.cost_usd` behaviour — neo doesn't compute cost from tokens today | Derive from router pricing: `ModelRouter` already has `Vec<ModelInfo>` with `pricing.prompt` + `pricing.completion`. Compute `(tokens_in * prompt + tokens_out * completion) / 1_000_000` inside `executor.run` and on `AgentResult`. Update `SessionManager::record_cost` callers to use the real value. |
| Errors not logged | `?` propagation skips the log path; only `Ok` returns reach the append call | Wrap the chat call in a match. On error, log a record with `status: "error"` and zero tokens. Keep the `?` semantics by re-raising. |
| `provider` hardcoded `"openrouter"` | neo only routes through OpenRouter today | When/if other providers (ollama, anthropic-direct) are added, route through them and tag records accordingly. Cheap to fix later. |
| `tool_calls` is the aggregate count for the whole `run()` invocation | Matches the contract field but loses per-tool granularity | Per-tool detail belongs in a separate `tool_invocations.jsonl` for the Tools tab. Out of scope for InvocationLog. |
| No fallback-from / fallback-to tracking | Router doesn't fall back today; metric is moot | When router fallback lands, extend `status` to `"fallback(prev_model)"` per the contract. |

### From PR #3 (ControlInbox)

| Gap | Why deferred | Fix sketch |
|---|---|---|
| Module lives in `src/session/control_inbox.rs` instead of `src/orchestrator/control_inbox.rs` | `orchestrator/mod.rs` is in WIP; touching it risks conflict | Move to `src/orchestrator/control_inbox.rs` and register in `orchestrator/mod.rs` once WIP merges. Public API is stable across the move. |
| Nobody calls `poll_once()` | Wiring into the long-running orchestrator loop touches `orchestrator/mod.rs` (WIP) | Add a tokio interval (~1s) in the orchestrator startup path that calls `poll_once()` and dispatches each `ControlCommand` via `handle_message` / `handle_pipeline`. |
| Sync `std::fs` instead of `tokio::fs` | Keeps the PR dep-free and the API non-async; polling is rare | Swap to `tokio::fs` if/when call frequency exceeds ~1Hz. Trivial change. |
| No `notify`-based watcher | Adding `notify` means touching Cargo.toml (WIP) and polling at 1Hz is adequate for v1 | Add `notify = "6"` and a watcher task once the WIP is in. The public API stays the same. |
| Parse failures leave files in place silently | Avoids the question of where to move them ("inbox/.dead/"?) and what to log | Add a `dead/` subdir and move malformed files there with a stderr warning. |

### From PR #4 (Pipeline events)

| Gap | Why deferred | Fix sketch |
|---|---|---|
| Orchestrator's `handle_pipeline` still calls `run_pipeline` (no events) | Switching the call site touches `orchestrator/mod.rs` (WIP) | Once WIP settles: create an `mpsc::channel(64)` in `handle_pipeline`, call `run_pipeline_with_events(..., Some(tx))`, and spawn a task to forward events to AgentWatch's state file (or emit on the StateEmitter channel from PR #2). |
| `Started` and `Finished` events don't carry tokens / cost yet | Keeps the enum minimal; the data is in `PipelineResult` anyway | Add summary fields once a real consumer (Plans tab) needs them. |
| `try_send` drops events when the channel is full | Pipeline must not block on slow consumers — correctness over completeness | Replace with `send().await` if a consumer needs strict delivery. Probably fine to keep `try_send` for the UI use case. |
| No event for the `track_result` call | Per-call invocation data already flows through PR #1's `InvocationLog`; duplicating in pipeline events would be redundant | Leave as-is; cross-reference via the `thread` field once PR #1's "enrich thread" gap is closed. |

## Path convention

neo uses `dirs::data_dir()` which is `~/Library/Application Support/neo` on macOS, `~/.local/share/neo` on Linux. AgentWatch follows the same convention via `src/data/paths.rs`. The Linux-y paths in the design handoff are interpreted as platform-portable references resolved through `dirs::data_dir()`.

## Milestones

| ID | Scope | Blocked on |
|----|------------------------------------------------------|---------------|
| M1 | Scaffold (this commit): chrome, 10 tab files, data contract types, inbox writer | — |
| M2 | Sessions tab fully rendered against existing thread JSON | — |
| M3 | neo upstream PRs (StateEmitter, InvocationLog, ControlInbox, pipeline events) | neo PRs |
| M4 | Agents / Plans / Tools / Models / Cost / Overview / Insights tabs at high fidelity | M3 |
| M5 | Console + Thread driver tabs — prompt send via InboxWriter; workflow-preset picker | M3 + preset PR |
| M6 | (optional) `agentd` socket fast path | neo-agentd PR |

## Design fidelity rule

The JSX in `~/Downloads/design_handoff_agentwatch/source/` is the executable spec. Every column / row / glyph / colour comes from there; the README says "if anything contradicts the source, the source wins." Treat the JSX as the master and the markdown as commentary.
