# Team Builder — Hero Panel Design

## Goal

A dedicated, full-screen team-building experience that makes assembling
a dev team feel like building a roster in a strategy game — intuitive,
dynamic, with every cost and trade-off visible in real time.

Today's surface (slash commands + small right-rail panel) is functional
but discovery-poor. Users have to know `/team add coder
anthropic/claude-sonnet-4 2` exists. The Hero Panel replaces it with a
keyboard-driven editor where every action is visible.

---

## Activation

Three entry points, all returning to wherever you came from on exit:

1. **`[T]` hotkey** — from any tab. Single-key launch, matching the
   existing `[1]`–`[0]` digit pattern.
2. **`/team edit`** — slash command, for muscle-memory users.
3. **Console PIPELINE rail** — pressing `Enter` while the team panel
   has focus (future, requires focus management on Console).

Exit is always **Esc** (with optional confirmation if there are unsaved
changes) or **`s`** to save and exit.

---

## Layout (130 × 30 content area)

```
╔═════════════════════════════════════════════════════════════════════════════╗
║ TEAM BUILDER  ▸ my-team*                  unsaved · esc cancel · s save     ║
╠═════════════════════════════════════════════════════════════════════════════╣
║                                                                             ║
║ ┌─ ROSTER  5 active / 8 total ─────────────────┐ ┌─ PRESETS ──────────────┐ ║
║ │ ▸ ✓ router      ×1  [or] (router)            │ │ ● balanced  5 · $$    │ ║
║ │   ✓ planner     ×1  [an] claude-sonnet-4     │ │ ● lean      2 · $     │ ║
║ │   ✓ coder       ×2  [an] claude-sonnet-4     │ │ ● scaled    6 · $$$   │ ║
║ │   □ debugger    ×1  [or] (router)            │ │ ● full      8 · $$    │ ║
║ │   ✓ tester      ×1  [op] gpt-4o-mini         │ │ ● local     1 · 0     │ ║
║ │   ✓ reviewer    ×1  [an] claude-opus-4       │ │ ▸ my-team*  5 · $$    │ ║
║ │   □ documenter  ×1  [ol] llama3.2:latest     │ │                       │ ║
║ │   □ oracle      ×1  [or] (router)            │ │ + new from current    │ ║
║ └──────────────────────────────────────────────┘ └───────────────────────┘ ║
║                                                                             ║
║ ┌─ SELECTED  coder  ─────────────────────────┐ ┌─ COST PREVIEW  per task ─┐ ║
║ │ status  ✓ included      count  ×2          │ │                          │ ║
║ │                                            │ │  router    × auto  ~free │ ║
║ │ MODEL  ▸ anthropic/claude-sonnet-4 [an]    │ │  planner   × auto  $0.012│ ║
║ │          anthropic/claude-opus-4   [an]    │ │  coder ×2  × sonnet$0.048│ ║
║ │          openai/gpt-4o             [op]    │ │  tester    × mini  $0.001│ ║
║ │          openai/gpt-4o-mini        [op]    │ │  reviewer  × opus  $0.135│ ║
║ │          deepseek/v3               [or]    │ │  ─────────────────────── │ ║
║ │          llama3.2:latest           [ol]    │ │  TOTAL              $0.196│ ║
║ │          (router auto)                     │ │                          │ ║
║ │                                            │ │  tier: $$  (≤ $0.20)    │ ║
║ └────────────────────────────────────────────┘ └──────────────────────────┘ ║
║                                                                             ║
║ STRENGTHS   code-heavy  parallel-coders  review-strict                      ║
║ WARNINGS    no debugger — /bug-hunt workflow disabled                       ║
║             reviewer model is premium — high-cost reviews                   ║
║                                                                             ║
╠═════════════════════════════════════════════════════════════════════════════╣
║ ↑↓ navigate · tab cycle pane · space toggle · +/- count · m models · s save║
╚═════════════════════════════════════════════════════════════════════════════╝
```

### Sections

