//! Spawns `neo` as a subprocess on prompt submit and streams its stdout/stderr
//! back to the UI via a channel. Each spawned process is identified by a
//! monotonic `JobId` that the UI uses to attribute output lines to a
//! specific `SubmittedPrompt`.
//!
//! This is the bridge that gives AgentWatch real responses today without
//! waiting on neo's `ControlInbox` to be wired into the orchestrator main
//! loop. Once neo PR #3's wiring lands, we can swap this for the
//! drop-a-file-in-inbox path — the UI surface stays the same.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
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
    Cancelled { id: JobId },
}

pub struct Runner {
    tx: Sender<JobEvent>,
    pub rx: Receiver<JobEvent>,
    next_id: AtomicU64,
    neo_bin: Option<PathBuf>,
    /// PID per in-flight job, so `cancel()` can send SIGTERM. Entries are
    /// removed by the run thread once the child exits.
    pids: Arc<Mutex<HashMap<JobId, u32>>>,
}

impl Runner {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx,
            rx,
            next_id: AtomicU64::new(1),
            neo_bin: resolve_neo_binary(),
            pids: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn neo_path(&self) -> Option<&PathBuf> {
        self.neo_bin.as_ref()
    }

    /// Spawn the right backend for the requested model. Ollama-shaped
    /// models dispatch to `ollama run` directly; anything else routes
    /// through neo (currently OpenRouter only).
    ///
    /// `override_model`: model id from the active team. Used for both
    /// the dispatch decision AND, for the neo path, forwarded as
    /// `NEO_DEFAULT_MODEL` so neo's config layer respects it.
    pub fn spawn(&self, workflow: &str, prompt: &str, override_model: Option<&str>) -> JobId {
        let id = JobId(self.next_id.fetch_add(1, Ordering::Relaxed));

        let provider = match override_model {
            Some("auto") | None => crate::data::Provider::OpenRouter,
            Some(m) => crate::data::provider_for(m),
        };

        match provider {
            crate::data::Provider::Ollama => self.spawn_ollama(id, prompt, override_model),
            _ => self.spawn_neo(id, workflow, prompt, override_model),
        }
    }

    fn spawn_neo(
        &self,
        id: JobId,
        workflow: &str,
        prompt: &str,
        override_model: Option<&str>,
    ) -> JobId {
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
        let override_owned = override_model.map(|s| s.to_string());
        let pids = self.pids.clone();
        thread::spawn(move || {
            run_neo(
                id,
                neo_bin,
                &workflow_owned,
                &prompt_owned,
                command_display,
                override_owned,
                tx,
                pids,
            )
        });
        id
    }

    fn spawn_ollama(&self, id: JobId, prompt: &str, model: Option<&str>) -> JobId {
        let model = model.unwrap_or("llama3.3:70b").to_string();
        let command_display = format!("ollama run {} \"{}\"", model, truncate(prompt, 60));
        let tx = self.tx.clone();
        let prompt_owned = prompt.to_string();
        let pids = self.pids.clone();
        thread::spawn(move || run_ollama(id, model, prompt_owned, command_display, tx, pids));
        id
    }

    /// Send SIGTERM to the in-flight subprocess for this job, if any. The
    /// run thread will see the child exit and emit `Cancelled` followed by
    /// `Finished`.
    pub fn cancel(&self, id: JobId) -> bool {
        let pid = self.pids.lock().ok().and_then(|p| p.get(&id).copied());
        let Some(pid) = pid else { return false };
        send_sigterm(pid);
        let _ = self.tx.send(JobEvent::Cancelled { id });
        true
    }
}

#[cfg(unix)]
fn send_sigterm(pid: u32) {
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
}

#[cfg(not(unix))]
fn send_sigterm(_pid: u32) {
    // No-op on non-unix; cancel becomes a soft "stop waiting" rather than
    // a real kill. AgentWatch is macOS+Linux for v1 so this is unreachable.
}

impl Default for Runner {
    fn default() -> Self {
        Self::new()
    }
}

