//! Outbound control channel. Writes `ControlCommand`s either to
//! `~/Library/Application Support/neo/inbox/<uuid>.json` (default fallback)
//! or to the neo-agentd Unix socket when the `agentd` feature is enabled.

pub mod inbox;
pub mod runner;

pub use inbox::InboxWriter;
pub use runner::{JobEvent, JobId, LineSource, Runner};