| ID | Position | Purpose |
|---|---|---|
| **Header** | row 1 | Team name (with `*` when dirty), unsaved-warning, save/cancel hint |
| **Roster** | rows 3-11, left 60% | All 8 built-in agents with include/exclude state, count multiplier, model badge |
| **Presets** | rows 3-11, right 40% | List of available teams (presets first, user-defined below); arrows + Enter loads into editor |
| **Selected** | rows 13-21, left 60% | Detail + model picker for the highlighted roster row |
| **Cost preview** | rows 13-21, right 40% | Live cost-per-task breakdown, total, tier glyph |
| **Strengths/warnings** | rows 22-24 | Heuristic-driven feedback as the user edits |
| **Footer** | row 26 | Keybinding reference |

---

## Interaction model

### Focus groups

The cursor can be in one of three focus groups:

1. **`Roster`** — default focus. ↑/↓ moves between agents. Space toggles
   include/exclude. `+`/`-` adjusts count. Highlighted row drives what
   the SELECTED panel shows.
2. **`Models`** — toggled by pressing `m` from Roster. Up/Down scrolls
   the model picker. Enter selects + returns focus to Roster.
3. **`Presets`** — toggled by pressing `p`. Up/Down moves selection.
   Enter loads the preset into the editor (offering to discard unsaved
   changes if dirty).

`Tab` cycles forward through focus groups. `Shift+Tab` cycles back.

### Keybindings

| Key | In Roster | In Models | In Presets |
|---|---|---|---|
| `↑/↓` or `j/k` | move row | scroll list | move selection |
| `space` | toggle include | (n/a) | (n/a) |
| `+` / `-` | count ± 1 (min 1, max 5) | (n/a) | (n/a) |
| `m` | enter Models focus | exit to Roster | (n/a) |
| `p` | enter Presets focus | (n/a) | exit to Roster |
| `enter` | (no-op or open Models) | apply model | load preset |
| `r` | reset to default `auto` model | (n/a) | (n/a) |
| `tab` / `shift+tab` | cycle focus | cycle focus | cycle focus |
| `s` | save (prompt for name if new) | save | save |
| `d` | delete current team (only user teams) | (n/a) | delete preset |
| `n` | start fresh — empty editor | (n/a) | (n/a) |
| `esc` | exit (confirm if dirty) | exit Models | exit Presets |

### Dirty state

- Any change marks the team `dirty` (header shows `team-name*`).
- Exit-on-dirty triggers a confirm toast: `unsaved changes — esc again
  to discard, s to save`.

### Save flow

- **Existing user team selected**: `s` saves in place.
- **Preset selected**: `s` prompts for a new name (presets are read-only).
  Input shows in the header: `save as ▸ ▌` (cursor).
- **Brand-new team via `n`**: `s` prompts for a name.

---

## Live cost computation

For each call, the team builder picks a *typical-task budget*:

- `prompt_size`: 3,000 input tokens (configurable later)
- `response_size`: 800 output tokens

Each included member contributes `count × cost(model, prompt, response)`:

- `model == "auto"` → use `claude-sonnet-4` as the auto-default
  reference (sensible router pick for most tasks)
- known model in pricing table → exact price × count
- `[co]` copilot → contributes `0` but shows `sub.` in the breakdown
- `[ol]` ollama → contributes `0` (free, local)

The total updates on every keystroke. A tier glyph at the bottom:

| Tier | Total per task | Glyph |
|---|---|---|
| `$` | < $0.05 | green |
| `$$` | $0.05–0.20 | cyan |
| `$$$` | $0.20–1.00 | yellow |
| `$$$$` | ≥ $1.00 | red |

Sub-line: `daily projection at 50 tasks/day: $X.XX` so the user sees the
real budget impact of their composition.

---

## Strength / warning heuristics

Pure functions over the current team state. Examples:

**Strengths** (positive tags shown in cyan):
- `code-heavy` — coder.count ≥ 2 OR coder model is sonnet/opus
- `parallel-coders` — coder.count ≥ 2
- `review-strict` — reviewer included AND reviewer model in [sonnet, opus, o3]
- `code-quality` — both reviewer AND tester included
- `documented` — documenter included
- `cost-aware` — total per task ≤ $0.05
- `private` — every model is Ollama (no network)
- `parallel-pipeline` — multiple roles with count ≥ 2

