use std::collections::HashMap;

use chrono::TimeZone;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, AGENT_ORDER};
use crate::data::InvocationRecord;
use crate::theme;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // strip
            Constraint::Min(0),    // 2-col body
            Constraint::Length(3), // footer note
        ])
        .split(area);

    summary_strip(f, chunks[0], app);

    let row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(70), Constraint::Min(0)])
        .split(chunks[1]);
    per_agent_table(f, row[0], app);
    recent_invocations_with_tools(f, row[1], app);

    sandbox_note(f, chunks[2]);
}

fn summary_strip(f: &mut Frame, area: Rect, app: &App) {
    let total_calls: u32 = app
        .invocations
        .today()
        .map(|r| r.tool_calls)
        .sum();
    let invs_with_tools = app
        .invocations
        .today()
        .filter(|r| r.tool_calls > 0)
        .count();
    let total_invs_today = app.invocations.calls_today();

    let dim = Style::default().fg(theme::DIM);
    let cyan_bold = Style::default()
        .fg(theme::CYAN)
        .add_modifier(Modifier::BOLD);
    let fg = Style::default().fg(theme::FG);

    let line = Line::from(vec![
        Span::styled(" total ", dim),
        Span::styled(format!(" {} ", total_calls), cyan_bold),
        Span::styled("tool calls today", fg),
        Span::styled(
            format!(
                "   across {} of {} invocations",
                invs_with_tools, total_invs_today
            ),
            dim,
        ),
        Span::styled(
            "    per-tool breakdown blocked on separate neo PR",
            dim,
        ),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn per_agent_table(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::FAINT))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "TOOL USE",
                Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  by agent  today ", Style::default().fg(theme::DIM)),
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 2 {
        return;
    }

    let header = Paragraph::new(Line::from(vec![Span::styled(
        "  AGENT        TOOL CALLS   INVOCATIONS   AVG/CALL   AVG LATENCY",
        Style::default().fg(theme::DIM),
    )]));
    f.render_widget(header, Rect { height: 1, ..inner });

    let rows_area = Rect {
        y: inner.y + 1,
        height: inner.height - 1,
        ..inner
    };

    let rollups = rollup_by_agent(app);
    let mut lines = Vec::new();
    let total: u32 = rollups.values().map(|r| r.tool_calls).sum();
    if total == 0 {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "  (no tool calls recorded yet)",
                Style::default().fg(theme::DIM),
            ))),
            rows_area,
        );
        return;
    }

    for name in AGENT_ORDER.iter() {
        let r = rollups.get(*name);
        let calls = r.map(|x| x.tool_calls).unwrap_or(0);
        let invs = r.map(|x| x.invocations).unwrap_or(0);
        let avg_per = if invs > 0 {
            calls as f32 / invs as f32
        } else {
            0.0
        };
        let avg_lat = r.map(|x| x.avg_latency_ms).unwrap_or(0);
        let dot_color = if calls > 0 { theme::GREEN } else { theme::DIM };
        let text_color = if calls > 0 { theme::FG } else { theme::DIM };
        lines.push(Line::from(vec![
            Span::styled("  ●  ", Style::default().fg(dot_color)),
            Span::styled(
                format!("{:<10}  ", capitalize(name)),
                Style::default().fg(text_color),
            ),
            Span::styled(
                format!("{:>10}    ", calls),
                Style::default().fg(text_color),
            ),
            Span::styled(
                format!("{:>10}    ", invs),
                Style::default().fg(theme::DIM),
            ),
            Span::styled(
                format!("{:>6.1}    ", avg_per),
                Style::default().fg(text_color),
            ),
            Span::styled(
                format!("{:>10}", format_latency(avg_lat)),
                Style::default().fg(theme::DIM),
            ),
        ]));
    }
    f.render_widget(Paragraph::new(lines), rows_area);
}

fn recent_invocations_with_tools(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::FAINT))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "RECENT",
                Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "  invocations w/ tool calls ",
                Style::default().fg(theme::DIM),
            ),
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let recent: Vec<&InvocationRecord> = app
        .invocations
        .records
        .iter()
        .rev()
        .filter(|r| r.tool_calls > 0)
        .take(inner.height as usize)
        .collect();

    if recent.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "  (no recent tool-using invocations)",
                Style::default().fg(theme::DIM),
            ))),
            inner,
        );
        return;
    }

    let mut lines = Vec::new();
    for r in recent {
        let time = chrono::Local
            .from_utc_datetime(&r.ts.naive_utc())
            .format("%H:%M:%S")
            .to_string();
        lines.push(Line::from(vec![
            Span::styled(" ● ", Style::default().fg(theme::GREEN)),
            Span::styled(format!("{}  ", time), Style::default().fg(theme::DIM)),
            Span::styled(
                format!("{:<10}  ", r.agent),
                Style::default().fg(theme::FG).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("× {} tools  ", r.tool_calls),
                Style::default().fg(theme::CYAN),
            ),
            Span::styled(
                format!("· {} · ${:.4}", short_model(&r.model), r.cost),
                Style::default().fg(theme::DIM),
            ),
        ]));
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn sandbox_note(f: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::FAINT));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let lines = vec![
        Line::from(vec![
            Span::styled(" SANDBOX  ", Style::default().fg(theme::DIM).add_modifier(Modifier::BOLD)),
            Span::styled(
                "neo's tools are workspace-scoped; shell tool prompts for confirmation by default.",
                Style::default().fg(theme::FG),
            ),
        ]),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

struct AgentRollup {
    invocations: u32,
    tool_calls: u32,
    avg_latency_ms: u32,
}

fn rollup_by_agent(app: &App) -> HashMap<String, AgentRollup> {
    let mut out: HashMap<String, AgentRollup> = HashMap::new();
    for r in app.invocations.today() {
        let entry = out.entry(r.agent.clone()).or_insert(AgentRollup {
            invocations: 0,
            tool_calls: 0,
            avg_latency_ms: 0,
        });
        let prior_total = entry.avg_latency_ms as u64 * entry.invocations as u64;
        entry.invocations += 1;
        entry.tool_calls += r.tool_calls;
        let new_total = prior_total + r.latency_ms as u64;
        entry.avg_latency_ms = (new_total / entry.invocations as u64) as u32;
    }
    out
}

fn short_model(model: &str) -> String {
    model.split('/').next_back().unwrap_or(model).to_string()
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

fn format_latency(ms: u32) -> String {
    if ms == 0 {
        "—".to_string()
    } else if ms < 1000 {
        format!("{}ms", ms)
    } else {
        format!("{:.1}s", ms as f64 / 1000.0)
    }
}
