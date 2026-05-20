//! Internal in-memory types. The data layer translates `contract` JSON into
//! these and the UI renders against them.

use std::time::SystemTime;

#[derive(Debug, Clone)]
pub struct Tick {
    pub at: SystemTime,
    pub runtime_online: bool,
    pub active_session: Option<String>,
    pub agents: Vec<AgentTick>,
    pub budget: Budget,
}

#[derive(Debug, Clone, Default)]
pub struct Budget {
    pub session_spent: f64,
    pub session_cap: f64,
    pub day_spent: f64,
    pub day_cap: f64,
}

#[derive(Debug, Clone)]
pub struct AgentTick {
    pub id: String,
    pub state: String,
    pub model: Option<String>,
    pub iteration: u32,
}

#[derive(Debug, Clone)]
pub struct Invocation {
    pub at: SystemTime,
    pub thread: String,
    pub agent: String,
    pub model: String,
    pub cost: f64,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub latency_ms: u32,
}
