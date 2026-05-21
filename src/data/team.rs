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
}

impl TeamMember {
    pub fn auto(agent: &str) -> Self {
        Self {
            agent: agent.to_string(),
            model: "auto".to_string(),
            count: 1,
        }
    }
}

impl Team {
    /// Three ship-with-the-binary presets. Index 0 is the default.
    pub fn presets() -> Vec<Team> {
        vec![
            Team {
                name: "balanced".to_string(),
                blurb: "router → planner → coder ×1 → tester → reviewer".to_string(),
                members: vec![
                    TeamMember::auto("router"),
                    TeamMember::auto("planner"),
                    TeamMember::auto("coder"),
                    TeamMember::auto("tester"),
                    TeamMember::auto("reviewer"),
                ],
            },
            Team {
                name: "lean".to_string(),
                blurb: "router → coder ×1 (skip planner/review)".to_string(),
                members: vec![
                    TeamMember::auto("router"),
                    TeamMember::auto("coder"),
                ],
            },
            Team {
                name: "scaled".to_string(),
                blurb: "planner → coder ×3 || tester → reviewer → documenter".to_string(),
                members: vec![
                    TeamMember::auto("router"),
                    TeamMember::auto("planner"),
                    TeamMember {
                        agent: "coder".to_string(),
                        model: "auto".to_string(),
                        count: 3,
                    },
                    TeamMember::auto("tester"),
                    TeamMember::auto("reviewer"),
                    TeamMember::auto("documenter"),
                ],
            },
            Team {
                name: "full".to_string(),
                blurb: "all 8 agents available, router auto-picks per task".to_string(),
                members: vec![
                    TeamMember::auto("router"),
                    TeamMember::auto("planner"),
                    TeamMember::auto("coder"),
                    TeamMember::auto("reviewer"),
                    TeamMember::auto("debugger"),
                    TeamMember::auto("tester"),
                    TeamMember::auto("documenter"),
                    TeamMember::auto("oracle"),
                ],
            },
            // Local-only preset — dispatches to ollama, never hits the
            // network or burns API credit. Edit /team set coder <model>
            // to pin a specific tag you've pulled (e.g. llama3.2:latest).
            Team {
                name: "local".to_string(),
                blurb: "ollama only — free, offline, no API key".to_string(),
                members: vec![
                    TeamMember {
                        agent: "coder".to_string(),
                        model: "llama3.2:latest".to_string(),
                        count: 1,
                    },
                ],
            },
        ]
    }

    pub fn total_size(&self) -> u32 {
        self.members.iter().map(|m| m.count as u32).sum()
    }

    /// If any member has a non-`auto` model assignment, return the first
    /// such model — we pass it as `NEO_DEFAULT_MODEL`. Returns `None`
    /// when the team is purely router-driven.
    pub fn override_model(&self) -> Option<&str> {
        self.members
            .iter()
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
