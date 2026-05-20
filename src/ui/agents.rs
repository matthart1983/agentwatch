use std::collections::HashMap;

use chrono::Utc;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::{App, AGENT_ORDER};
use crate::data::AgentAgg;
use crate::theme;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // show strip
            Constraint::Length(16), // AGENTS box
            Constraint::Min(0),     // drill-in
        ])
        .split(area);

    show_strip(f, chunks[0], app);
    agents_table(f, chunks[1], app);
    drill_in(f, chunks[2], app);
}

fn show_strip(f: &mut Frame, area: Rect, app: &App) {
    let rows = build_rows(app);
    let active = rows.iter().filter(|r| r.calls > 0).count();
    let idle = AGENT_ORDER.len() - active;

    let dim = Style::default().fg(theme::DIM);
    let on = Style::default()
        .bg(theme::SEL_BG)
        .fg(theme::FG)
        .add_modifier(Modifier::BOLD);
    let fg = Style::default().fg(theme::FG);

    let spans = vec![
        Span::styled(" show ", dim),
        Span::styled(format!(" all {} ", AGENT_ORDER.len()), on),
        Span::raw("  "),
        Span::styled(format!("active {}", active), fg),
        Span::raw("  "),
        Span::styled(format!("idle {}", idle), fg),
        Span::raw("   "),
        Span::styled(
            "live state lands with neo PR #2 (StateEmitter)",
            dim,
        ),
    ];
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn agents_table(f: &mut Frame, area: Rect, app: &App) {
    let rows = build_rows(app);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::FAINT))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "AGENTS",
                Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  8 built-in ", Style::default().fg(theme::DIM)),
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 2 {
        return;
    }

    let header = Paragraph::new(Line::from(vec![Span::styled(
        "  AGENT        STATE        LAST MODEL                       CALLS/D   COST/D    AVG LAT",
        Style::default().fg(theme::DIM),
    )]));
    f.render_widget(header, Rect { height: 1, ..inner });

    let rows_area = Rect {
        y: inner.y + 1,
        height: inner.height - 1,
        ..inner
    };

    let mut lines = Vec::new();
    for (i, r) in rows.iter().enumerate() {
        lines.push(agent_line(r, i == app.agents_selected));
    }
    f.render_widget(Paragraph::new(lines), rows_area);
}

fn agent_line<'a>(r: &'a AgentRow, selected: bool) -> Line<'a> {
    let base = if selected {
        Style::default().bg(theme::SEL_BG).fg(theme::FG)
    } else {
        Style::default().fg(theme::FG)
    };
    let dot_color = if r.calls > 0 { theme::GREEN } else { theme::DIM };
    let dot_style = if selected {
        Style::default().bg(theme::SEL_BG).fg(dot_color)
    } else {
        Style::default().fg(dot_color)
    };
    let dim = if selected {
        Style::default().bg(theme::SEL_BG).fg(theme::DIM)
    } else {
        Style::default().fg(theme::DIM)
    };

    let state_label = if r.calls == 0 { "idle" } else { "—" };
    let model_label = r
        .last_model
        .as_deref()
        .map(short_model)
        .unwrap_or_else(|| "—".to_string());

    Line::from(vec![
        Span::styled("  ●  ", dot_style),
        Span::styled(format!("{:<10}  ", capitalize(&r.agent)), base),
        Span::styled(format!("{:<10}  ", state_label), dim),
        Span::styled(format!("{:<32}  ", truncate(&model_label, 32)), base),
        Span::styled(format!("{:>5}     ", r.calls), base),
        Span::styled(format!("${:>5.2}  ", r.cost), base),
        Span::styled(format!("{:>7}", format_latency(r.avg_latency_ms)), dim),
    ])
}

fn drill_in(f: &mut Frame, area: Rect, app: &App) {
    let rows = build_rows(app);
    let selected = rows.get(app.agents_selected);

    let title = selected
        .map(|r| format!(" {}  DETAIL ", capitalize(&r.agent)))
        .unwrap_or_else(|| " DETAIL ".to_string());

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::FAINT))
        .title(Line::from(Span::styled(
            title,
            Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(r) = selected else {
        return;
    };

    let dim = Style::default().fg(theme::DIM);
    let fg = Style::default().fg(theme::FG);

    let last_seen = r
        .last_seen
        .map(|t| {
            let age = Utc::now() - t;
            if age.num_seconds() < 60 {
                format!("{}s ago", age.num_seconds().max(0))
            } else if age.num_minutes() < 60 {
                format!("{}m ago", age.num_minutes())
            } else if age.num_hours() < 48 {
                format!("{}h ago", age.num_hours())
            } else {
                format!("{}d ago", age.num_days())
            }
        })
        .unwrap_or_else(|| "—".to_string());

    let lines = vec![
        kv("calls today", format!("{}", r.calls), dim, fg),
        kv("cost today", format!("${:.4}", r.cost), dim, fg),
        kv("avg latency", format_latency(r.avg_latency_ms), dim, fg),
        kv("last model", r.last_model.clone().unwrap_or_else(|| "—".to_string()), dim, fg),
        kv("last seen", last_seen, dim, fg),
        Line::raw(""),
        Line::from(vec![Span::styled(
            "  live state (current iteration, current tool, system prompt) lands with neo PR #2",
            dim,
        )]),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

fn kv<'a>(k: &'a str, v: String, k_style: Style, v_style: Style) -> Line<'a> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(format!("{:<14}", k), k_style),
        Span::styled(v, v_style),
    ])
}

struct AgentRow {
    agent: String,
    calls: usize,
    cost: f64,
    avg_latency_ms: u32,
    last_model: Option<String>,
    last_seen: Option<chrono::DateTime<Utc>>,
}

fn build_rows(app: &App) -> Vec<AgentRow> {
    let aggs: HashMap<String, AgentAgg> = app
        .invocations
        .by_agent_today()
        .into_iter()
        .map(|a| (a.agent.clone(), a))
        .collect();

    // Last model per agent — walk records in reverse.
    let mut last_model: HashMap<String, String> = HashMap::new();
    for r in app.invocations.records.iter().rev() {
        last_model.entry(r.agent.clone()).or_insert_with(|| r.model.clone());
    }

    AGENT_ORDER
        .iter()
        .map(|name| {
            let agg = aggs.get(*name);
            AgentRow {
                agent: name.to_string(),
                calls: agg.map(|a| a.calls).unwrap_or(0),
                cost: agg.map(|a| a.cost).unwrap_or(0.0),
                avg_latency_ms: agg.map(|a| a.avg_latency_ms).unwrap_or(0),
                last_model: last_model.get(*name).cloned(),
                last_seen: agg.and_then(|a| a.last_seen),
            }
        })
        .collect()
}

fn short_model(model: &str) -> String {
    model.split('/').next_back().unwrap_or(model).to_string()
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let kept: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", kept)
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(first) => first.to_uppercase().chain(c).collect(),
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

#[allow(dead_code)]
fn _dot_color() -> Color {
    theme::GREEN
}
