# Multi-provider support — design plan

Today AgentWatch is a thin front-end to neo, and neo only knows
OpenRouter. This document plans the path to first-class support for
additional providers — **Anthropic (direct), OpenAI (direct), GitHub
Copilot, and Ollama** — while keeping the boundary between AgentWatch
(UI) and neo (orchestrator) clean.

## Goals

| Provider | Why it matters |
|---|---|
| **Ollama** | Local-only, free, no API key. Privacy-sensitive prompts, dev-time iteration without billing meter. Cheap fallback for the `oracle` workflow. |
| **Anthropic direct** | Most users already have an `ANTHROPIC_API_KEY` for claude.ai / Claude Code. Skips the OpenRouter hop for ~5–10% latency win and same price. Earliest access to new Claude features (prompt caching, citations) than aggregators ship. |
| **OpenAI direct** | Some users have OpenAI but not OpenRouter. Lower latency for OpenAI models. Access to assistants/responses API. |
| **GitHub Copilot** | Many devs already pay for Copilot Business/Enterprise. Lets them use AgentWatch without a second LLM bill. |

## Current architecture (what we have)

```
                   ┌──────────────┐    spawns
                   │  AgentWatch  │  ──────────►  neo (subprocess)
                   │     (UI)     │                ─ uses OpenRouter only
                   └──────────────┘                ─ ONE provider, ONE key
                          ▲
                          │ reads
                          ▼
               ~/Library/Application Support/neo/
               ├ threads/*.json
               ├ invocations.jsonl      (provider field already exists,
               │                         hardcoded to "openrouter")
               └ inbox/<uuid>.json
```

Key shape today:
- `OPENROUTER_API_KEY` (single env var)
- Model ids: `<owner>/<model>` (OpenRouter convention)
- One pricing table (in AgentWatch — `src/data/pricing.rs`)
- One default model (`NEO_DEFAULT_MODEL`)

## Architectural decision: where does the provider abstraction live?

**Decision: in neo.** AgentWatch stays a UI; neo owns multi-provider
dispatch. This keeps two invariants:

1. AgentWatch never holds a long-lived secret beyond the env var it
   inherits.
2. AgentWatch's data layer doesn't change — `InvocationRecord.provider`
   is already part of the contract; we just start seeing values other
   than `"openrouter"`.

Things that DO change in AgentWatch:

- Pricing table → multi-provider (different rates per provider for the
  same model name).
- Model registry → know which provider a model belongs to.
- Team panel → show provider badge per member.
- Models tab → group/filter by provider.
- Cost tab → per-provider breakdown.
- New `/auth` slash command + panel for credential introspection (never
  for entry — keys go in env or `~/.config/neo/auth.toml`).

## Provider integration notes

### Ollama (local, no key)

| Field | Value |
|---|---|
| Auth | None — uses `http://localhost:11434` by default |
| Models | `llama3.3:70b`, `mistral`, `deepseek-coder:33b`, etc. |
| Pricing | $0 (local compute) |
| Speed | Slow — local GPU bound |
| Tool use | Model-dependent (most current models support function calling) |
| Streaming | Yes (chunked /api/generate) |
| Capability tier | Good for q&a, docs, planning. Weaker for complex code edits. |

Integration:
- neo gets `OllamaProvider` impl
- AgentWatch shows `ollama:` prefix or 🏠 glyph in model names
- Pricing entry: zero across the board
- Detection: `curl http://localhost:11434/api/tags` succeeds → enabled

### Anthropic direct

| Field | Value |
|---|---|
| Auth | `ANTHROPIC_API_KEY` |
| Models | `claude-sonnet-4`, `claude-opus-4`, `claude-3.5-sonnet`, `claude-3.5-haiku` |
| Pricing | Same as OpenRouter for the same model (Anthropic sets the rate) — but no aggregator markup |
| Speed | Fast (direct) |
| Tool use | Full — neo's existing tool registry works as-is |
| Streaming | Yes (SSE) |
| Prompt caching | Yes — automatic 5-min cache, ~90% input discount on cache hits. Worth wiring through because long-running pipelines (Planner → Coder → Reviewer over the same context) win materially. |

Integration:
- neo gets `AnthropicProvider` impl. Anthropic's API is similar enough
  to OpenAI's chat completions that the bulk of `OpenAIProvider` is
  reusable; the differences live in request shaping (system message goes
  to a top-level `system` field, not the messages array) and cache
  control headers.
