//! Team composition: which of neo's 8 agents are on the bench, the model
//! each one prefers, and how many parallel instances to allow.
//!
//! Today neo doesn't accept per-agent model overrides on the command line
//! — when the active team has a non-`auto` model selection we forward it
//! as `NEO_DEFAULT_MODEL` to the spawned subprocess. That coarsely sets
//! the default for the whole run. True per-agent assignment requires a
//! follow-up neo PR.
//!
//! Persistence: presets ship in-binary; the active selection is written to
//! `~/.config/agentwatch/team.toml`. We don't persist user edits to the
//! preset definitions themselves yet — slash commands cycle and override
//! the active one in memory only.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    pub name: String,
    pub blurb: String,
    pub members: Vec<TeamMember>,
    /// True for ship-with-the-binary presets — the editor treats them
    /// as read-only (you must save-as to a new name to edit). Not
    /// persisted; reset on load from defaults.
    #[serde(default)]
    pub is_preset: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    pub agent: String,
    /// Either `"auto"` (let the router decide) or a concrete OpenRouter
    /// model id (e.g. `"anthropic/claude-sonnet-4"`).
    pub model: String,
    /// Max parallel instances permitted for this role. 1 unless the role
    /// is genuinely parallelizable (currently only coder is, per neo's
    /// pipeline). Values above 1 are advisory until neo's parallelism
    /// honours them.
    pub count: u8,
    /// Whether the member is active. `false` is "on the bench" — the
    /// editor lets users toggle agents on/off without losing the model
    /// they had configured.
    #[serde(default = "default_true")]
    pub included: bool,
    /// Optional free-text note for this role (e.g. "rust-only", "review
    /// security-heavy"). Surfaced in the SELECTED panel of the builder.
    #[serde(default)]
    pub notes: Option<String>,
}

fn default_true() -> bool {
    true
}

impl TeamMember {
    pub fn auto(agent: &str) -> Self {
        Self {
            agent: agent.to_string(),
            model: "auto".to_string(),
            count: 1,
            included: true,
            notes: None,
        }
    }
}

impl Team {
    /// Five ship-with-the-binary presets. Index 0 is the default.
    /// All are marked `is_preset = true` so the editor treats them as
    /// read-only (changes require save-as).
    pub fn presets() -> Vec<Team> {
        let mk = |name: &str, blurb: &str, members: Vec<TeamMember>| Team {
            name: name.to_string(),
            blurb: blurb.to_string(),
            members,
            is_preset: true,
        };

        let coder_x3 = TeamMember {
            agent: "coder".to_string(),
            model: "auto".to_string(),
            count: 3,
            included: true,
            notes: None,
        };
        let coder_llama = TeamMember {
            agent: "coder".to_string(),
            model: "llama3.2:latest".to_string(),
            count: 1,
            included: true,
            notes: None,
        };

        vec![
            mk(
                "balanced",
                "router → planner → coder ×1 → tester → reviewer",
                vec![
                    TeamMember::auto("router"),
                    TeamMember::auto("planner"),
                    TeamMember::auto("coder"),
                    TeamMember::auto("tester"),
                    TeamMember::auto("reviewer"),
                ],
            ),
            mk(
                "lean",
                "router → coder ×1 (skip planner/review)",
                vec![TeamMember::auto("router"), TeamMember::auto("coder")],
            ),
            mk(
                "scaled",
                "planner → coder ×3 || tester → reviewer → documenter",
                vec![
                    TeamMember::auto("router"),
                    TeamMember::auto("planner"),
                    coder_x3,
                    TeamMember::auto("tester"),
                    TeamMember::auto("reviewer"),
                    TeamMember::auto("documenter"),
                ],
            ),
            mk(
                "full",
                "all 8 agents available, router auto-picks per task",
                vec![
                    TeamMember::auto("router"),
                    TeamMember::auto("planner"),
                    TeamMember::auto("coder"),
                    TeamMember::auto("reviewer"),
                    TeamMember::auto("debugger"),
                    TeamMember::auto("tester"),
                    TeamMember::auto("documenter"),
                    TeamMember::auto("oracle"),
                ],
            ),
            mk(
                "local",
                "ollama only — free, offline, no API key",
                vec![coder_llama],
            ),
        ]
    }

    /// All 8 agent roles in canonical order, returning the existing
    /// member if present or a default `auto`/excluded skeleton if not.
    /// Used by the Hero Panel ROSTER to render every role consistently.
    pub fn full_roster(&self) -> Vec<TeamMember> {
        const ROLES: &[&str] = &[
            "router", "planner", "coder", "reviewer", "debugger", "tester",
            "documenter", "oracle",
        ];
        ROLES
            .iter()
            .map(|role| {
                self.members
                    .iter()
                    .find(|m| m.agent == *role)
                    .cloned()
                    .unwrap_or_else(|| TeamMember {
                        agent: role.to_string(),
                        model: "auto".to_string(),
                        count: 1,
                        included: false,
                        notes: None,
                    })
            })
            .collect()
    }

    /// Number of *included* members times their counts. Excluded
    /// members and members with included=false don't count.
    pub fn active_size(&self) -> u32 {
        self.members
            .iter()
            .filter(|m| m.included)
            .map(|m| m.count as u32)
            .sum()
    }

    /// Total roster size counting bench (excluded) members. Use
    /// `active_size()` for "agents that will actually fire on a task".
    pub fn total_size(&self) -> u32 {
        self.members
            .iter()
            .filter(|m| m.included)
            .map(|m| m.count as u32)
            .sum()
    }

    /// If any *included* member has a non-`auto` model assignment,
    /// return the first such model — we pass it as `NEO_DEFAULT_MODEL`.
    /// Returns `None` when the team is purely router-driven.
    pub fn override_model(&self) -> Option<&str> {
        self.members
            .iter()
            .filter(|m| m.included)
            .find(|m| m.model != "auto")
            .map(|m| m.model.as_str())
    }

    pub fn member_mut(&mut self, agent: &str) -> Option<&mut TeamMember> {
        self.members.iter_mut().find(|m| m.agent == agent)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TeamsFile {
    pub active: Option<String>,
    #[serde(default)]
    pub teams: Vec<Team>,
}

fn config_path() -> Result<PathBuf> {
    let base = dirs::config_dir().context("could not determine config dir")?;
    Ok(base.join("agentwatch").join("teams.toml"))
}

/// Load the persisted teams file. Missing file is fine — returns empty.
/// On disk we keep only user-defined teams; presets live in the binary
/// and are merged in `App::new()`.
pub fn load_teams_file() -> TeamsFile {
    let Ok(path) = config_path() else {
        return TeamsFile::default();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return TeamsFile::default();
    };
    toml::from_str(&text).unwrap_or_default()
}

/// Persist user-defined teams + active name. Presets are excluded so a
/// preset rename in a future binary version doesn't break user state.
pub fn save_teams_file(file: &TeamsFile) -> Result<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = toml::to_string_pretty(file)?;
    std::fs::write(&path, text)?;
    Ok(())
}

/// Backwards-compat shim — the old single-name file is auto-migrated on
/// first save_teams_file call. New callers use `load_teams_file` directly.
pub fn load_active_name() -> Option<String> {
    let file = load_teams_file();
    file.active
}
