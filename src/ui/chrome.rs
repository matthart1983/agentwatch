use chrono::Local;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{App, Tab};
use crate::theme;

pub fn header(f: &mut Frame, area: Rect, app: &App) {
    let now = Local::now().format("%H:%M:%S").to_string();
    let live = if app.runtime_online { "● LIVE" } else { "○ OFFLINE" };
    let live_color = if app.runtime_online {
        theme::GREEN
    } else {
        theme::DIM
    };

    let line = Line::from(vec![
        Span::styled("agentwatch", Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD)),
        Span::styled("  v0.1.0", Style::default().fg(theme::DIM)),
        Span::raw("   "),
        Span::styled("session: —", Style::default().fg(theme::DIM)),
        Span::raw("   "),
        Span::styled("spend $0.00 / $20.00", Style::default().fg(theme::DIM)),
        Span::raw("   "),
        Span::styled(live, Style::default().fg(live_color)),
        Span::raw("   "),
        Span::styled(now, Style::default().fg(theme::DIM)),
    ]);

    f.render_widget(Paragraph::new(line), area);
}

pub fn tabs(f: &mut Frame, area: Rect, app: &App) {
    let mut spans = Vec::new();
    for tab in Tab::ALL.iter() {
        let is_active = *tab == app.current_tab;
        let style = if is_active {
            Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::DIM)
        };
        let label = format!("[{}] {}", tab.footer_digit(), tab.label());
        spans.push(Span::styled(label, style));
        spans.push(Span::raw("  "));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

pub fn footer(f: &mut Frame, area: Rect, _app: &App) {
    let agent = Style::default().fg(theme::GREEN);
    let mod_ = Style::default().fg(theme::CYAN);
    let sess = Style::default().fg(theme::YELLOW);
    let nav = Style::default().fg(theme::DIM);
    let sep = Span::styled("  │  ", Style::default().fg(theme::FAINT));

    let line1 = Line::from(vec![
        Span::styled("enter:Send  esc:Clear/Quit  ^k:Cancel  ^c:Quit", agent),
        sep.clone(),
        Span::styled("^1-6:Workflow  alt+1-9 0:Tab", mod_),
        sep.clone(),
        Span::styled("F5:Reload", sess),
        sep,
        Span::styled("q:Quit  1-9 0:Tab  ?:Help", nav),
    ]);

    f.render_widget(Paragraph::new(vec![line1, Line::raw("")]), area);
}
