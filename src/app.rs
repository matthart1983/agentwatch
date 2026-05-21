use std::path::PathBuf;
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use tui_textarea::TextArea;

use crate::data::{
    contract::ControlCommand, invocations, team, threads, threads::FullThread, InvocationStore,
    Team, TeamMember, ThreadSummary,
};
use crate::driver::{InboxWriter, JobEvent, JobId, LineSource, Runner};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Console,
    Thread,
    Agents,
    Plans,
    Sessions,
    Tools,
    Models,
    Cost,
    Overview,
    Insights,
}

impl Tab {
    pub const ALL: [Tab; 10] = [
        Tab::Thread,
        Tab::Console,
        Tab::Agents,
        Tab::Plans,
        Tab::Sessions,
        Tab::Tools,
        Tab::Models,
        Tab::Cost,
        Tab::Overview,
        Tab::Insights,
    ];

    pub fn from_index(n: u8) -> Self {
        match n {
            1 => Tab::Thread,
            2 => Tab::Console,
            3 => Tab::Agents,
            4 => Tab::Plans,
            5 => Tab::Sessions,
            6 => Tab::Tools,
            7 => Tab::Models,
            8 => Tab::Cost,
            9 => Tab::Overview,
            0 => Tab::Insights,
            _ => Tab::Thread,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Tab::Thread => "Thread",
            Tab::Console => "Console",
            Tab::Agents => "Agents",
            Tab::Plans => "Plans",
            Tab::Sessions => "Sessions",
            Tab::Tools => "Tools",
            Tab::Models => "Models",
            Tab::Cost => "Cost",
            Tab::Overview => "Overview",
            Tab::Insights => "Insights",
        }
    }

    pub fn footer_digit(&self) -> char {
        match self {
            Tab::Thread => '1',
            Tab::Console => '2',
            Tab::Agents => '3',
            Tab::Plans => '4',
            Tab::Sessions => '5',
            Tab::Tools => '6',
            Tab::Models => '7',
            Tab::Cost => '8',
            Tab::Overview => '9',
            Tab::Insights => '0',
        }
    }