**Warnings** (shown in yellow):
- `no debugger — /bug-hunt workflow disabled`
- `no planner — complex tasks may fail`
- `no reviewer — code lands unreviewed`
- `reviewer model is premium — high-cost reviews`
- `coder count > 1 but no planner — parallel work needs a plan`
- `all-Copilot team — tool use may be limited`
- `mixed-provider — need both OPENROUTER_API_KEY and ANTHROPIC_API_KEY`

**Critical** (shown in red):
- `no coder — task cannot execute`
- `team is empty`

---

## State machine

```
                  ┌───────────────────┐
                  │   inactive        │
                  │   (other tab)     │
                  └──────────┬────────┘
                             │ T / /team edit
                             ▼
                  ┌───────────────────┐
                  │  Builder.Roster   │◄────┐
                  │  (default focus)  │     │
                  └────┬─────────┬────┘     │
                       │         │          │
                  m    │         │  p       │ esc
                       ▼         ▼          │
            ┌──────────────┐ ┌──────────────┴───┐
            │ Models picker│ │  Presets picker  │
            └──────┬───────┘ └──────────────────┘
                   │ enter / esc                 ▲
                   ▼                             │
              save flow                   ┌──────┴──────┐
              (if dirty)        ◄─────────┤ confirm     │
                                          │ discard?    │
                                          └─────────────┘
```

---

## Data model changes

```rust
pub struct Team {
    pub name: String,
    pub blurb: String,
    pub members: Vec<TeamMember>,
    pub is_preset: bool,            // NEW — disables in-place edit
    pub tags: Vec<String>,          // NEW — auto-derived strengths
}

pub struct TeamMember {
    pub agent: String,
    pub model: String,
    pub count: u8,
    pub included: bool,             // NEW — currently implicit from list
    pub notes: Option<String>,      // NEW — user freeform note
}
```

Builder state (transient, not persisted):

```rust
pub struct BuilderState {
    pub editing: Team,              // working copy
    pub original: Team,              // for revert
    pub focus: BuilderFocus,
    pub roster_idx: usize,
    pub models_idx: usize,
    pub presets_idx: usize,
    pub dirty: bool,
    pub naming: Option<String>,     // partial name when in "save as" flow
    pub last_esc_at: Option<SystemTime>, // for "esc again to discard"
}

pub enum BuilderFocus {
    Roster,
    Models,
    Presets,
    Naming,
}
```

---

## Model picker source

The picker pulls from `pricing::models_by_provider()` (already exists),
showing every known (provider, model) pair grouped by provider. Each
row carries the provider badge and the per-Mtoken rate so the user can
compare costs at a glance:

```
 ┌─ MODELS ──────────────────────────────────────────────┐
 │  ANTHROPIC                                            │
 │    claude-sonnet-4    [an]    $3 / $15 per Mtok       │
 │    claude-opus-4      [an]    $15 / $75               │
 │    claude-3.5-haiku   [an]    $0.80 / $4              │
 │                                                       │
 │  OPENAI                                               │
 │    gpt-4o             [op]    $2.50 / $10             │
 │    gpt-4o-mini        [op]    $0.15 / $0.60           │
 │    o3                 [op]    $10 / $30               │
 │                                                       │
 │  OPENROUTER                                           │
 │    deepseek/v3        [or]    $0.27 / $1.10           │
 │    …                                                  │
 │                                                       │
 │  OLLAMA  (local, free)                                │
 │    llama3.2:latest    [ol]    $0 — only if pulled     │
 │                                                       │
 │  COPILOT  (subscription)                              │
 │    gpt-4o-copilot     [co]    sub.                    │
 └───────────────────────────────────────────────────────┘
```

Local Ollama models that the user has pulled (detected via
`ollama list`) are tagged with a green `●` instead of `○` to show
they're available offline.

---

## Implementation plan (phased)

### Phase A — Foundation (no UI rendering yet)
- [ ] Extend `Team` and `TeamMember` with `is_preset`, `included`,
      `tags`, `notes`
- [ ] Migrate the 5 presets to set `is_preset = true`
- [ ] Backwards-compat: existing user teams loaded from TOML default
      `is_preset = false`, `included = true`, `tags = []`, `notes = None`
- [ ] `tags_for(&Team) -> Vec<String>` strength heuristic in new
      `data/team_tags.rs`
