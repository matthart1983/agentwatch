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
    pub fn submit_prompt(&mut self) {
        let text = self.prompt.lines().join("\n").trim().to_string();
        if text.is_empty() {
            return;
        }
        let workflow = self.workflow_name().to_string();
        let at = Utc::now();

        // Always drop the inbox file too — it's our future-facing path and
        // costs nothing today (just creates a small JSON file on disk).
        let delivered = match self.inbox.as_ref() {
            Some(inbox) => inbox
                .send(&ControlCommand::Prompt {
                    workflow: workflow.clone(),
                    text: text.clone(),
                })
                .is_ok(),
            None => false,
        };

        // Spawn neo to actually run the prompt and stream output back.
        let job_id = self.runner.spawn(&workflow, &text);
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
            }
        }
    }

    pub fn clear_prompt(&mut self) {
        self.prompt = TextArea::default();
        self.prompt.set_cursor_line_style(Default::default());
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