    pub fn next(&self) -> Self {
        let i = Self::ALL.iter().position(|t| t == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }

    pub fn prev(&self) -> Self {
        let i = Self::ALL.iter().position(|t| t == self).unwrap_or(0);
        Self::ALL[(i + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

pub struct App {
    pub current_tab: Tab,
    pub started_at: SystemTime,
    pub runtime_online: bool,
    pub threads: Vec<ThreadSummary>,
    pub sessions_selected: usize,
    pub invocations: InvocationStore,
    pub models_selected: usize,
    pub agents_selected: usize,
    pub workspace: PathBuf,
    pub workflow: usize,
    pub prompt: TextArea<'static>,
    pub submitted: Vec<SubmittedPrompt>,
    pub last_submit_status: Option<SubmitStatus>,
    pub inbox: Option<InboxWriter>,
    pub runner: Runner,
    /// Monotonic frame counter — bumped on every tick. Used for spinner
    /// animation and to throttle the invocations.jsonl auto-reload.
    pub frame: u64,
    /// Set by `/quit`-style slash commands. Main loop polls and breaks.
    pub should_quit: bool,
    /// Last toast — a one-liner displayed in the transcript briefly after
    /// slash commands so the user gets feedback without it being noisy.
    pub toast: Option<Toast>,
    /// Available team presets, loaded on startup. Index 0 is the default.
    pub teams: Vec<Team>,
    pub active_team: usize,
    /// Highlighted index in the slash-completion popup. Reset to 0 every
    /// time the filter list changes shape.
    pub slash_popup_idx: usize,
    /// Hero Panel state. `Some` while the team builder overlay is open.
    pub builder: Option<crate::builder::BuilderState>,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub text: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct SubmittedPrompt {
    pub at: DateTime<Utc>,
    pub workflow: String,
    pub text: String,
    pub delivered: bool,
    pub job_id: Option<JobId>,
    pub command: Option<String>,
    pub response: Vec<ResponseLine>,
    pub completed: Option<JobCompletion>,
}

#[derive(Debug, Clone)]
pub struct ResponseLine {
    pub source: LineSource,
    pub text: String,
}

#[derive(Debug, Clone)]
pub enum JobCompletion {
    Success,
    Failure { code: Option<i32> },
    SpawnError { reason: String },
}

#[derive(Debug, Clone)]
pub enum SubmitStatus {
    Sent { workflow: String, at: DateTime<Utc> },
    Failed { reason: String, at: DateTime<Utc> },
}

impl App {
    pub fn new() -> Self {
        let threads = threads::load_all().unwrap_or_default();
        let invocations = InvocationStore::load().unwrap_or(InvocationStore { records: Vec::new() });
        let workspace = std::env::current_dir().unwrap_or_default();
        let mut prompt = TextArea::default();
        prompt.set_cursor_line_style(Default::default());
        let inbox = InboxWriter::new().ok();
        let runner = Runner::new();
        let mut teams = Team::presets();
        let persisted = team::load_teams_file();
        // Append user-defined teams (skip dupes if a user team somehow
        // shares a name with a preset; preset wins).
        for ut in persisted.teams {
            if !teams.iter().any(|t| t.name == ut.name) {
                teams.push(ut);
            }
        }
        let active_team = persisted
            .active
            .and_then(|n| teams.iter().position(|t| t.name == n))
            .unwrap_or(0);
        Self {
            current_tab: Tab::Thread,
            started_at: SystemTime::now(),
            runtime_online: false,
            threads,
            sessions_selected: 0,
            invocations,
            models_selected: 0,
            agents_selected: 0,
            workspace,
            workflow: 0,
            prompt,
            submitted: Vec::new(),
            last_submit_status: None,
            inbox,
            runner,
            frame: 0,
            should_quit: false,
            toast: None,
            teams,
            active_team,
            slash_popup_idx: 0,
            builder: None,
        }
    }

    /// Open the Hero Panel pointed at the currently active team.
    pub fn open_builder(&mut self) {
        let team = self.current_team().clone();
        let presets_idx = self.active_team;
        self.builder = Some(crate::builder::BuilderState::new(team, presets_idx));
    }

    pub fn close_builder(&mut self) {
        self.builder = None;
    }

    pub fn builder_is_open(&self) -> bool {
        self.builder.is_some()
    }

    /// Route a keystroke when the builder overlay is open. Returns true
    /// if the keystroke triggered an exit so the main loop can break.
    pub fn handle_builder_key(&mut self, k: crossterm::event::KeyEvent) {
        use crossterm::event::{KeyCode, KeyModifiers};
        use crate::builder::BuilderFocus;
        let Some(b) = self.builder.as_mut() else {
            return;
        };

        let now = std::time::SystemTime::now();
        let two_esc = b
            .last_esc_at
            .and_then(|t| now.duration_since(t).ok())
            .map(|d| d.as_secs() < 2)
            .unwrap_or(false);

        match (k.code, k.modifiers) {
            // ── Esc — context-sensitive ─────────────────────────────
            (KeyCode::Esc, _) => match b.focus {
                BuilderFocus::Models | BuilderFocus::Presets => {
                    b.focus = BuilderFocus::Roster;
                }
                BuilderFocus::Naming => {
                    b.naming = None;
                    b.focus = BuilderFocus::Roster;
                }
                BuilderFocus::Roster => {
                    if b.is_dirty() && !two_esc {
                        b.last_esc_at = Some(now);
                        self.set_toast(
                            "unsaved changes — press esc again to discard, s to save",
                        );
                    } else {
                        self.close_builder();
                    }
                }
            },

            // ── Roster ↔ Models ↔ Presets focus cycling ─────────────
            (KeyCode::Tab, KeyModifiers::NONE) => b.focus_next(),
            (KeyCode::BackTab, _) => b.focus_prev(),
            (KeyCode::Char('m'), KeyModifiers::NONE) => {
                b.focus = match b.focus {
                    BuilderFocus::Models => BuilderFocus::Roster,
                    _ => BuilderFocus::Models,
                };
                b.models_idx = 0;
            }
            (KeyCode::Char('p'), KeyModifiers::NONE) => {
                b.focus = match b.focus {
                    BuilderFocus::Presets => BuilderFocus::Roster,
                    _ => BuilderFocus::Presets,
                };
            }

            // ── Roster-focus actions ─────────────────────────────────
            (KeyCode::Up, _) | (KeyCode::Char('k'), KeyModifiers::NONE)
                if b.focus == BuilderFocus::Roster =>
            {
                b.roster_up()
            }
            (KeyCode::Down, _) | (KeyCode::Char('j'), KeyModifiers::NONE)
                if b.focus == BuilderFocus::Roster =>
            {
                b.roster_down()
            }
            (KeyCode::Char(' '), _) if b.focus == BuilderFocus::Roster => {
                b.toggle_included()
            }
            (KeyCode::Char('+'), _) | (KeyCode::Char('='), _)
                if b.focus == BuilderFocus::Roster =>
            {
                b.adjust_count(1)
            }
            (KeyCode::Char('-'), _) | (KeyCode::Char('_'), _)
                if b.focus == BuilderFocus::Roster =>
            {
                b.adjust_count(-1)
            }
            (KeyCode::Char('r'), KeyModifiers::NONE)
                if b.focus == BuilderFocus::Roster =>
            {
                b.set_selected_model("auto");
            }

            // ── Models-focus actions ─────────────────────────────────
            (KeyCode::Up, _) | (KeyCode::Char('k'), KeyModifiers::NONE)
                if b.focus == BuilderFocus::Models =>
            {
                b.models_idx = b.models_idx.saturating_sub(1);
            }
            (KeyCode::Down, _) | (KeyCode::Char('j'), KeyModifiers::NONE)
                if b.focus == BuilderFocus::Models =>
            {
                let n = model_picker_len();
                if b.models_idx + 1 < n {
                    b.models_idx += 1;
                }
            }
            (KeyCode::Enter, _) if b.focus == BuilderFocus::Models => {
                if let Some(model) = model_at(b.models_idx) {
                    b.set_selected_model(&model);
                    b.focus = BuilderFocus::Roster;
                }
            }

            // ── Presets-focus actions ────────────────────────────────
            (KeyCode::Up, _) | (KeyCode::Char('k'), KeyModifiers::NONE)
                if b.focus == BuilderFocus::Presets =>
            {
                b.presets_idx = b.presets_idx.saturating_sub(1);
            }
            (KeyCode::Down, _) | (KeyCode::Char('j'), KeyModifiers::NONE)
                if b.focus == BuilderFocus::Presets =>
            {
                if b.presets_idx + 1 < self.teams.len() {
                    b.presets_idx += 1;
                }
            }
            (KeyCode::Enter, _) if b.focus == BuilderFocus::Presets => {
                if let Some(t) = self.teams.get(b.presets_idx).cloned() {
                    b.editing = t.clone();
                    b.original = t;
                    b.focus = BuilderFocus::Roster;
                    b.roster_idx = 0;
                }
            }

            // ── Save / delete / new ──────────────────────────────────
            (KeyCode::Char('s'), KeyModifiers::NONE) => {
                self.builder_save();
            }
            (KeyCode::Char('d'), KeyModifiers::NONE)
                if b.focus == BuilderFocus::Presets =>
            {
                let name = self
                    .teams
                    .get(b.presets_idx)
                    .map(|t| t.name.clone())
                    .unwrap_or_default();
                if let Some(t) = self.teams.get(b.presets_idx) {
                    if t.is_preset {
                        self.set_toast("can't delete a built-in preset");
                    } else {
                        self.teams.remove(b.presets_idx);
                        if b.presets_idx >= self.teams.len() && b.presets_idx > 0 {
                            // ok, b is borrowed mut, can't touch self.teams here
                        }
                        self.persist_teams();
                        self.set_toast(&format!("deleted '{}'", name));
                    }
                }
            }
            (KeyCode::Char('n'), KeyModifiers::NONE) => {
                if let Some(b2) = self.builder.as_mut() {
                    b2.editing = crate::data::Team {
                        name: "untitled".to_string(),
                        blurb: "".to_string(),
                        members: Vec::new(),
                        is_preset: false,
                    };
                    b2.original = b2.editing.clone();
                    b2.focus = BuilderFocus::Roster;
                    b2.roster_idx = 0;
                }
            }

            _ => {}
        }
    }

    /// Save the working copy. If it's a preset, save-as with a "(copy)"
    /// suffix; if it's a user team, save in place.
    fn builder_save(&mut self) {
        let Some(b) = self.builder.as_mut() else {
            return;
        };
        let mut to_save = b.editing.clone();
        if to_save.is_preset {
            to_save.name = format!("{}-edit", to_save.name);
            to_save.is_preset = false;
        }
        let name = to_save.name.clone();
        if let Some(idx) = self.teams.iter().position(|t| t.name == name) {
            self.teams[idx] = to_save;
            self.active_team = idx;
        } else {
            self.teams.push(to_save);
            self.active_team = self.teams.len() - 1;
        }
        // Update the builder's snapshot so subsequent edits land on top
        // of the saved state.
        if let Some(b2) = self.builder.as_mut() {
            b2.original = b2.editing.clone();
            b2.original.is_preset = false;
            b2.editing.is_preset = false;
            b2.editing.name = name.clone();
            b2.original.name = name.clone();
            // presets_idx might be stale now — point at the new active team
            b2.presets_idx = self.active_team;
        }
        self.persist_teams();
        self.set_toast(&format!("saved '{}' and activated", name));
    }

    /// Is the user composing a slash command (prompt starts with `/`)?
    pub fn slash_mode(&self) -> bool {
        self.prompt
            .lines()
            .iter()
            .next()
            .map(|l| l.starts_with('/'))
            .unwrap_or(false)
    }

    /// Currently filtered slash-command matches for the typed prompt.
    pub fn slash_matches(&self) -> Vec<&'static SlashCmd> {
        let text = self.prompt.lines().join(" ");
        slash_matches(&text)
    }

    pub fn slash_popup_up(&mut self) {
        let matches = self.slash_matches();
        if matches.is_empty() {
            return;
        }
        self.slash_popup_idx = self.slash_popup_idx.saturating_sub(1);
    }

    pub fn slash_popup_down(&mut self) {
        let matches = self.slash_matches();
        if matches.is_empty() {
            return;
        }
        if self.slash_popup_idx + 1 < matches.len() {
            self.slash_popup_idx += 1;
        }
    }

    /// Replace the prompt with the highlighted completion + a space so the
    /// user can type the rest (e.g. arguments). Returns true if a completion
    /// was applied.
    pub fn slash_complete(&mut self) -> bool {
        let Some(cmd) = self
            .slash_matches()
            .get(self.slash_popup_idx)
            .copied()
        else {
            return false;
        };
        self.prompt = TextArea::default();
        self.prompt.set_cursor_line_style(Default::default());
        for ch in format!("/{} ", cmd.name).chars() {
            self.prompt
                .input(tui_textarea::Input { key: tui_textarea::Key::Char(ch), ctrl: false, alt: false, shift: false });
        }
        true
    }

    pub fn current_team(&self) -> &Team {
        &self.teams[self.active_team.min(self.teams.len() - 1)]
    }

    pub fn current_team_mut(&mut self) -> &mut Team {
        let idx = self.active_team.min(self.teams.len() - 1);
        &mut self.teams[idx]
    }

    pub fn workflow_name(&self) -> &'static str {
        WORKFLOWS[self.workflow.min(WORKFLOWS.len() - 1)].name
    }

    pub fn prompt_is_empty(&self) -> bool {
        self.prompt.lines().iter().all(|l| l.is_empty())
    }

    /// Read the prompt textarea, drop a `ControlCommand::Prompt` into neo's
    /// inbox AND spawn `neo` as a subprocess to actually execute the prompt
    /// so responses stream back to the UI today (without waiting for neo's
    /// ControlInbox wiring). Once that wiring lands we'll drop the spawn.
    ///
    /// A prompt that starts with `/` is intercepted as a slash command and
    /// executed locally instead of being sent to neo.
    pub fn submit_prompt(&mut self) {
        let text = self.prompt.lines().join("\n").trim().to_string();
        if text.is_empty() {
            return;
        }

        if text.starts_with('/') {
            self.dispatch_slash(&text);
            self.clear_prompt();
            return;
        }

        let (text, attachment_summary) = expand_attachments(&text, &self.workspace);
        if let Some(summary) = attachment_summary {
            self.set_toast(&summary);
        }

        // Session continuity — prepend prior turns from this agentwatch run
        // so neo can answer follow-ups in context. Capped to keep prompt
        // size sane.
        let outbound_text = wrap_with_session_context(&self.submitted, &text);

        let workflow = self.workflow_name().to_string();
        let at = Utc::now();

        // Always drop the inbox file too — it's our future-facing path and
        // costs nothing today (just creates a small JSON file on disk).
        let delivered = match self.inbox.as_ref() {
            Some(inbox) => inbox
                .send(&ControlCommand::Prompt {
                    workflow: workflow.clone(),
                    text: outbound_text.clone(),
                })
                .is_ok(),
            None => false,
        };

        // Spawn neo with the context-enriched prompt so neo sees the
        // full conversation, not just the latest user line. If the active
        // team has a non-auto model assignment we pass it via env so neo
        // respects the team's model preference.
        let override_model = self.current_team().override_model().map(|s| s.to_string());
        let job_id =
            self.runner
                .spawn(&workflow, &outbound_text, override_model.as_deref());
        self.last_submit_status = Some(SubmitStatus::Sent {
            workflow: workflow.clone(),
            at,
        });

        self.submitted.push(SubmittedPrompt {
            at,
            workflow,
            text,
            delivered,
            job_id: Some(job_id),
            command: None,
            response: Vec::new(),
            completed: None,
        });
        self.prompt = TextArea::default();
        self.prompt.set_cursor_line_style(Default::default());
    }

    /// Drain everything the runner has queued for us since the last tick
    /// and attach it to the correct SubmittedPrompt.
    fn drain_runner_events(&mut self) {
        while let Ok(event) = self.runner.rx.try_recv() {
            match event {
                JobEvent::Started { id, command } => {
                    if let Some(sp) = self.submitted.iter_mut().find(|s| s.job_id == Some(id)) {
                        sp.command = Some(command);
                    }
                }
                JobEvent::Line { id, source, text } => {
                    if let Some(sp) = self.submitted.iter_mut().find(|s| s.job_id == Some(id)) {
                        sp.response.push(ResponseLine { source, text });
                    }
                }
                JobEvent::Finished { id, status } => {
                    if let Some(sp) = self.submitted.iter_mut().find(|s| s.job_id == Some(id)) {
                        sp.completed = Some(match status {
                            Some(s) if s.success() => JobCompletion::Success,
                            Some(s) => JobCompletion::Failure { code: s.code() },
                            None => JobCompletion::Failure { code: None },
                        });
                    }
                }
                JobEvent::Failed { id, reason } => {
                    if let Some(sp) = self.submitted.iter_mut().find(|s| s.job_id == Some(id)) {
                        sp.completed = Some(JobCompletion::SpawnError { reason });
                    }
                }
                JobEvent::Cancelled { id } => {
                    if let Some(sp) = self.submitted.iter_mut().find(|s| s.job_id == Some(id)) {
                        // Mark immediately so the spinner stops. The follow-up
                        // Finished event (from the child reaping) is benign —
                        // we only set completed if it's still None.
                        if sp.completed.is_none() {
                            sp.completed = Some(JobCompletion::SpawnError {
                                reason: "cancelled".to_string(),
                            });
                        }
                    }
                }
            }
        }
    }

    pub fn clear_prompt(&mut self) {
        self.prompt = TextArea::default();
        self.prompt.set_cursor_line_style(Default::default());
    }

    /// Kill the most recently submitted prompt's subprocess if it's still
    /// running. The Runner emits a Cancelled event which `drain_runner_events`
    /// turns into `JobCompletion::SpawnError { reason: "cancelled" }`.
    pub fn cancel_running_job(&mut self) -> bool {
        let Some(sp) = self.submitted.last() else {
            return false;
        };
        if sp.completed.is_some() {
            return false;
        }
        let Some(id) = sp.job_id else {
            return false;
        };
        self.runner.cancel(id)
    }

    pub fn job_in_flight(&self) -> bool {
        matches!(self.submitted.last(), Some(sp) if sp.completed.is_none() && sp.job_id.is_some())
    }

    fn dispatch_slash(&mut self, raw: &str) {
        let cmd = raw.trim_start_matches('/').trim().to_lowercase();
        let (head, rest) = match cmd.split_once(' ') {
            Some((h, r)) => (h, r.trim()),
            None => (cmd.as_str(), ""),
        };
        match head {
            "quit" | "exit" | "q" => {
                self.should_quit = true;
            }
            "cancel" => {
                if self.cancel_running_job() {
                    self.set_toast("cancelled in-flight job");
                } else {
                    self.set_toast("no job to cancel");
                }
            }
            "clear" => {
                self.submitted.clear();
                self.set_toast("transcript cleared");
            }
            "threads" | "sessions" => {
                self.current_tab = Tab::Sessions;
            }
            "cost" => self.current_tab = Tab::Cost,
            "models" => self.current_tab = Tab::Models,
            "agents" => self.current_tab = Tab::Agents,
            "plans" => self.current_tab = Tab::Plans,
            "tools" => self.current_tab = Tab::Tools,
            "overview" => self.current_tab = Tab::Overview,
            "insights" => self.current_tab = Tab::Insights,
            "console" => self.current_tab = Tab::Console,
            "thread" | "home" => self.current_tab = Tab::Thread,
            "reload" => {
                self.reload_threads();
                self.set_toast("reloaded threads + invocations");
            }
            "team" => self.handle_team_cmd(rest),
            "auth" => self.handle_auth_cmd(),
            "resume" => {
                let fragment = rest.trim();
                let target_id = if fragment.is_empty() {
                    self.threads.first().map(|t| t.id.clone())
                } else {
                    self.threads
                        .iter()
                        .find(|t| t.id.contains(fragment))
                        .map(|t| t.id.clone())
                };
                match target_id {
                    Some(id) => match threads::load_one(&id) {
                        Ok(full) => self.resume_thread_full(full),
                        Err(e) => self.set_toast(&format!("couldn't load thread: {}", e)),
                    },
                    None => {
                        if fragment.is_empty() {
                            self.set_toast("no threads to resume");
                        } else {
                            self.set_toast(&format!("no thread matches '{}'", fragment));
                        }
                    }
                }
            }
            "help" | "?" => {
                self.set_toast(
                    "/cancel /clear /reload /quit · /threads /cost /models /agents /plans /tools /overview /insights /console /thread · /team [list|next|prev|<name>|set <agent> <model>|count <agent> <n>]",
                );
            }
            _ => {
                self.set_toast(&format!("unknown command: /{}", head));
            }
        }
    }

    fn handle_team_cmd(&mut self, args: &str) {
        let mut parts = args.split_whitespace();
        match parts.next() {
            None | Some("show") => {
                let t = self.current_team();
                self.set_toast(&format!(
                    "team: {} — {} agents · {}",
                    t.name,
                    t.total_size(),
                    t.blurb
                ));
            }
            Some("list") => {
                let names: Vec<String> = self
                    .teams
                    .iter()
                    .enumerate()
                    .map(|(i, t)| {
                        if i == self.active_team {
                            format!("[{}]", t.name)
                        } else {
                            t.name.clone()
                        }
                    })
                    .collect();
                self.set_toast(&format!("teams: {}", names.join(" · ")));
            }
            Some("next") => {
                self.active_team = (self.active_team + 1) % self.teams.len();
                self.after_team_switch();
            }
            Some("prev") => {
                self.active_team =
                    (self.active_team + self.teams.len() - 1) % self.teams.len();
                self.after_team_switch();
            }
            Some("set") => {
                let agent = parts.next().unwrap_or("");
                let model = parts.collect::<Vec<_>>().join(" ");
                if agent.is_empty() || model.is_empty() {
                    self.set_toast("usage: /team set <agent> <model>");
                    return;
                }
                if let Some(m) = self.current_team_mut().member_mut(agent) {
                    m.model = model.clone();
                    self.set_toast(&format!("{} → model {}", agent, model));
                } else {
                    self.set_toast(&format!(
                        "no '{}' on the current team — try /team next to switch",
                        agent
                    ));
                }
            }
            Some("count") => {
                let agent = parts.next().unwrap_or("");
                let n: u8 = parts
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                if agent.is_empty() || n == 0 {
                    self.set_toast("usage: /team count <agent> <n>");
                    return;
                }
                if let Some(m) = self.current_team_mut().member_mut(agent) {
                    m.count = n;
                    self.set_toast(&format!("{} count → {}", agent, n));
                } else {
                    self.set_toast(&format!("no '{}' on the current team", agent));
                }
            }
            Some("add") => {
                let agent = parts.next().unwrap_or("");
                if agent.is_empty() {
                    self.set_toast("usage: /team add <agent> [model] [count]");
                    return;
                }
                if !AGENT_ORDER.contains(&agent) {
                    self.set_toast(&format!(
                        "unknown agent '{}' — try: {}",
                        agent,
                        AGENT_ORDER.join(" ")
                    ));
                    return;
                }
                let model = parts.next().unwrap_or("auto").to_string();
                let count: u8 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
                if self.current_team().members.iter().any(|m| m.agent == agent) {
                    self.set_toast(&format!(
                        "'{}' is already on the team — use /team set / /team count to edit",
                        agent
                    ));
                    return;
                }
                self.current_team_mut().members.push(TeamMember {
                    agent: agent.to_string(),
                    model,
                    count,
                    included: true,
                    notes: None,
                });
                self.set_toast(&format!("+ {} added to team", agent));
            }
            Some("rm") | Some("remove") => {
                let agent = parts.next().unwrap_or("");
                if agent.is_empty() {
                    self.set_toast("usage: /team rm <agent>");
                    return;
                }
                let team = self.current_team_mut();
                let before = team.members.len();
                team.members.retain(|m| m.agent != agent);
                if team.members.len() < before {
                    self.set_toast(&format!("- {} removed", agent));
                } else {
                    self.set_toast(&format!("'{}' not on the current team", agent));
                }
            }
            Some("save") => {
                let new_name = parts.next().unwrap_or("").to_string();
                if new_name.is_empty() {
                    self.set_toast("usage: /team save <name>");
                    return;
                }
                if ["balanced", "lean", "scaled", "full", "local"].contains(&new_name.as_str()) {
                    self.set_toast(&format!(
                        "'{}' is a built-in preset — pick a different name",
                        new_name
                    ));
                    return;
                }
                let blurb = parts.collect::<Vec<_>>().join(" ");
                let mut clone = self.current_team().clone();
                clone.name = new_name.clone();
                if !blurb.is_empty() {
                    clone.blurb = blurb;
                }
                // Replace existing user team of same name, else push.
                if let Some(idx) = self.teams.iter().position(|t| t.name == new_name) {
                    self.teams[idx] = clone;
                    self.active_team = idx;
                } else {
                    self.teams.push(clone);
                    self.active_team = self.teams.len() - 1;
                }
                self.persist_teams();
                self.set_toast(&format!("saved as '{}' and activated", new_name));
            }
            Some("delete") | Some("rm-team") => {
                let target = parts.next().unwrap_or("").to_string();
                if target.is_empty() {
                    self.set_toast("usage: /team delete <name>");
                    return;
                }
                if ["balanced", "lean", "scaled", "full", "local"].contains(&target.as_str()) {
                    self.set_toast("can't delete a built-in preset");
                    return;
                }
                let Some(idx) = self.teams.iter().position(|t| t.name == target) else {
                    self.set_toast(&format!("no team '{}'", target));
                    return;
                };
                self.teams.remove(idx);
                if self.active_team >= self.teams.len() {
                    self.active_team = 0;
                } else if self.active_team > idx {
                    self.active_team -= 1;
                }
                self.persist_teams();
                self.set_toast(&format!("deleted '{}'", target));
            }
            Some("models") => {
                let names: Vec<&str> = crate::data::pricing::known_models();
                self.set_toast(&format!("known models: {}", names.join(" · ")));
            }
            Some(name) => {
                if let Some(idx) = self.teams.iter().position(|t| t.name == name) {
                    self.active_team = idx;
                    self.after_team_switch();
                } else {
                    let names: Vec<&str> =
                        self.teams.iter().map(|t| t.name.as_str()).collect();
                    self.set_toast(&format!(
                        "no team '{}' — known: {}",
                        name,
                        names.join(", ")
                    ));
                }
            }
        }
    }

    fn handle_auth_cmd(&mut self) {
        use crate::data::provider::{is_configured, Provider};
        let mut bits: Vec<String> = Vec::new();
        for p in Provider::all() {
            let mark = if is_configured(p) { "✓" } else { "✗" };
            bits.push(format!("{} {}", mark, p.name()));
        }
        self.set_toast(&format!("auth: {}", bits.join(" · ")));
    }

    fn after_team_switch(&mut self) {
        let name = self.current_team().name.clone();
        let blurb = self.current_team().blurb.clone();
        self.persist_teams();
        self.set_toast(&format!("team → {} ({})", name, blurb));
    }

    /// Write all user-defined teams plus the active selection to disk.
    /// Presets are excluded — they live in the binary and a future rename
    /// would otherwise break user state.
    fn persist_teams(&self) {
        let preset_names: std::collections::HashSet<&'static str> =
            ["balanced", "lean", "scaled", "full", "local"]
                .into_iter()
                .collect();
        let user_teams: Vec<Team> = self
            .teams
            .iter()
            .filter(|t| !preset_names.contains(t.name.as_str()))
            .cloned()
            .collect();
        let file = team::TeamsFile {
            active: Some(self.current_team().name.clone()),
            teams: user_teams,
        };
        let _ = team::save_teams_file(&file);
    }

    fn set_toast(&mut self, text: &str) {
        self.toast = Some(Toast {
            text: text.to_string(),
            created_at: Utc::now(),
        });
    }

    /// Returns the active toast if it's still within the display window
    /// (3 seconds). Past that the UI hides it.
    pub fn active_toast(&self) -> Option<&Toast> {
        let t = self.toast.as_ref()?;
        if (Utc::now() - t.created_at).num_seconds() < 4 {
            Some(t)
        } else {
            None
        }
    }

    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
        self.drain_runner_events();

        // Auto-tail invocations.jsonl roughly every second. The main loop
        // ticks at ~30 Hz (33 ms), so 30 frames ≈ 1 s. Cheap operation —
        // small append-only file, full re-read is fine for v1.
        if self.frame % 30 == 0 {
            if let Ok(inv) = invocations::InvocationStore::load() {
                self.invocations = inv;
            }
        }

        // Refresh threads less often — they're written less frequently.
        if self.frame % 300 == 0 {
            if let Ok(t) = threads::load_all() {
                self.threads = t;
            }
        }
    }

    /// Animated dot pattern for spinners. 8 frames at ~10 fps.
    pub fn spinner_frame(&self) -> &'static str {
        const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        FRAMES[(self.frame / 3) as usize % FRAMES.len()]
    }

    /// Is the last submitted prompt still in flight (subprocess not yet
    /// finished)? If so we surface the thinking spinner.
    pub fn waiting_for_runtime(&self) -> Option<&SubmittedPrompt> {
        let last = self.submitted.last()?;
        if last.completed.is_none() {
            Some(last)
        } else {
            None
        }
    }

    pub fn reload_threads(&mut self) {
        if let Ok(t) = threads::load_all() {
            self.threads = t;
            if self.sessions_selected >= self.threads.len() {
                self.sessions_selected = self.threads.len().saturating_sub(1);
            }
        }
        if let Ok(inv) = invocations::InvocationStore::load() {
            self.invocations = inv;
        }
    }

    pub fn move_selection(&mut self, delta: i32) {
        let (current, len) = match self.current_tab {
            Tab::Sessions => (&mut self.sessions_selected, self.threads.len()),
            Tab::Models => (&mut self.models_selected, self.invocations.by_model_today().len()),
            Tab::Agents => (&mut self.agents_selected, AGENT_ORDER.len()),
            _ => return,
        };
        if len == 0 {
            return;
        }
        let n = len as i32;
        let mut i = *current as i32 + delta;
        if i < 0 {
            i = 0;
        }
        if i >= n {
            i = n - 1;
        }
        *current = i as usize;
    }

    pub fn selected_thread(&self) -> Option<&ThreadSummary> {
        self.threads.get(self.sessions_selected)
    }

    /// Hydrate `self.submitted` from a past thread's user/assistant turns
    /// so the Thread tab shows it and follow-up prompts continue the
    /// conversation (via the existing session-context wrapper).
    pub fn resume_thread_full(&mut self, full: FullThread) {
        self.submitted.clear();
        let base_time = Utc::now() - chrono::Duration::seconds(full.turns.len() as i64);
        for (i, turn) in full.turns.iter().enumerate() {
            let response: Vec<ResponseLine> = turn
                .assistant
                .lines()
                .map(|l| ResponseLine {
                    source: LineSource::Stdout,
                    text: l.to_string(),
                })
                .collect();
            self.submitted.push(SubmittedPrompt {
                at: base_time + chrono::Duration::seconds(i as i64),
                workflow: self.workflow_name().to_string(),
                text: turn.user.clone(),
                delivered: true,
                job_id: None,
                command: None,
                response,
                completed: Some(JobCompletion::Success),
            });
        }
        let short = if full.id.len() > 6 {
            &full.id[full.id.len() - 6..]
        } else {
            &full.id
        };
        self.set_toast(&format!(
            "resumed T-...{} · {} turns",
            short,
            full.turns.len()
        ));
        self.current_tab = Tab::Thread;
    }

    /// Convenience: resume the currently selected Sessions row.
    pub fn resume_selected(&mut self) -> bool {
        let Some(t) = self.selected_thread() else {
            return false;
        };
        let id = t.id.clone();
        match threads::load_one(&id) {
            Ok(full) => {
                self.resume_thread_full(full);
                true
            }
            Err(e) => {
                self.set_toast(&format!("couldn't load thread: {}", e));
                false
            }
        }
    }
}

/// Build a context-wrapped prompt that includes prior conversational
/// turns from this agentwatch session so neo can answer follow-ups
/// coherently. Each prior turn contributes the user line + the agent
/// reply (collapsed from streaming lines).
///
/// Token budget: we cap total context at ~12_000 chars (≈3k tokens) by
/// dropping oldest turns first. The current prompt is always preserved.
pub fn wrap_with_session_context(prior: &[SubmittedPrompt], current: &str) -> String {
    let completed_turns: Vec<&SubmittedPrompt> = prior
        .iter()
        .filter(|sp| !sp.response.is_empty())
        .collect();
    if completed_turns.is_empty() {
        return current.to_string();
    }

    let mut turns: Vec<String> = Vec::new();
    for sp in &completed_turns {
        let reply: String = sp
            .response
            .iter()
            .map(|line| strip_ansi(&line.text))
            .collect::<Vec<_>>()
            .join("\n");
        turns.push(format!("User: {}\n\nAssistant: {}", sp.text, reply));
    }

    // Drop oldest turns until we're under budget.
    const MAX_CONTEXT_CHARS: usize = 12_000;
    let header = "Previous conversation in this session:\n\n";
    let footer = "\n\n---\n\nLatest user message:\n";
    let fixed_overhead = header.len() + footer.len() + current.len();

    let mut total: usize = turns.iter().map(|t| t.len() + 4).sum::<usize>() + fixed_overhead;
    while total > MAX_CONTEXT_CHARS && !turns.is_empty() {
        let dropped = turns.remove(0);
        total -= dropped.len() + 4;
    }

    if turns.is_empty() {
        return current.to_string();
    }

    let mut out = String::with_capacity(total);
    out.push_str(header);
    for (i, t) in turns.iter().enumerate() {
        if i > 0 {
            out.push_str("\n\n");
        }
        out.push_str(t);
    }
    out.push_str(footer);
    out.push_str(current);
    out
}

/// Strip ANSI CSI escape sequences (neo uses `colored!` heavily).
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            while let Some(&p) = chars.peek() {
                chars.next();
                if p.is_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Scan a prompt for `@path` tokens, try to read each file, and return:
/// - the prompt with a "Context:" block prepended listing the file contents
/// - an optional summary string for the toast ("attached: foo.rs, bar.rs")
///
/// Files larger than 50KB are truncated. Tokens whose path doesn't resolve
/// to a real file are left in the prompt verbatim and excluded from the
/// summary. The summary string is `None` when no attachment was attempted.
pub fn expand_attachments(prompt: &str, workspace: &std::path::Path) -> (String, Option<String>) {
    let mut attached: Vec<(String, String)> = Vec::new();
    let mut missing: Vec<String> = Vec::new();

    for token in prompt.split_whitespace() {
        let Some(rest) = token.strip_prefix('@') else {
            continue;
        };
        // Strip trailing punctuation that's almost never part of a path
        let path_str = rest.trim_end_matches(|c: char| matches!(c, ',' | '.' | ';' | ':' | ')' | ']'));
        if path_str.is_empty() {
            continue;
        }
        let path = std::path::Path::new(path_str);
        let resolved = if path.is_absolute() {
            path.to_path_buf()
        } else {
            workspace.join(path)
        };
        if !resolved.is_file() {
            missing.push(path_str.to_string());
            continue;
        }
        match std::fs::read_to_string(&resolved) {
            Ok(mut content) => {
                const MAX: usize = 50_000;
                if content.len() > MAX {
                    content.truncate(MAX);
                    content.push_str("\n... (truncated)\n");
                }
                attached.push((path_str.to_string(), content));
            }
            Err(_) => missing.push(path_str.to_string()),
        }
    }

    if attached.is_empty() && missing.is_empty() {
        return (prompt.to_string(), None);
    }

    let mut out = String::new();
    if !attached.is_empty() {
        out.push_str("Context:\n");
        for (path, content) in &attached {
            out.push_str(&format!("\n--- @{} ---\n{}\n", path, content));
        }
        out.push_str("\n---\n\n");
    }
    out.push_str(prompt);

    let summary = match (attached.len(), missing.len()) {
        (a, 0) if a > 0 => Some(format!(
            "attached {} file{}: {}",
            a,
            if a == 1 { "" } else { "s" },
            attached
                .iter()
                .map(|(p, _)| p.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
        (0, _) => Some(format!(
            "no files matched: {}",
            missing.join(", ")
        )),
        (a, _) => Some(format!(
            "attached {} · couldn't find {}",
            a,
            missing.join(", ")
        )),
    };
    (out, summary)
}

/// Flat list of (provider, model) pairs for the builder's model picker.
/// Plus an "auto" entry at the top so users can revert from a fixed
/// model back to router-managed selection.
pub fn model_picker_entries() -> Vec<(crate::data::Provider, &'static str)> {
    use crate::data::{pricing, Provider};
    let mut out: Vec<(Provider, &'static str)> = vec![(Provider::Unknown, "auto")];
    for (provider, models) in pricing::models_by_provider() {
        for m in models {
            out.push((provider, m));
        }
    }
    out
}

pub fn model_picker_len() -> usize {
    model_picker_entries().len()
}

pub fn model_at(idx: usize) -> Option<String> {
    model_picker_entries()
        .get(idx)
        .map(|(_, m)| m.to_string())
}

/// One entry in the slash-command registry. Used for both the popup and
/// the `/help` toast so we never drift.
pub struct SlashCmd {
    pub name: &'static str,
    pub usage: &'static str,
    pub help: &'static str,
}

pub const SLASH_CMDS: &[SlashCmd] = &[
    SlashCmd { name: "cancel",   usage: "/cancel",                       help: "kill the running neo subprocess" },
    SlashCmd { name: "clear",    usage: "/clear",                        help: "wipe the in-memory transcript" },
    SlashCmd { name: "reload",   usage: "/reload",                       help: "re-read threads + invocations" },
    SlashCmd { name: "help",     usage: "/help",                         help: "show this list as a toast" },
    SlashCmd { name: "quit",     usage: "/quit",                         help: "exit AgentWatch" },
    SlashCmd { name: "team",     usage: "/team [list|next|prev|<name>|add|rm|set|count|save|delete|models]", help: "build / switch / save dev teams" },
    SlashCmd { name: "auth",     usage: "/auth",                         help: "show which LLM providers are configured (env-only)" },
    SlashCmd { name: "resume",   usage: "/resume [<id-fragment>]",       help: "load a past thread into the working transcript" },
    SlashCmd { name: "threads",  usage: "/threads",                      help: "→ [5] Sessions tab" },
    SlashCmd { name: "cost",     usage: "/cost",                         help: "→ [8] Cost tab" },
    SlashCmd { name: "models",   usage: "/models",                       help: "→ [7] Models tab" },
    SlashCmd { name: "agents",   usage: "/agents",                       help: "→ [3] Agents tab" },
    SlashCmd { name: "plans",    usage: "/plans",                        help: "→ [4] Plans tab" },
    SlashCmd { name: "tools",    usage: "/tools",                        help: "→ [6] Tools tab" },
    SlashCmd { name: "overview", usage: "/overview",                     help: "→ [9] Overview tab" },
    SlashCmd { name: "insights", usage: "/insights",                     help: "→ [0] Insights tab" },
    SlashCmd { name: "console",  usage: "/console",                      help: "→ [2] Console tab" },
    SlashCmd { name: "thread",   usage: "/thread",                       help: "→ [1] Thread tab" },
];

/// Filter the registry by the text after the leading `/`. Empty filter
/// returns the full list. Match is a case-insensitive prefix.
pub fn slash_matches(prompt: &str) -> Vec<&'static SlashCmd> {
    let stripped = prompt.trim_start_matches('/').to_lowercase();
    let (head, _) = stripped.split_once(' ').unwrap_or((stripped.as_str(), ""));
    SLASH_CMDS
        .iter()
        .filter(|c| c.name.starts_with(head))
        .collect()
}

/// Canonical order for the Agents tab. Matches neo's `AgentId` enum order.
pub const AGENT_ORDER: [&str; 8] = [
    "router",
    "planner",
    "coder",
    "reviewer",
    "debugger",
    "tester",
    "documenter",
    "oracle",
];

pub struct Workflow {
    pub name: &'static str,
    pub cap: f64,
    pub blurb: &'static str,
}

/// Six pre-tuned router profiles, selectable with `[1]`-`[6]` from the
/// Console right rail. Caps are hard stops, not advisory.
pub const WORKFLOWS: &[Workflow] = &[
    Workflow { name: "feature-build", cap: 5.0, blurb: "planner → coder → tester → reviewer" },
    Workflow { name: "bug-hunt",      cap: 3.0, blurb: "debugger → coder → tester" },
    Workflow { name: "refactor",      cap: 8.0, blurb: "planner → multi-pass coder → reviewer" },
    Workflow { name: "docs",          cap: 1.0, blurb: "documenter solo" },
    Workflow { name: "review-only",   cap: 2.0, blurb: "reviewer on existing diff" },
    Workflow { name: "oracle",        cap: 1.0, blurb: "q&a, no edits" },
];

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
