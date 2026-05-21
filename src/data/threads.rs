//! Threads loader. Reads `<neo_data_dir>/threads/T-*.json` and produces a
//! sorted `Vec<ThreadSummary>` for the Sessions tab.
//!
//! This is independent of the live state contract — neo already writes
//! these files today (`src/session/manager.rs`) so M2 doesn't require any
//! upstream changes.

use std::path::Path;

use anyhow::Result;
use serde::Deserialize;

use super::contract::ThreadSummary;
use super::paths;

#[derive(Deserialize)]
struct ThreadOnDisk {
    id: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    workspace: String,
    #[serde(default)]
    cost_total: f64,
    #[serde(default)]
    models_used: Vec<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    messages: Vec<RawMessage>,
}

#[derive(Deserialize)]
struct RawMessage {
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    content: Option<String>,
}

pub fn load_all() -> Result<Vec<ThreadSummary>> {
    let dir = paths::threads_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }
    load_from_dir(&dir)
}

/// A thread with its full message history. Used by resume.
#[derive(Debug, Clone)]
pub struct FullThread {
    pub id: String,
    pub workspace: String,
    pub turns: Vec<Turn>,
}

#[derive(Debug, Clone)]
pub struct Turn {
    pub user: String,
    pub assistant: String,
}

/// Load one thread by id and return its user/assistant turns in order.
/// System messages and tool messages are skipped — they're internal scaffolding
/// and we want what the user actually saw.
pub fn load_one(id: &str) -> Result<FullThread> {
    let dir = paths::threads_dir()?;
    let path = dir.join(format!("{}.json", id));
    let bytes = std::fs::read(&path)?;
    let raw: ThreadOnDisk = serde_json::from_slice(&bytes)?;

    let mut turns = Vec::new();
    let mut pending_user: Option<String> = None;
    for msg in raw.messages {
        let role = msg.role.unwrap_or_default();
        let content = msg.content.unwrap_or_default();
        if content.is_empty() {
            continue;
        }
        match role.as_str() {
            "user" => {
                // If the previous user message wasn't answered, drop it
                // (it was probably the same turn that's about to repeat).
                pending_user = Some(content);
            }
            "assistant" => {
                if let Some(user_text) = pending_user.take() {
                    turns.push(Turn {
                        user: user_text,
                        assistant: content,
                    });
                }
            }
            _ => {}
        }
    }
    Ok(FullThread {
        id: raw.id,
        workspace: raw.workspace,
        turns,
    })
}

fn load_from_dir(dir: &Path) -> Result<Vec<ThreadSummary>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let raw: ThreadOnDisk = match serde_json::from_slice(&bytes) {
            Ok(t) => t,
            Err(_) => continue,
        };

        let first_user_message = raw
            .messages
            .iter()
            .find(|m| m.role.as_deref() == Some("user"))
            .and_then(|m| m.content.clone());

        out.push(ThreadSummary {
            id: raw.id,
            created_at: raw.created_at,
            updated_at: raw.updated_at,
            workspace: raw.workspace,
            cost_total: raw.cost_total,
            models_used: raw.models_used,
            tags: raw.tags,
            message_count: raw.messages.len(),
            first_user_message,
        });
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(out)
}
