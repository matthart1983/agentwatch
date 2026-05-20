use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::app::App;
use crate::data::{insights::compute, Insight, Severity};
use crate::theme;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let insights = compute(&app.invocations, &app.threads);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // strip
            Constraint::Min(0),    // cards
            Constraint::Length(1), // footer note
        ])
        .split(area);

    summary_strip(f, chunks[0], &insights);
    cards(f, chunks[1], &insights);
    footer_note(f, chunks[2]);
}

fn summary_strip(f: &mut Frame, area: Rect, insights: &[Insight]) {
    let crits = insights.iter().filter(|i| i.severity == Severity::Crit).count();
    let warns = insights.iter().filter(|i| i.severity == Severity::Warn).count();
    let infos = insights.iter().filter(|i| i.severity == Severity::Info).count();

    let dim = Style::default().fg(theme::DIM);
    let (dot_color, label_color) = if crits > 0 {
        (theme::RED, theme::RED)
    } else if warns > 0 {
        (theme::YELLOW, theme::YELLOW)
    } else {
        (theme::GREEN, theme::DIM)
    };

    let line = Line::from(vec![
        Span::styled(" ● ", Style::default().fg(dot_color)),
        Span::styled(
            format!("{} active", insights.len()),
            Style::default().fg(label_color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("   {} crit, {} warn, {} info", crits, warns, infos),
            dim,
        ),
        Span::styled("    rules run every tick", dim),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn cards(f: &mut Frame, area: Rect, insights: &[Insight]) {
    if insights.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "  (no active insights — system looks healthy)",
                Style::default().fg(theme::DIM),
            ))),
            area,
        );
        return;
    }

    let card_h = 6u16;
    let mut y = area.y;
    for ins in insights {
        if y + card_h > area.y + area.height {
            break;
        }
        let card_area = Rect {
            x: area.x,
            y,
            width: area.width,
            height: card_h,
        };
        draw_card(f, card_area, ins);
        y += card_h;
    }
}

fn draw_card(f: &mut Frame, area: Rect, ins: &Insight) {
    let (sev_color, sev_bg) = severity_colors(ins.severity);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::FAINT));
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Colored left edge
    let edge_area = Rect { width: 1, ..area };
    let edge: Vec<Line<'static>> = (0..edge_area.height)
        .map(|_| Line::from(Span::styled("│", Style::default().fg(sev_color))))
        .collect();
    f.render_widget(Paragraph::new(edge), edge_area);

    let dim = Style::default().fg(theme::DIM);
    let fg = Style::default().fg(theme::FG);
    let title_style = Style::default()
        .fg(theme::FG)
        .add_modifier(Modifier::BOLD);

    let badge = Span::styled(
        format!(" {} ", ins.severity.label()),
        Style::default()
            .fg(sev_color)
            .bg(sev_bg)
            .add_modifier(Modifier::BOLD),
    );

    let title_line = Line::from(vec![
        Span::raw(" "),
        badge,
        Span::raw("  "),
        Span::styled(ins.title.clone(), title_style),
    ]);

    let mut lines = vec![title_line];
    for body in &ins.body {
        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(body.clone(), fg),
        ]));
    }
    while lines.len() < (inner.height as usize).saturating_sub(1) {
        lines.push(Line::raw(""));
    }
    if let Some(tab) = ins.suggested_tab {
        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(format!("→ open {} tab", tab), dim),
        ]));
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn footer_note(f: &mut Frame, area: Rect) {
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "  Insights are read-only — they never modify agents, models, tools, or sessions.",
            Style::default().fg(theme::DIM),
        ))),
        area,
    );
}

fn severity_colors(s: Severity) -> (Color, Color) {
    match s {
        Severity::Crit => (theme::RED, theme::WARN_BG),
        Severity::Warn => (theme::YELLOW, theme::WARN_BG),
        Severity::Info => (theme::CYAN, theme::SEL_BG),
    }
}
