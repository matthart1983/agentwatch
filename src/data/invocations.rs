//! Loader + aggregator for `<neo_data_dir>/invocations.jsonl`.
//!
//! neo's `InvocationLog` appends one JSON line per agent call. AgentWatch
//! parses these into `InvocationRecord` and rolls them up by model, by
//! agent, and by time bucket for the Models / Cost / Agents / Overview tabs.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::Path;

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};

use super::contract::InvocationRecord;
use super::paths;

pub struct InvocationStore {
    pub records: Vec<InvocationRecord>,
}

impl InvocationStore {
    pub fn load() -> Result<Self> {
        let path = paths::invocations_log()?;
        Self::load_from(&path)
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        let mut records = Vec::new();
        let file = match std::fs::File::open(path) {
            Ok(f) => f,
            Err(_) => return Ok(Self { records }),
        };
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(mut rec) = serde_json::from_str::<InvocationRecord>(&line) {
                // Neo records cost: 0.0 today (see PLAN.md PR #1 follow-ups).
                // Compute it from tokens + our pricing table so downstream
                // aggregators show real numbers. Honest fallback: leave it 0
                // if the model isn't in the pricing table.
                if rec.cost == 0.0 {
                    rec.cost = super::pricing::compute_cost(
                        &rec.model,
                        rec.tokens_in,
                        rec.tokens_out,
                    );
                }
                records.push(rec);
            }
        }
        records.sort_by(|a, b| a.ts.cmp(&b.ts));
        Ok(Self { records })
    }

    pub fn in_window(&self, since: DateTime<Utc>) -> impl Iterator<Item = &InvocationRecord> {
        self.records.iter().filter(move |r| r.ts >= since)
    }

    pub fn today(&self) -> impl Iterator<Item = &InvocationRecord> {
        let since = Utc::now() - Duration::hours(24);
        self.in_window(since)
    }

    /// Per-model aggregates over the last 24 hours.
    pub fn by_model_today(&self) -> Vec<ModelAgg> {
        aggregate_by(self.today(), |r| r.model.clone(), build_model_agg)
    }

    /// Per-agent aggregates over the last 24 hours.
    pub fn by_agent_today(&self) -> Vec<AgentAgg> {
        aggregate_by(self.today(), |r| r.agent.clone(), build_agent_agg)
    }

    pub fn total_cost_today(&self) -> f64 {
        self.today().map(|r| r.cost).sum()
    }

    pub fn calls_today(&self) -> usize {
        self.today().count()
    }

    /// Group invocations into pseudo-pipelines: any gap larger than
    /// `gap_seconds` starts a new group. The result is newest-pipeline first,
    /// with steps newest-first inside each pipeline.
    pub fn recent_pipelines(&self, gap_seconds: i64, max_pipelines: usize) -> Vec<PastPipeline> {
        if self.records.is_empty() {
            return Vec::new();
        }
        let mut all: Vec<&InvocationRecord> = self.records.iter().collect();
        all.sort_by(|a, b| b.ts.cmp(&a.ts)); // newest first

        let mut groups: Vec<PastPipeline> = Vec::new();
        let mut current: Vec<&InvocationRecord> = Vec::new();

        for r in all {
            if let Some(last) = current.last() {
                let gap = (last.ts - r.ts).num_seconds();
                if gap > gap_seconds {
                    groups.push(finalise(std::mem::take(&mut current)));
                    if groups.len() >= max_pipelines {
                        return groups;
                    }
                }
            }
            current.push(r);
        }
        if !current.is_empty() {
            groups.push(finalise(current));
        }
        groups.truncate(max_pipelines);
        groups
    }
}

/// A pseudo-pipeline reconstructed from time-clustered invocations. Until
/// neo PR #4's PipelineEvent wiring lands in the orchestrator, this is the
/// closest we can get to "what just happened in a pipeline" from the log.
#[derive(Debug, Clone)]
pub struct PastPipeline {
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub finished_at: chrono::DateTime<chrono::Utc>,
    pub total_cost: f64,
    pub total_tokens_in: u64,
    pub total_tokens_out: u64,
    /// Steps oldest-first so the timeline reads top-down chronologically.
    pub steps: Vec<PipelineStep>,
}

