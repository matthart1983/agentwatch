use std::path::PathBuf;

use anyhow::{Context, Result};
use uuid::Uuid;

use crate::data::contract::ControlCommand;
use crate::data::paths;

pub struct InboxWriter {
    dir: PathBuf,
}

impl InboxWriter {
    pub fn new() -> Result<Self> {
        let dir = paths::inbox_dir()?;
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create neo inbox: {}", dir.display()))?;
        Ok(Self { dir })
    }

    /// Drop one JSON file in neo's inbox. neo's filesystem watch picks it up
    /// and processes it as if it came from the CLI.
    pub fn send(&self, cmd: &ControlCommand) -> Result<PathBuf> {
        let path = self.dir.join(format!("{}.json", Uuid::new_v4()));
        let json = serde_json::to_vec_pretty(cmd)?;
        std::fs::write(&path, json)
            .with_context(|| format!("failed to write control file: {}", path.display()))?;
        Ok(path)
    }
}
