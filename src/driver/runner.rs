//! Spawns `neo` as a subprocess on prompt submit and streams its stdout/stderr
//! back to the UI via a channel. Each spawned process is identified by a
//! monotonic `JobId` that the UI uses to attribute output lines to a
//! specific `SubmittedPrompt`.
//!
//! This is the bridge that gives AgentWatch real responses today without
//! waiting on neo's `ControlInbox` to be wired into the orchestrator main
//! loop. Once neo PR #3's wiring lands, we can swap this for the
//! drop-a-file-in-inbox path — the UI surface stays the same.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JobId(pub u64);

#[derive(Debug, Clone, Copy)]
pub enum LineSource {
    Stdout,
    Stderr,
}

#[derive(Debug)]
pub enum JobEvent {
    Started { id: JobId, command: String },
    Line { id: JobId, source: LineSource, text: String },
    Finished { id: JobId, status: Option<ExitStatus> },
    Failed { id: JobId, reason: String },
}

pub struct Runner {
    tx: Sender<JobEvent>,
    pub rx: Receiver<JobEvent>,
    next_id: AtomicU64,
    neo_bin: Option<PathBuf>,
}

impl Runner {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx,
            rx,
            next_id: AtomicU64::new(1),
            neo_bin: resolve_neo_binary(),
        }
    }

    pub fn neo_path(&self) -> Option<&PathBuf> {
        self.neo_bin.as_ref()
    }

    /// Spawn `neo <subcommand> <prompt>` and stream its output. The thread
    /// closes the channel for this job once the process exits. Returns
    /// the assigned `JobId`.
    pub fn spawn(&self, workflow: &str, prompt: &str) -> JobId {
        let id = JobId(self.next_id.fetch_add(1, Ordering::Relaxed));
        let subcommand = workflow_to_subcommand(workflow);
        let command_display = format!("neo {} \"{}\"", subcommand, truncate(prompt, 60));

        let Some(neo_bin) = self.neo_bin.clone() else {
            let _ = self.tx.send(JobEvent::Failed {
                id,
                reason: "neo binary not found. Set AGENTWATCH_NEO_BIN, or `cargo install` neo, or build it at ~/projects/active/neo/target/release/neo".to_string(),
            });
            return id;
        };

        let tx = self.tx.clone();
        let prompt_owned = prompt.to_string();
        let workflow_owned = workflow.to_string();
        thread::spawn(move || run(id, neo_bin, &workflow_owned, &prompt_owned, command_display, tx));
        id
    }
}

impl Default for Runner {
    fn default() -> Self {
        Self::new()
    }
}

fn run(
    id: JobId,
    neo_bin: PathBuf,
    workflow: &str,
    prompt: &str,
    command_display: String,
    tx: Sender<JobEvent>,
) {
    let subcommand = workflow_to_subcommand(workflow);
    let _ = tx.send(JobEvent::Started {
        id,
        command: command_display,
    });

    let mut cmd = Command::new(&neo_bin);
    cmd.arg(subcommand).arg(prompt);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send(JobEvent::Failed {
                id,
                reason: format!("failed to spawn neo: {}", e),
            });
            return;
        }
    };

    // Spawn one reader thread per stream so we capture interleaved output.
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let tx_out = tx.clone();
    let tx_err = tx.clone();
    let stdout_handle = stdout.map(|s| {
        thread::spawn(move || pipe_lines(id, s, LineSource::Stdout, tx_out))
    });
    let stderr_handle = stderr.map(|s| {
        thread::spawn(move || pipe_lines(id, s, LineSource::Stderr, tx_err))
    });

    let status = child.wait().ok();

    if let Some(h) = stdout_handle {
        let _ = h.join();
    }
    if let Some(h) = stderr_handle {
        let _ = h.join();
    }

    let _ = tx.send(JobEvent::Finished { id, status });
}

fn pipe_lines<R: std::io::Read + Send + 'static>(
    id: JobId,
    reader: R,
    source: LineSource,
    tx: Sender<JobEvent>,
) {
    let buf = BufReader::new(reader);
    for line in buf.lines().map_while(Result::ok) {
        let _ = tx.send(JobEvent::Line {
            id,
            source,
            text: line,
        });
    }
}

fn workflow_to_subcommand(workflow: &str) -> &'static str {
    match workflow {
        "bug-hunt" => "debug",
        "docs" => "doc",
        "review-only" => "review",
        "oracle" => "ask",
        // feature-build, refactor — default to the full pipeline
        _ => "do",
    }
}

fn resolve_neo_binary() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("AGENTWATCH_NEO_BIN") {
        let path = PathBuf::from(p);
        if path.exists() {
            return Some(path);
        }
    }
    if let Ok(out) = Command::new("sh").arg("-c").arg("which neo").output() {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                let path = PathBuf::from(s);
                if path.exists() {
                    return Some(path);
                }
            }
        }
    }
    let dev_paths = [
        "~/projects/active/neo/target/release/neo",
        "~/projects/active/neo/target/debug/neo",
    ];
    for p in dev_paths {
        let expanded = expand_tilde(p);
        if expanded.exists() {
            return Some(expanded);
        }
    }
    None
}

fn expand_tilde(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(p)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let kept: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", kept)
    }
}
