# AgentWatch demo — VHS script
#
# Renders demo.gif: open agentwatch, pick the cheap "oracle" workflow,
# ask a real question, watch the R2 lens scan while neo streams the
# response, then cycle through observer tabs to show the spend and
# insights light up.
#
# Run:   vhs demo.tape
# Needs: OPENROUTER_API_KEY in env (sourced via ~/.zshrc),
#        a built agentwatch binary at ./target/release/agentwatch,
#        a neo binary findable on PATH or in ~/projects/active/neo/target/release/.

Output demo.gif

# bash, not zsh — p10k's instant-prompt warning clobbers the demo and
# trips crossterm's input reader. OPENROUTER_API_KEY is inherited from
# whatever shell ran `vhs demo.tape`.
Set Shell "bash"
Set FontSize 14
Set Width 1280
Set Height 760
Set Theme "Catppuccin Mocha"
Set TypingSpeed 28ms
Set Framerate 18
Set Padding 10

# --- Boot ----------------------------------------------------------------
# Pre-select the cheap "oracle" workflow via --workflow so we don't burn
# budget on the full feature-build pipeline. Land on [1] Thread (default).
Hide
Type "./target/release/agentwatch --workflow oracle"
Enter
Sleep 2s
Show

# Land on [1] Thread — the R2 bot is idle, lens steady, oracle pre-selected.
Sleep 3s

# --- Type a real prompt -------------------------------------------------
Type "what makes a great CLI tool? one paragraph."
Sleep 1500ms
Enter

# --- Neo runs ----------------------------------------------------------
# Lens starts scanning, hologram dots fade in, status text on the right
# of the bot ticks the seconds. Streaming output appears above.
Sleep 14s

# --- Cycle through observer tabs ----------------------------------------
# Tab key cycles forward: Thread → Console → Agents → Plans → Sessions →
# Tools → Models → Cost → Overview → Insights → Thread.
# We dwell on the high-impact ones.

# Console — full instrument panel showing the same submission
Tab
Sleep 3s

# Agents — the agent that ran lights up green
Tab
Sleep 3s

# Plans — pseudo-pipeline reconstructed from the invocation
Tab
Sleep 3s

# Skip Sessions and Tools, jump to Models
Tab
Tab
Tab
Sleep 3s

# Cost — gauges tick, by-model bar appears
Tab
Sleep 3s

# Overview — one-screen rollup of everything
Tab
Sleep 3s

# Insights — rule engine cards
Tab
Sleep 3s

# Back home with one more Tab and quit
Tab
Sleep 2s
Escape
Sleep 500ms
Escape
Sleep 1s
