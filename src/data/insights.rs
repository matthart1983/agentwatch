//! Pure functions over `InvocationStore` + `Vec<ThreadSummary>` that
//! return zero or more `Insight`s. Each rule is self-contained and can be
//! tested with a fixture store.

use chrono::{Duration, Utc};

use super::contract::ThreadSummary;
use super::invocations::InvocationStore;

#[derive(Debug, Clone)]
pub struct Insight {
    pub severity: Severity,
    pub title: String,
    pub body: Vec<String>,
    pub suggested_tab: Option<&'static str>,
    pub age_seconds: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Crit,
    Warn,
    Info,
}

impl Severity {
    pub fn label(&self) -> &'static str {
        match self {
            Severity::Crit => "CRIT",
            Severity::Warn => "WARN",
            Severity::Info => "INFO",
        }
    }
}

/// Default daily cap mirrors what Cost tab uses. Make this configurable
/// once we read neo's config.
const DAY_CAP: f64 = 20.0;

pub fn compute(store: &InvocationStore, threads: &[ThreadSummary]) -> Vec<Insight> {
    let mut out = Vec::new();
    out.extend(insight_high_latency(store));
    out.extend(insight_budget_approach(store));
    out.extend(insight_model_concentration(store));
    out.extend(insight_recent_failures(store));
    out.extend(insight_top_agent(store));
    out.extend(insight_idle(store, threads));

    // Sort by severity desc, then by recency desc.
    out.sort_by(|a, b| {
        sev_rank(b.severity)
            .cmp(&sev_rank(a.severity))
            .then(a.age_seconds.cmp(&b.age_seconds))
    });
    out
}

fn sev_rank(s: Severity) -> u8 {
    match s {
        Severity::Crit => 3,
        Severity::Warn => 2,
        Severity::Info => 1,
    }
}

fn insight_high_latency(store: &InvocationStore) -> Option<Insight> {
    let mut models = store.by_model_today();
    if models.is_empty() {
        return None;
    }
    models.sort_by_key(|m| std::cmp::Reverse(m.p99_latency_ms));
    let worst = models.first()?;
    if worst.p99_latency_ms < 5_000 || worst.calls < 3 {
        return None;
    }
    Some(Insight {
        severity: Severity::Warn,
        title: format!(
            "{} p99 latency is high ({})",
            short(&worst.model),
            format_latency(worst.p99_latency_ms)
        ),
        body: vec![
            format!(
                "Across {} calls today, p50 is {} and p99 is {}.",
                worst.calls,
                format_latency(worst.p50_latency_ms),
                format_latency(worst.p99_latency_ms)
            ),
            "Likely upstream congestion. Router has no automatic fallback yet.".to_string(),
        ],
        suggested_tab: Some("Models"),
        age_seconds: 0,
    })
}

fn insight_budget_approach(store: &InvocationStore) -> Option<Insight> {
    let spent = store.total_cost_today();
    if spent <= 0.0 {
        return None;
    }
    let pct = (spent / DAY_CAP) * 100.0;
    if pct < 50.0 {
        return None;
    }
    let severity = if pct >= 90.0 {
        Severity::Crit
    } else if pct >= 75.0 {
        Severity::Warn
    } else {
        Severity::Info
    };

    // Project from last-hour rate.
    let last_hour: f64 = store
        .in_window(Utc::now() - Duration::hours(1))
        .map(|r| r.cost)
        .sum();
    let hours_left = (24 - chrono::Local::now().to_utc().hour_diff()) as f64;
    let projected = spent + last_hour * hours_left.max(0.0);

    Some(Insight {
        severity,
        title: format!(
            "Spent ${:.2} of ${:.0} daily cap ({:.0}%)",
            spent, DAY_CAP, pct
        ),
        body: vec![
            format!(
                "Current rate: ${:.2}/hr (last 60 min). Projected total today: ${:.2}.",
                last_hour, projected
            ),
            if projected > DAY_CAP {
                format!(
                    "On track to exceed cap by ${:.2}.",
                    projected - DAY_CAP
                )
            } else {
                format!(
                    "On track to land at {:.0}% of cap.",
                    (projected / DAY_CAP) * 100.0
                )
            },
        ],
        suggested_tab: Some("Cost"),
        age_seconds: 60,
    })
}

