use std::path::PathBuf;
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use tui_textarea::TextArea;

use crate::data::{contract::ControlCommand, invocations, threads, InvocationStore, ThreadSummary};
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
        }
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
        // full conversation, not just the latest user line.
        let job_id = self.runner.spawn(&workflow, &outbound_text);
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
        let (head, _rest) = match cmd.split_once(' ') {
            Some((h, r)) => (h, r),
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
            "help" | "?" => {
                self.set_toast(
                    "slash commands: /cancel /clear /reload /threads /cost /models /agents /plans /tools /overview /insights /console /thread /quit",
                );
            }
            _ => {
                self.set_toast(&format!("unknown command: /{}", head));
            }
        }
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