fn run_neo(
    id: JobId,
    neo_bin: PathBuf,
    workflow: &str,
    prompt: &str,
    command_display: String,
    override_model: Option<String>,
    tx: Sender<JobEvent>,
    pids: Arc<Mutex<HashMap<JobId, u32>>>,
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
    if let Some(model) = override_model {
        cmd.env("NEO_DEFAULT_MODEL", model);
    }

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

    // Register PID so cancel() can find it. Done before piping so a very
    // fast cancel doesn't race.
    if let Ok(mut p) = pids.lock() {
        p.insert(id, child.id());
    }

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

    if let Ok(mut p) = pids.lock() {
        p.remove(&id);
    }

    let _ = tx.send(JobEvent::Finished { id, status });
}

fn run_ollama(
    id: JobId,
    model: String,
    prompt: String,
    command_display: String,
    tx: Sender<JobEvent>,
    pids: Arc<Mutex<HashMap<JobId, u32>>>,
) {
    let _ = tx.send(JobEvent::Started {
        id,
        command: command_display,
    });

    // Resolve ollama binary — PATH lookup mostly; fall back to common paths.
    let ollama_bin = resolve_ollama_binary();
    let Some(ollama_bin) = ollama_bin else {
        let _ = tx.send(JobEvent::Failed {
            id,
            reason: "ollama not found on PATH (install: brew install ollama / curl https://ollama.ai/install.sh)".to_string(),
        });
        return;
    };

    let started_at = std::time::Instant::now();
    let prompt_chars = prompt.chars().count();

    let mut cmd = Command::new(&ollama_bin);
    cmd.arg("run").arg(&model).arg(&prompt);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send(JobEvent::Failed {
                id,
                reason: format!("failed to spawn ollama: {}", e),
            });
            return;
        }
    };

    if let Ok(mut p) = pids.lock() {
        p.insert(id, child.id());
    }

    // Track output for token estimation. ollama doesn't print token counts
    // to stdout, so we estimate ≈ char_count / 4 (rough English ratio).
    let response_chars: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let tx_out = tx.clone();
    let tx_err = tx.clone();
    let response_chars_clone = response_chars.clone();

    let stdout_handle = stdout.map(|s| {
        thread::spawn(move || {
            let buf = BufReader::new(s);
            for line in buf.lines().map_while(Result::ok) {
                if let Ok(mut c) = response_chars_clone.lock() {
                    *c += line.chars().count() + 1; // +1 for the newline
                }
                let _ = tx_out.send(JobEvent::Line {
                    id,
                    source: LineSource::Stdout,
                    text: line,
                });
            }
        })
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

    if let Ok(mut p) = pids.lock() {
        p.remove(&id);
    }

    // Log to invocations.jsonl so all the existing tabs light up the
    // same way they do for neo calls. Cost is 0.0 — Ollama is free.
    let elapsed = started_at.elapsed();
    let chars_out = response_chars.lock().map(|c| *c).unwrap_or(0);
    log_ollama_invocation(&model, prompt_chars, chars_out, elapsed);

    let _ = tx.send(JobEvent::Finished { id, status });
}

fn log_ollama_invocation(
    model: &str,
    prompt_chars: usize,
    response_chars: usize,
    elapsed: std::time::Duration,
) {
    // Rough token estimate: ~4 chars per token for English. ollama's
    // CLI doesn't surface token counts, so this is a soft approximation
    // for the Cost / Models tabs.
    let tokens_in = (prompt_chars / 4) as u32;
    let tokens_out = (response_chars / 4) as u32;

    let record = serde_json::json!({
        "ts": chrono::Utc::now().to_rfc3339(),
        "thread": "",
        "agent": "coder",
        "model": model,
        "provider": "ollama",
        "tokens_in": tokens_in,
        "tokens_out": tokens_out,
        "cost": 0.0,
        "latency_ms": elapsed.as_millis().min(u32::MAX as u128) as u32,
        "status": "success",
        "tool_calls": 0,
    });

    let Ok(path) = (|| -> anyhow::Result<std::path::PathBuf> {
        let base = dirs::data_dir().ok_or_else(|| anyhow::anyhow!("no data dir"))?;
        Ok(base.join("neo").join("invocations.jsonl"))
    })() else {
        return;
    };

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        use std::io::Write;
        let _ = writeln!(f, "{}", record);
    }
}

fn resolve_ollama_binary() -> Option<PathBuf> {
    if let Ok(out) = Command::new("sh").arg("-c").arg("which ollama").output() {
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
    for p in ["/usr/local/bin/ollama", "/opt/homebrew/bin/ollama"] {
        let path = PathBuf::from(p);
        if path.exists() {
            return Some(path);
        }
    }
    None
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