- Model id format: `anthropic/claude-sonnet-4` — matches existing
  OpenRouter convention so the team panel and pricing table don't need
  to flip.
- Conflict resolution: same as OpenAI — direct provider beats aggregator
  when both are configured.
- AgentWatch's pricing table is already correct for these models (we
  copied Anthropic's rates), so no per-provider markup is needed.

### OpenAI direct

| Field | Value |
|---|---|
| Auth | `OPENAI_API_KEY` |
| Models | `gpt-4o`, `gpt-4o-mini`, `o1`, `o3-mini`, `gpt-4-turbo` |
| Pricing | Often 5–10% cheaper than the same model via OpenRouter |
| Speed | Fast (direct) |
| Tool use | Full — neo's existing tool registry works as-is |
| Streaming | Yes (SSE) |

Integration:
- neo gets `OpenAIProvider` impl (mostly a copy of OpenRouter client with
  a different base URL and auth header)
- Model id format: `openai/gpt-4o` (already matches; just a different
  routing decision at dispatch time)
- Conflict resolution: if both OpenRouter and OpenAI are configured and
  the user picks `openai/gpt-4o`, prefer the direct provider

### GitHub Copilot

| Field | Value |
|---|---|
| Auth | OAuth device flow → `~/.config/github-copilot/hosts.json` or `GH_COPILOT_TOKEN` |
| Models | `copilot-codex` (legacy), `gpt-4o-copilot`, `o1-copilot` |
| Pricing | Per-seat subscription — no per-token billing |
| Speed | Fast |
| Tool use | Limited — Copilot's API surface is narrower than OpenAI direct |
| Streaming | Yes |

Integration:
- neo gets `CopilotProvider` impl. Auth is the gnarly bit — Copilot's
  token is short-lived and refreshed via the GitHub CLI or a device-flow
  helper.
- Pricing: $0 per call (subscription is already paid). Show a small
  `subscription` tag instead of a dollar amount.
- Tool use restrictions: tag models as `tool_use: limited` in the
  capability table so neo's router doesn't pick Copilot for heavy
  tool-using agents (Coder).

## Credential management

**AgentWatch does not store credentials.** Keys live in:

1. **Env vars** (default): `OPENROUTER_API_KEY`, `ANTHROPIC_API_KEY`,
   `OPENAI_API_KEY`, `OLLAMA_HOST` (optional, defaults to
   `http://localhost:11434`), `GH_COPILOT_TOKEN`.
2. **neo's config layer**: `~/.config/neo/auth.toml`:
   ```toml
   [providers.openrouter]
   api_key = "sk-or-v1-..."

   [providers.anthropic]
   api_key = "sk-ant-..."

   [providers.openai]
   api_key = "sk-..."

   [providers.copilot]
   # source = "github-cli"  ← read from gh's stored token
   # source = "env"
   token = "..."

   [providers.ollama]
   base_url = "http://localhost:11434"
   ```
3. **OS keychain** (v2): for serious deployments. Out of scope for v1.

AgentWatch surface:
- `/auth` slash command: prints which providers are detected (✓/✗) and
  where each key was found (env vs `auth.toml`). Never prints the key
  itself.
- New **AUTH** panel on the Cost tab footer or as a dedicated tab if we
  reshuffle the tab order.

## UI changes

### Models tab — group by provider

```
[7] Models                                                  10 in use today

 sort  spend ↓   calls   latency   success                    via 3 providers

╭─ MODELS  10 in use today ──────────────────────────────────────────────────╮
│   PROVIDER     MODEL                          CALLS   $TODAY   p50    SUCC│
│   anthropic    claude-sonnet-4                  82    $1.20    1.2s   99% │
│   openrouter   anthropic/claude-sonnet-4        42    $0.60    1.4s   98% │
│   openai       openai/gpt-4o                    84    $0.18    0.6s   99% │
│   ollama       llama3.3:70b                     22    $0.00    4.4s   88% │
│   copilot      gpt-4o-copilot                   18    sub      0.9s   97% │
╰────────────────────────────────────────────────────────────────────────────╯
```

### Cost tab — by-provider stack

Add a third panel next to BY MODEL / BY AGENT:
```
╭─ BY PROVIDER  today  $2.34 ─────╮
│ anthropic    $1.20  ██████   51% │
│ openrouter   $0.66  ████     28% │
│ openai       $0.48  ██       21% │
│ copilot      sub.   ░░░        — │
│ ollama       $0.00  ░          0% │
╰──────────────────────────────────╯
```

