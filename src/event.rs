use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app::Tab;

pub enum Action {
    Quit,
    SwitchTab(Tab),
    NextTab,
    PrevTab,
    Tick,
    SelectionUp,
    SelectionDown,
    Reload,
    SelectWorkflow(usize),
    PromptKey(KeyEvent),
    PromptSubmit,
    PromptCancel,
}

/// Read one event (or time out and emit `Tick`). The caller passes:
/// - `current_tab` so we know whether driver-tab semantics apply
/// - `prompt_is_empty` so single-letter quits (`q`, `Esc`) work on driver
///   tabs only when the user isn't mid-typing
pub fn poll_event(
    tick_rate: Duration,
    current_tab: Tab,
    prompt_is_empty: bool,
) -> Result<Option<Action>> {
    if !event::poll(tick_rate)? {
        return Ok(Some(Action::Tick));
    }

    let Event::Key(k) = event::read()? else {
        return Ok(None);
    };
    if k.kind != KeyEventKind::Press {
        return Ok(None);
    }

    if let Some(a) = global_hotkey(&k, current_tab, prompt_is_empty) {
        return Ok(Some(a));
    }

    if matches!(current_tab, Tab::Console | Tab::Thread) {
        Ok(Some(Action::PromptKey(k)))
    } else {
        Ok(observer_key(&k))
    }
}

fn global_hotkey(k: &KeyEvent, current_tab: Tab, prompt_is_empty: bool) -> Option<Action> {
    let driver = matches!(current_tab, Tab::Console | Tab::Thread);

    match (k.code, k.modifiers) {
        // ── Universal quits ─────────────────────────────────────────────
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => Some(Action::Quit),
        (KeyCode::Char('d'), KeyModifiers::CONTROL) if prompt_is_empty => Some(Action::Quit),
        // ── 'q' / Esc — quit on observer tabs always, on driver tabs
        //    only when the prompt is empty (otherwise they're prompt input
        //    / prompt-clear).
        (KeyCode::Char('q'), KeyModifiers::NONE) if !driver => Some(Action::Quit),
        (KeyCode::Char('q'), KeyModifiers::NONE) if driver && prompt_is_empty => {
            Some(Action::Quit)
        }
        (KeyCode::Esc, _) if !driver => Some(Action::Quit),
        (KeyCode::Esc, _) if driver && prompt_is_empty => Some(Action::Quit),
        (KeyCode::Esc, _) if driver => Some(Action::PromptCancel),

        // ── Tab navigation ──────────────────────────────────────────────
        (KeyCode::Char(c @ '0'..='9'), KeyModifiers::NONE) if !driver => {
            let n = c.to_digit(10).unwrap_or(0) as u8;
            Some(Action::SwitchTab(Tab::from_index(n)))
        }
        (KeyCode::Char(c @ '0'..='9'), KeyModifiers::ALT) => {
            let n = c.to_digit(10).unwrap_or(0) as u8;
            Some(Action::SwitchTab(Tab::from_index(n)))
        }
        (KeyCode::Char(c @ '1'..='6'), KeyModifiers::CONTROL) => {
            let n = c.to_digit(10).unwrap_or(1) as usize - 1;
            Some(Action::SelectWorkflow(n))
        }
        (KeyCode::Tab, KeyModifiers::NONE) => Some(Action::NextTab),
        (KeyCode::BackTab, _) => Some(Action::PrevTab),
        (KeyCode::F(5), _) => Some(Action::Reload),

        // ── Prompt submit / clear ───────────────────────────────────────
        (KeyCode::Enter, KeyModifiers::NONE) if driver => Some(Action::PromptSubmit),
        (KeyCode::Char('k'), KeyModifiers::CONTROL) => Some(Action::PromptCancel),

        _ => None,
    }
}

fn observer_key(k: &KeyEvent) -> Option<Action> {
    match (k.code, k.modifiers) {
        (KeyCode::Up, _) | (KeyCode::Char('k'), KeyModifiers::NONE) => Some(Action::SelectionUp),
        (KeyCode::Down, _) | (KeyCode::Char('j'), KeyModifiers::NONE) => Some(Action::SelectionDown),
        _ => None,
    }
}
