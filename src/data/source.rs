//! `NeoSource` is the producer side of the data layer.
//!
//! In v1 it polls `state.json` + tails `invocations.jsonl` (both written
//! upstream by neo — see TECHNICAL.md §"Data contract with neo"). When neo
//! ships `neo-agentd`, the `agentd` feature swaps the poller for a socket
//! reader without changing this trait.

use std::path::PathBuf;
use std::time::SystemTime;

use anyhow::Result;

use super::contract::StateSnapshot;
use super::paths;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeStatus {
    Online,
    Offline,
}

pub struct NeoSource {
    state_path: PathBuf,
    invocations_path: PathBuf,
    last_state_mtime: Option<SystemTime>,
}

impl NeoSource {
    pub fn new() -> Result<Self> {
        Ok(Self {
            state_path: paths::state_file()?,
            invocations_path: paths::invocations_log()?,
            last_state_mtime: None,
        })
    }

    /// Returns `Ok(Some(...))` if state changed since last call.
    /// Returns `Ok(None)` if unchanged or not yet present (runtime offline).
    pub fn poll_state(&mut self) -> Result<Option<StateSnapshot>> {
        let meta = match std::fs::metadata(&self.state_path) {
            Ok(m) => m,
            Err(_) => return Ok(None),
        };
        let mtime = meta.modified().ok();
        if mtime == self.last_state_mtime {
            return Ok(None);
        }
        self.last_state_mtime = mtime;

        let bytes = std::fs::read(&self.state_path)?;
        let snap: StateSnapshot = serde_json::from_slice(&bytes)?;
        Ok(Some(snap))
    }

    pub fn invocations_path(&self) -> &PathBuf {
        &self.invocations_path
    }
}