### Team panel — provider badge per member

```
 ● coder      ×2  claude-sonnet-4   ~$0.024   [an]
 ● tester         deepseek/v3       ~$0.002   [or]
 ● docs           llama3.3:70b      $0.000    [ol]
 ● reviewer       gpt-4o-copilot    sub.      [co]
 ● oracle         gpt-4o-mini       ~$0.001   [op]
```

`[or]` `[an]` `[op]` `[ol]` `[co]` = openrouter / anthropic / openai /
ollama / copilot provider tags, color-coded.

### Status strip — active providers

Console row 3 gets `  via openrouter+openai+ollama` after `workspace`.

## Phased delivery

### Phase 1 — AgentWatch foundation (no neo PR needed)
- [ ] Extend `pricing.rs` to a multi-provider table: `(provider, model) → ModelPrice`
- [ ] `provider_for(model)` heuristic with manual overrides
- [ ] Team panel shows provider badge
- [ ] `/auth` slash command (reads env, doesn't depend on neo)
- [ ] Models tab gains a PROVIDER column
- [ ] Cost tab gets a BY PROVIDER panel

### Phase 2 — neo: provider abstraction + first new provider
- [ ] `trait LlmProvider` in `neo/src/api/`
- [ ] `OpenRouterProvider` (refactor of today's client) + `OllamaProvider` impl
- [ ] `ProviderRegistry` keyed by name, populated from `auth.toml`
- [ ] Router gains a `select_provider(model)` step before `select_model`
- [ ] `InvocationRecord.provider` populated from the chosen provider
- [ ] Backwards-compat: missing `auth.toml` falls back to env-var-only
      OpenRouter (today's behaviour)

### Phase 3 — Anthropic direct
- [ ] `AnthropicProvider` impl with top-level `system` field handling
- [ ] Prompt-cache headers wired through so multi-step pipelines benefit
- [ ] Routing precedence: anthropic direct beats `anthropic/...` via
      OpenRouter when both keys are present

### Phase 4 — OpenAI direct
- [ ] `OpenAIProvider` impl (largely a parameterised OpenRouter client)
- [ ] Routing precedence as above for `openai/...` models

### Phase 5 — Copilot
- [ ] `CopilotProvider` with token refresh
- [ ] `gh auth status --show-token` integration as default token source
- [ ] Subscription marker in pricing table (cost = 0, but tier = `sub.`)
- [ ] Capability tags: `tool_use: limited` for Copilot models

### Phase 6 — polish
- [ ] OS keychain support
- [ ] In-app `/auth set <provider> <key>` (with confirmation; writes
      to `auth.toml`)
- [ ] Per-provider rate-limit awareness (back-off when 429)
- [ ] OpenRouter live-pricing fetch (already planned, gets reused)

## Open questions

1. **Routing precedence when multiple providers ship the same model.**
   If both OpenRouter (`anthropic/claude-sonnet-4`) and OpenAI direct
   (`openai/gpt-4o`) are configured, who wins? Default proposal: direct
   provider beats aggregator, with a per-model override in `auth.toml`.
2. **Cost reporting for Copilot.** "Subscription" hides marginal cost.
   Do we show `$0` or `sub.` in the same column? Proposal: `sub.` with a
   separate "subscription roster" panel listing what's been paid for.
3. **Ollama model discovery.** Hardcode common models or hit
   `/api/tags`? Proposal: hit `/api/tags` on startup and cache.
4. **Tool-use degradation.** When Copilot is selected for an agent that
   needs tools, fall back or fail? Proposal: warn in AgentWatch's
   Insights tab and let the router pick a different model.
5. **Per-team provider preference.** Should `/team set coder ollama:*`
   mean "any ollama model"? Proposal: yes, with a wildcard syntax.

## What ships first

The most user-facing improvement is **Ollama**, because it's free and
removes the API-key gate for first-time users. Recommended slice:

1. Phase 1 foundation in AgentWatch (small, ships immediately)
2. Phase 2 neo PR for the trait + Ollama
3. Demo: `/team set coder ollama:deepseek-coder:33b`, hit submit, watch
   the lens scan with `$0.00` cost ticking through

Then **Anthropic direct** is the next-easiest because AgentWatch's
pricing table is already authored against Anthropic's published rates —
no UI changes beyond the provider badge. After that, OpenAI direct and
Copilot follow as separate neo PRs.
