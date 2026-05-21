//! Strength / warning heuristics over a `Team`. Pure functions —
//! reactive UI just re-runs them on every state change.

use super::team::Team;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warn,
    Crit,
}

#[derive(Debug, Clone)]
pub struct Tag {
    pub label: &'static str,
}

#[derive(Debug, Clone)]
pub struct Warning {
    pub severity: Severity,
    pub message: String,
}

/// Positive things to call out about the team composition.
pub fn tags_for(team: &Team) -> Vec<Tag> {
    let mut out = Vec::new();
    let has = |role: &str| {
        team.members
            .iter()
            .any(|m| m.included && m.agent == role)
    };
    let member = |role: &str| {
        team.members.iter().find(|m| m.included && m.agent == role)
    };

    // code-heavy: coder count ≥ 2 OR coder model is sonnet/opus-class
    if let Some(coder) = member("coder") {
        let premium =
            is_anthropic_premium(&coder.model) || is_openai_premium(&coder.model);
        if coder.count >= 2 || premium {
            out.push(Tag { label: "code-heavy" });
        }
        if coder.count >= 2 {
            out.push(Tag { label: "parallel-coders" });
        }
    }

    // review-strict: reviewer included with a premium model
    if let Some(rev) = member("reviewer") {
        if is_anthropic_premium(&rev.model) || is_openai_premium(&rev.model) {
            out.push(Tag { label: "review-strict" });
        }
    }

    if has("reviewer") && has("tester") {
        out.push(Tag { label: "code-quality" });
    }

    if has("documenter") {
        out.push(Tag { label: "documented" });
    }

    // cost-aware: per-task estimate under $0.05
    if estimated_cost_per_task(team) <= 0.05 {
        out.push(Tag { label: "cost-aware" });
    }

    // private: every included member is on Ollama
    let all_ollama = !team.members.is_empty()
        && team
            .members
            .iter()
            .filter(|m| m.included)
            .all(|m| is_ollama(&m.model));
    if all_ollama {
        out.push(Tag { label: "private" });
    }

    // parallel-pipeline: multiple included members with count ≥ 2
    let parallel_count = team
        .members
        .iter()
        .filter(|m| m.included && m.count >= 2)
        .count();
    if parallel_count >= 2 {
        out.push(Tag { label: "parallel-pipeline" });
    }

    out
}

/// Things to call out as problems.
pub fn warnings_for(team: &Team) -> Vec<Warning> {
    let mut out = Vec::new();
    let has = |role: &str| {
        team.members
            .iter()
            .any(|m| m.included && m.agent == role)
    };
    let member = |role: &str| {
        team.members.iter().find(|m| m.included && m.agent == role)
    };
    let included: Vec<_> = team.members.iter().filter(|m| m.included).collect();

    if included.is_empty() {
        out.push(Warning {
            severity: Severity::Crit,
            message: "team is empty — nothing will run".to_string(),
        });
        return out;
    }

    if !has("coder") {
        out.push(Warning {
            severity: Severity::Crit,
            message: "no coder — tasks cannot execute".to_string(),
        });
    }

    if !has("debugger") {
        out.push(Warning {
            severity: Severity::Info,
            message: "no debugger — /bug-hunt workflow disabled".to_string(),
        });
    }

    if !has("planner") {
        out.push(Warning {
            severity: Severity::Info,
            message: "no planner — complex tasks may struggle".to_string(),
        });
    }

    if !has("reviewer") {
        out.push(Warning {
            severity: Severity::Info,
            message: "no reviewer — code lands unreviewed".to_string(),
        });
    }

    // Premium reviewer cost callout
    if let Some(rev) = member("reviewer") {
        if is_anthropic_premium(&rev.model) || is_openai_premium(&rev.model) {
            out.push(Warning {
                severity: Severity::Info,
                message: "reviewer model is premium — high-cost reviews".to_string(),
            });
        }
    }

    // parallel coders without a planner
    if let Some(c) = member("coder") {
        if c.count >= 2 && !has("planner") {
            out.push(Warning {
                severity: Severity::Warn,
                message: "parallel coders without a planner — divergence risk".to_string(),
            });
        }
    }

    // All-Copilot tool-use restriction
    let all_copilot = !included.is_empty()
        && included.iter().all(|m| is_copilot(&m.model));
    if all_copilot {
        out.push(Warning {
            severity: Severity::Warn,
            message: "all-Copilot team — tool use may be limited".to_string(),
        });
    }

    // Mixed provider auth
    let needs: std::collections::HashSet<&'static str> = included
        .iter()
        .filter_map(|m| required_env(&m.model))
        .collect();
    if needs.len() >= 2 {
        let list: Vec<&str> = needs.into_iter().collect();
        out.push(Warning {
            severity: Severity::Warn,
            message: format!(
                "mixed-provider — needs {}",
                list.join(" + ")
            ),
        });
    }

    out
}

