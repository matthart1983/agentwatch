# AgentWatch demo — VHS script
#
# Renders demo.gif: open agentwatch, type a pun, watch the R2 lens scan
# while neo streams a thoughtful response, then tab through Cost / Insights
# / Overview to show the live data lighting up.
#
# Run:   vhs demo.tape
# Needs: OPENROUTER_API_KEY in env (inherited from parent shell)
#        a built agentwatch binary at ./target/release/agentwatch
#        a built neo binary at ~/projects/active/neo/target/release/neo

Output demo.gif

# bash, not zsh — p10k's instant-prompt warning clobbers the demo and
# trips crossterm's input reader.
Set Shell "bash"
Set FontSize 14
Set Width 1280
Set Height 760
Set Theme "Catppuccin Mocha"
Set TypingSpeed 32ms
Set Framerate 18
Set Padding 10

# --- Boot ---------------------------------------------------------------
# Pre-select the oracle workflow via --workflow so the demo doesn't kick
# off the full feature-build pipeline.
Hide
Type "./target/release/agentwatch --workflow oracle"
Enter
Sleep 2s
Show

# Land on [1] Thread. The R2 bot reads "lens warmed · awaiting orders".
Sleep 3s

# --- Type a memorable prompt --------------------------------------------
# The pun lands either way: even without knowing the engine is named neo,
# it's a meta question to the AI. With the context it's a free pun.
Type "neo, what is the matrix?"
Sleep 1500ms
Enter

# --- Neo runs ----------------------------------------------------------
# Lens scans, hologram dots fade in, status text on the right of the bot
# ticks the seconds. The response streams into the transcript above.
# We give it room to finish (claude-sonnet-4 + tool use ≈ 20–25s).
Sleep 26s

# --- Spend ticking up --------------------------------------------------
# Tab cycles forward: Thread → Console → Agents → Plans → Sessions →
# Tools → Models → Cost. Seven Tabs to reach Cost from Thread.
Tab
Tab
Tab
Tab
Tab
Tab
Tab
Sleep 4s

# --- Overview rollup ---------------------------------------------------
Tab
Sleep 4s

# --- Insights light up -------------------------------------------------
Tab
Sleep 4s

# --- Back home, response still in the transcript ------------------------
Tab
Sleep 3s

# --- Quit gracefully ---------------------------------------------------
Escape
Sleep 400ms
Escape
Sleep 800ms
