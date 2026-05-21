//! Hero Panel state machine. While `App.builder` is `Some`, the UI
//! renders the team-builder overlay instead of the regular tab content,
//! and key events are routed through `builder_key()` rather than the
//! per-tab handler.

use std::time::SystemTime;

use crate::data::team::Team;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuilderFocus {
    Roster,
    Models,
    Presets,
    /// Capturing keystrokes for a "save as" name input. Held in
    /// `BuilderState.naming`.
    Naming,
}

#[derive(Debug, Clone)]
pub struct BuilderState {
    /// Working copy — edits go here, original stays unchanged until save.
    pub editing: Team,
    /// Snapshot taken on open so Esc-twice can revert.
    pub original: Team,
    pub focus: BuilderFocus,
    /// Currently highlighted roster row (0..8).
    pub roster_idx: usize,
    /// Currently highlighted row in the model picker.
    pub models_idx: usize,
    /// Currently highlighted preset row.
    pub presets_idx: usize,
    /// Partial name when the user has hit `s` on a preset / new team.
    /// `None` when not in the naming flow.
    pub naming: Option<String>,
    /// Last esc press — used for the "press esc again to discard" dance.
    pub last_esc_at: Option<SystemTime>,
}

impl BuilderState {
    pub fn new(team: Team, presets_idx: usize) -> Self {
        Self {
            editing: team.clone(),
            original: team,
            focus: BuilderFocus::Roster,
            roster_idx: 0,
            models_idx: 0,
            presets_idx,
            naming: None,
            last_esc_at: None,
        }
    }

    /// Has the working copy diverged from the original?
    pub fn is_dirty(&self) -> bool {
        // Compare via JSON because Team is small and PartialEq isn't
        // derived on its fields.
        serde_json::to_string(&self.editing).ok()
            != serde_json::to_string(&self.original).ok()
    }

    pub fn toggle_included(&mut self) {
        let roster = self.editing.full_roster();
        let Some(target) = roster.get(self.roster_idx) else {
            return;
        };
        let agent = target.agent.clone();
        // Find or insert the corresponding member, then flip.
        match self.editing.members.iter_mut().find(|m| m.agent == agent) {
            Some(existing) => {
                existing.included = !existing.included;
                if existing.included && existing.count == 0 {
                    existing.count = 1;
                }
            }
            None => {
                let mut new_member = target.clone();
                new_member.included = true;
                self.editing.members.push(new_member);
            }
        }
    }

    pub fn adjust_count(&mut self, delta: i32) {
        let roster = self.editing.full_roster();
        let Some(target) = roster.get(self.roster_idx) else {
            return;
        };
        let agent = target.agent.clone();
        if let Some(m) = self.editing.members.iter_mut().find(|m| m.agent == agent) {
            let new_count = (m.count as i32 + delta).clamp(1, 5);
            m.count = new_count as u8;
        } else if delta > 0 {
            let mut nm = target.clone();
            nm.included = true;
            nm.count = 1;
            self.editing.members.push(nm);
        }
    }

    pub fn set_selected_model(&mut self, model: &str) {
        let roster = self.editing.full_roster();
        let Some(target) = roster.get(self.roster_idx) else {
            return;
        };
        let agent = target.agent.clone();
        match self.editing.members.iter_mut().find(|m| m.agent == agent) {
            Some(m) => m.model = model.to_string(),
            None => {
                let mut nm = target.clone();
                nm.model = model.to_string();
                nm.included = true;
                self.editing.members.push(nm);
            }
        }
    }

    /// Selected roster row's TeamMember (synthesised if not on team yet).
    pub fn selected_member(&self) -> Option<crate::data::team::TeamMember> {
        self.editing.full_roster().into_iter().nth(self.roster_idx)
    }

    pub fn roster_up(&mut self) {
        self.roster_idx = self.roster_idx.saturating_sub(1);
    }
    pub fn roster_down(&mut self) {
        if self.roster_idx + 1 < 8 {
            self.roster_idx += 1;
        }
    }

    pub fn focus_next(&mut self) {
        self.focus = match self.focus {
            BuilderFocus::Roster => BuilderFocus::Models,
            BuilderFocus::Models => BuilderFocus::Presets,
            BuilderFocus::Presets => BuilderFocus::Roster,
            BuilderFocus::Naming => BuilderFocus::Naming,
        };
    }

    pub fn focus_prev(&mut self) {
        self.focus = match self.focus {
            BuilderFocus::Roster => BuilderFocus::Presets,
            BuilderFocus::Models => BuilderFocus::Roster,
            BuilderFocus::Presets => BuilderFocus::Models,
            BuilderFocus::Naming => BuilderFocus::Naming,
        };
    }
}