- [ ] `warnings_for(&Team) -> Vec<TeamWarning>` heuristic alongside
- [ ] Unit tests with fixture teams covering each tag/warning

### Phase B — Builder state machine
- [ ] `app::builder::BuilderState` + `BuilderFocus`
- [ ] `App::open_builder()` / `App::close_builder()` —
      transient, doesn't touch `current_tab`
- [ ] `App.builder: Option<BuilderState>` — when Some, the builder
      overlays the rest of the UI
- [ ] Key handling: when builder is open, route via a dedicated
      `builder_key()` instead of the per-tab handler
- [ ] No rendering yet — purely state

### Phase C — Roster + Presets rendering
- [ ] `ui::team_builder` module
- [ ] Full-screen overlay (130 × 30) renders on top of everything when
      `app.builder.is_some()`
- [ ] ROSTER panel with checkbox / count / model badge per agent
- [ ] PRESETS panel listing all teams (presets + user) with cost tier
- [ ] Footer with keybinding hint

### Phase D — Selected + Cost preview + Model picker
- [ ] SELECTED panel showing the roster's highlighted agent
- [ ] Model picker (when focus is `Models`) listing
      `pricing::models_by_provider()` with provider headers
- [ ] COST PREVIEW with per-member line, total, tier glyph, daily
      projection
- [ ] Live update on every state change

### Phase E — Save / load / preset operations
- [ ] Naming flow ("save as ▸ ▌" mini-input in the header)
- [ ] Preset load with "unsaved changes" confirm
- [ ] Delete user teams from the preset list
- [ ] `n` starts a fresh team

### Phase F — Strengths / warnings + polish
- [ ] STRENGTHS row pulls from `tags_for`
- [ ] WARNINGS row pulls from `warnings_for`
- [ ] Animated dirty-state indicator
- [ ] Ollama "pulled locally" detection via `ollama list` cache
- [ ] `?` overlay with extended help

### Phase G — Activation hookups
- [ ] `T` hotkey routed in `event.rs` global handler
- [ ] `/team edit` slash command opens the builder
- [ ] Confirm-on-dirty on tab switch / quit while open

---

## Open questions

1. **Should the builder live in its own tab or as a modal?**
   Modal (this plan) keeps the existing 10-tab layout intact and gives
   the builder a clear "I'm focused here right now" mode. Tab is
   simpler to implement but eats slot space. **Proposal: modal.**

2. **Ollama-pulled detection — when do we refresh?**
   Once per builder open is probably enough. Cache for the duration of
   the agentwatch session. Use `ollama list` parsed into a set.

3. **Per-agent role notes (the `notes` field) — surface where?**
   Show in SELECTED panel as a free-text field editable on the `e` key.
   For v1 keep it read-only; editing is Phase 2 polish.

4. **Multi-select roster ops?** e.g. "toggle all coders to opus".
   Probably over-engineered for v1. Defer.

5. **Workflow tagging per team?**
   A team could declare which workflows it supports (e.g. "this team
   has no debugger, so /bug-hunt is unavailable"). Useful but creates
   coupling between workflow choice and team. **Proposal: defer to
   v2 — surface a soft warning today instead.**

6. **Live OpenRouter pricing fetch?**
   Already in PROVIDERS.md follow-ups. Until it lands the cost preview
   uses hardcoded rates. Cost numbers carry an `(est.)` suffix.

7. **Should the builder persist its "last-opened" state?**
   Probably yes — reopening should resume editing the team you were
   building. Store builder state in `~/.config/agentwatch/builder.toml`
   if `is_dirty`. Discard on save or cancel.

---

## What ships first

The slice that delivers most visible value with least implementation cost:

1. **Phases A + B + C**: data model + state machine + roster/presets
   rendering. User sees the panel, can navigate, can toggle agents
   in/out and load presets. Save still uses slash commands.
2. **Phase D**: model picker + live cost. The "hero" moment.
3. **Phases E + F**: save flow + strengths/warnings.
4. **Phase G**: activation hookups (T hotkey, /team edit, confirms).

If we want to demo earlier, ship Phases A–C as a single PR. The builder
is rough but real — users can see their team composition visually,
toggle members, and pick presets. Models and live cost follow.