fn insight_model_concentration(store: &InvocationStore) -> Option<Insight> {
    let models = store.by_model_today();
    let total_calls: usize = models.iter().map(|m| m.calls).sum();
    if total_calls < 10 {
        return None;
    }
    let leader = models
        .iter()
        .max_by_key(|m| m.calls)?;
    let share = (leader.calls as f64 / total_calls as f64) * 100.0;
    if share < 60.0 {
        return None;
    }
    Some(Insight {
        severity: Severity::Info,
        title: format!(
            "Router prefers {} ({:.0}% of calls today)",
            short(&leader.model),
            share
        ),
        body: vec![
            format!(
                "{} of {} calls went to {}. Success rate {:.0}%.",
                leader.calls,
                total_calls,
                short(&leader.model),
                leader.success_rate * 100.0
            ),
            "Router scoring favors this model for the current task mix.".to_string(),
        ],
        suggested_tab: Some("Models"),
        age_seconds: 0,
    })
}

fn insight_recent_failures(store: &InvocationStore) -> Option<Insight> {
    let recent: Vec<_> = store.records.iter().rev().take(20).collect();
    if recent.len() < 5 {
        return None;
    }
    let failures = recent
        .iter()
        .filter(|r| r.status != "success")
        .count();
    if failures == 0 {
        return None;
    }
    let pct = (failures as f64 / recent.len() as f64) * 100.0;
    let severity = if pct >= 30.0 {
        Severity::Crit
    } else if pct >= 10.0 {
        Severity::Warn
    } else {
        Severity::Info
    };
    Some(Insight {
        severity,
        title: format!("{} of last {} calls failed ({:.0}%)", failures, recent.len(), pct),
        body: vec![
            "Check the Models tab for which model is degraded.".to_string(),
        ],
        suggested_tab: Some("Models"),
        age_seconds: 0,
    })
}

fn insight_top_agent(store: &InvocationStore) -> Option<Insight> {
    let agents = store.by_agent_today();
    let total_calls: usize = agents.iter().map(|a| a.calls).sum();
    if total_calls < 10 {
        return None;
    }
    let leader = agents.iter().max_by_key(|a| a.calls)?;
    let share = (leader.calls as f64 / total_calls as f64) * 100.0;
    Some(Insight {
        severity: Severity::Info,
        title: format!(
            "{} is doing {:.0}% of today's work",
            cap(&leader.agent),
            share
        ),
        body: vec![
            format!(
                "{} calls today, ${:.2} spent on this agent.",
                leader.calls, leader.cost
            ),
        ],
        suggested_tab: Some("Agents"),
        age_seconds: 600,
    })
}

fn insight_idle(store: &InvocationStore, threads: &[ThreadSummary]) -> Option<Insight> {
    if !store.records.is_empty() {
        return None;
    }
    Some(Insight {
        severity: Severity::Info,
        title: if threads.is_empty() {
            "No activity yet. Submit a prompt on the Console.".to_string()
        } else {
            format!(
                "{} historical sessions but no calls recorded in invocations.jsonl",
                threads.len()
            )
        },
        body: if threads.is_empty() {
            vec!["The rule engine starts producing insights once neo runs once.".to_string()]
        } else {
            vec![
                "Older threads predate neo's InvocationLog (PR #1)."
                    .to_string(),
                "Run a fresh prompt — it'll appear here within seconds.".to_string(),
            ]
        },
        suggested_tab: Some("Console"),
        age_seconds: 0,
    })
}

// helpers ------------------------------------------------------------
fn short(model: &str) -> String {
    model.split('/').next_back().unwrap_or(model).to_string()
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

fn format_latency(ms: u32) -> String {
    if ms == 0 {
        "—".to_string()
    } else if ms < 1000 {
        format!("{}ms", ms)
    } else {
        format!("{:.1}s", ms as f64 / 1000.0)
    }
}

/// Hours since midnight local time. chrono::Timelike isn't in scope here
/// so we use a small wrapper that callers can extend.
trait HourDiff {
    fn hour_diff(&self) -> u32;
}
impl HourDiff for chrono::DateTime<chrono::Utc> {
    fn hour_diff(&self) -> u32 {
        use chrono::Timelike;
        let local = self.with_timezone(&chrono::Local);
        local.hour()
    }
}
