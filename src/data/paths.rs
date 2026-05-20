use std::path::PathBuf;

use anyhow::{Context, Result};

/// On macOS this is `~/Library/Application Support/neo`, matching neo's own
/// `dirs::data_dir()` choice. On Linux it's `~/.local/share/neo`.
pub fn neo_data_dir() -> Result<PathBuf> {
    let base = dirs::data_dir().context("could not determine system data dir")?;
    Ok(base.join("neo"))
}

pub fn threads_dir() -> Result<PathBuf> {
    Ok(neo_data_dir()?.join("threads"))
}

pub fn state_file() -> Result<PathBuf> {
    Ok(neo_data_dir()?.join("state.json"))
}

pub fn invocations_log() -> Result<PathBuf> {
    Ok(neo_data_dir()?.join("invocations.jsonl"))
}

pub fn inbox_dir() -> Result<PathBuf> {
    Ok(neo_data_dir()?.join("inbox"))
}

pub fn agentd_socket() -> Result<PathBuf> {
    Ok(neo_data_dir()?.join("neo-agentd.sock"))
}
