//! The on-disk contract between neo and AgentWatch.
//!
//! These types deserialize neo's `state.json` and `invocations.jsonl` and
//! serialize control commands written to `inbox/<uuid>.json` (or sent over
//! the neo-agentd socket when the `agentd` feature is on).
//!
//! The contract is intentionally narrow — see `design_handoff_agentwatch/TECHNICAL.md`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub ts: DateTime<Utc>,
    pub runtime: String,
    pub version: String,
    pub active_session: Option<String>,
    pub agents: Vec<AgentState>,
    pub pipeline: Option<Pipeline>,
    pub budget: BudgetSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    pub id: String,
    pub state: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub iteration: u32,
    #[serde(default)]
    pub current_tool: Option<CurrentTool>,
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentTool {
    pub name: String,
    #[serde(default)]
    pub args_preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pipeline {
    pub task: String,
    pub tasks: Vec<PipelineTask>,
    #[serde(default)]
    pub review_cycle: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineTask {
    pub id: u32,
    pub agent: String,
    pub status: String,
    #[serde(default)]
    pub deps: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BudgetSnapshot {
    #[serde(default)]
    pub session: Option<BudgetWindow>,
    #[serde(default)]
    pub day: Option<BudgetWindow>,
    #[serde(default)]
    pub rolling_24h: Option<BudgetWindow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetWindow {
    pub spent: f64,
    #[serde(default)]
    pub cap: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvocationRecord {
    pub ts: DateTime<Utc>,
    pub thread: String,
    pub agent: String,
    pub model: String,
    pub provider: String,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub cost: f64,
    pub latency_ms: u32,
    pub status: String,
    pub tool_calls: u32,
}

/// Mirrors neo's `Thread` on disk (`session/types.rs`) — only the index-relevant
/// fields. `messages` is parsed separately so we can index without loading
/// every transcript into memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadSummary {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub workspace: String,
    #[serde(default)]
    pub cost_total: f64,
    #[serde(default)]
    pub models_used: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Populated by the loader, not present in the JSON. Counted from
    /// `messages.len()` so we can use it as a stand-in for "calls" until
    /// `invocations.jsonl` lands.
    #[serde(skip)]
    pub message_count: usize,
    /// First user message in the thread, used as the task description.
    #[serde(skip)]
    pub first_user_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum ControlCommand {
    Prompt {
        workflow: String,
        text: String,
    },
    Cancel {
        session: String,
    },
    Fork {
        session: String,
        from_message: u32,
    },
    Resume {
        session: String,
    },
    Attach {
        session: String,
        file: String,
    },
}