// ── pricing helpers — kept here so heuristics can stand alone ──────────

fn estimated_cost_per_task(team: &Team) -> f64 {
    use super::pricing;
    const TYP_IN: u32 = 3_000;
    const TYP_OUT: u32 = 800;
    const AUTO_FALLBACK: &str = "anthropic/claude-sonnet-4";
    team.members
        .iter()
        .filter(|m| m.included)
        .map(|m| {
            let model = if m.model == "auto" {
                AUTO_FALLBACK
            } else {
                m.model.as_str()
            };
            pricing::compute_cost(model, TYP_IN, TYP_OUT) * m.count as f64
        })
        .sum()
}

fn is_anthropic_premium(model: &str) -> bool {
    let m = model.to_lowercase();
    m.contains("sonnet") || m.contains("opus") || m.contains("o3")
}

fn is_openai_premium(model: &str) -> bool {
    let m = model.to_lowercase();
    m == "o3" || m == "openai/o3" || m == "o1" || m.contains("gpt-4o")
        && !m.contains("mini")
}

fn is_ollama(model: &str) -> bool {
    super::provider::provider_for(model) == super::provider::Provider::Ollama
}

fn is_copilot(model: &str) -> bool {
    super::provider::provider_for(model) == super::provider::Provider::Copilot
}

fn required_env(model: &str) -> Option<&'static str> {
    use super::provider::{provider_for, Provider};
    let p = provider_for(model);
    p.env_key().filter(|_| !matches!(p, Provider::Unknown))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::team::TeamMember;

    fn t(members: Vec<TeamMember>) -> Team {
        Team {
            name: "t".into(),
            blurb: "".into(),
            members,
            is_preset: false,
        }
    }

    #[test]
    fn empty_team_warns_critical() {
        let team = t(vec![]);
        let w = warnings_for(&team);
        assert!(w.iter().any(|w| w.severity == Severity::Crit));
    }

    #[test]
    fn excluded_members_dont_count() {
        // Router is included, coder is on the bench (excluded). The
        // warnings should reflect "no coder" — not "team is empty".
        let team = t(vec![
            TeamMember::auto("router"),
            TeamMember {
                agent: "coder".into(),
                model: "auto".into(),
                count: 1,
                included: false,
                notes: None,
            },
        ]);
        let warnings = warnings_for(&team);
        assert!(warnings.iter().any(|w| w.message.contains("no coder")));
        assert!(!warnings.iter().any(|w| w.message.contains("empty")));
    }

    #[test]
    fn parallel_coders_tag() {
        let team = t(vec![TeamMember {
            agent: "coder".into(),
            model: "auto".into(),
            count: 3,
            included: true,
            notes: None,
        }]);
        let tags = tags_for(&team);
        assert!(tags.iter().any(|t| t.label == "parallel-coders"));
    }

    #[test]
    fn all_ollama_is_private() {
        let team = t(vec![TeamMember {
            agent: "coder".into(),
            model: "llama3.2:latest".into(),
            count: 1,
            included: true,
            notes: None,
        }]);
        assert!(tags_for(&team).iter().any(|t| t.label == "private"));
    }
}
