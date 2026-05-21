pub mod agents;
pub mod bot;
pub mod chrome;
pub mod console;
pub mod cost;
pub mod insights;
pub mod models;
pub mod overview;
pub mod plans;
pub mod sessions;
pub mod team_builder;
pub mod thread;
pub mod tools;

use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};

use crate::app::{App, Tab};

pub fn render(f: &mut Frame, app: &App) {
    // Hero Panel takes over the whole frame when open.
    if app.builder_is_open() {
        team_builder::render(f, app);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // row 0: header
            Constraint::Length(2), // rows 1-2: tab bar
            Constraint::Min(0),    // rows 3-32: content
            Constraint::Length(2), // rows 34-35: footer
        ])
        .split(f.size());

    chrome::header(f, chunks[0], app);
    chrome::tabs(f, chunks[1], app);

    match app.current_tab {
        Tab::Console => console::render(f, chunks[2], app),
        Tab::Thread => thread::render(f, chunks[2], app),
        Tab::Agents => agents::render(f, chunks[2], app),
        Tab::Plans => plans::render(f, chunks[2], app),
        Tab::Sessions => sessions::render(f, chunks[2], app),
        Tab::Tools => tools::render(f, chunks[2], app),
        Tab::Models => models::render(f, chunks[2], app),
        Tab::Cost => cost::render(f, chunks[2], app),
        Tab::Overview => overview::render(f, chunks[2], app),
        Tab::Insights => insights::render(f, chunks[2], app),
    }

    chrome::footer(f, chunks[3], app);
}