#[derive(Debug, Clone)]
pub struct PipelineStep {
    pub idx: usize,
    pub at: chrono::DateTime<chrono::Utc>,
    pub agent: String,
    pub model: String,
    pub status: String,
    pub cost: f64,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub latency_ms: u32,
    pub tool_calls: u32,
}

fn finalise(mut group_newest_first: Vec<&InvocationRecord>) -> PastPipeline {
    // Flip to oldest-first so step indices make sense.
    group_newest_first.reverse();
    let started_at = group_newest_first.first().map(|r| r.ts).unwrap();
    let finished_at = group_newest_first.last().map(|r| r.ts).unwrap();
    let total_cost: f64 = group_newest_first.iter().map(|r| r.cost).sum();
    let total_tokens_in: u64 = group_newest_first.iter().map(|r| r.tokens_in as u64).sum();
    let total_tokens_out: u64 = group_newest_first.iter().map(|r| r.tokens_out as u64).sum();
    let steps = group_newest_first
        .iter()
        .enumerate()
        .map(|(i, r)| PipelineStep {
            idx: i + 1,
            at: r.ts,
            agent: r.agent.clone(),
            model: r.model.clone(),
            status: r.status.clone(),
            cost: r.cost,
            tokens_in: r.tokens_in,
            tokens_out: r.tokens_out,
            latency_ms: r.latency_ms,
            tool_calls: r.tool_calls,
        })
        .collect();
    PastPipeline {
        started_at,
        finished_at,
        total_cost,
        total_tokens_in,
        total_tokens_out,
        steps,
    }
}

#[derive(Debug, Clone)]
pub struct ModelAgg {
    pub model: String,
    pub provider: String,
    pub calls: usize,
    pub cost: f64,
    pub p50_latency_ms: u32,
    pub p99_latency_ms: u32,
    pub success_rate: f32,
    pub tokens_in: u64,
    pub tokens_out: u64,
}

#[derive(Debug, Clone)]
pub struct AgentAgg {
    pub agent: String,
    pub calls: usize,
    pub cost: f64,
    pub avg_latency_ms: u32,
    pub last_seen: Option<DateTime<Utc>>,
}

fn aggregate_by<'a, I, K, B, V>(records: I, key: K, build: B) -> Vec<V>
where
    I: Iterator<Item = &'a InvocationRecord>,
    K: Fn(&InvocationRecord) -> String,
    B: Fn(&str, &[&'a InvocationRecord]) -> V,
{
    let mut buckets: HashMap<String, Vec<&InvocationRecord>> = HashMap::new();
    for r in records {
        buckets.entry(key(r)).or_default().push(r);
    }
    buckets
        .into_iter()
        .map(|(k, rs)| build(&k, &rs))
        .collect()
}

fn build_model_agg(model: &str, rs: &[&InvocationRecord]) -> ModelAgg {
    let calls = rs.len();
    let cost: f64 = rs.iter().map(|r| r.cost).sum();
    let tokens_in: u64 = rs.iter().map(|r| r.tokens_in as u64).sum();
    let tokens_out: u64 = rs.iter().map(|r| r.tokens_out as u64).sum();
    let provider = rs.first().map(|r| r.provider.clone()).unwrap_or_default();

    let mut latencies: Vec<u32> = rs.iter().map(|r| r.latency_ms).collect();
    latencies.sort_unstable();
    let p50 = percentile(&latencies, 0.50);
    let p99 = percentile(&latencies, 0.99);

    let successes = rs.iter().filter(|r| r.status == "success").count();
    let success_rate = if calls == 0 { 0.0 } else { successes as f32 / calls as f32 };

    ModelAgg {
        model: model.to_string(),
        provider,
        calls,
        cost,
        p50_latency_ms: p50,
        p99_latency_ms: p99,
        success_rate,
        tokens_in,
        tokens_out,
    }
}

fn build_agent_agg(agent: &str, rs: &[&InvocationRecord]) -> AgentAgg {
    let calls = rs.len();
    let cost: f64 = rs.iter().map(|r| r.cost).sum();
    let avg_latency_ms = if calls == 0 {
        0
    } else {
        (rs.iter().map(|r| r.latency_ms as u64).sum::<u64>() / calls as u64) as u32
    };
    let last_seen = rs.iter().map(|r| r.ts).max();
    AgentAgg {
        agent: agent.to_string(),
        calls,
        cost,
        avg_latency_ms,
        last_seen,
    }
}

fn percentile(sorted: &[u32], p: f64) -> u32 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}
