use chrono::{Duration, Utc};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::{App, AGENT_ORDER};
use crate::data::InvocationRecord;
use crate::theme;

const DAY_CAP: f64 = 20.0;
const SESSION_CAP: f64 = 5.0;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),  // KPI tiles
            Constraint::Length(12), // agent strip + pipeline
            Constraint::Length(8),  // recent invocations + insights
            Constraint::Min(0),     // budget strip
        ])
        .split(area);

    kpi_tiles(f, chunks[0], app);

    let mid = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(80), Constraint::Min(0)])
        .split(chunks[1]);
    agent_strip(f, mid[0], app);
    pipeline_mini(f, mid[1], app);

    let bot = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(80), Constraint::Min(0)])
        .split(chunks[2]);
    recent_invocations(f, bot[0], app);
    insights_strip(f, bot[1], app);

    budget_strip(f, chunks[3], app);
}

fn kpi_tiles(f: &mut Frame, area: Rect, app: &App) {
    let active = AGENT_ORDER
        .iter()
        .filter(|name| {
            app.invocations
                .by_agent_today()
                .iter()
                .any(|a| a.agent == **name && a.calls > 0)
        })
        .count();
    let total_agents = AGENT_ORDER.len();
    let cost_today = app.invocations.total_cost_today();
    let pct_of_cap = if DAY_CAP > 0.0 {
        (cost_today / DAY_CAP * 100.0) as u32
    } else {
        0
    };
    let recent_invs: Vec<&InvocationRecord> = app
        .invocations
        .in_window(Utc::now() - Duration::hours(1))
        .collect();
    let calls_per_min = recent_invs.len() / 60;

    let tiles = [
        (
            "ACTIVE AGENTS",
            format!("{}/{}", active, total_agents),
            "out of 8 built-in".to_string(),
            theme::GREEN,
        ),
        (
            "CALLS/MIN",
            format!("{}", calls_per_min),
            format!("{} in last hour", recent_invs.len()),
            if calls_per_min > 0 {
                theme::GREEN
            } else {
                theme::DIM
            },
        ),
        (
            "COST TODAY",
            format!("${:.2}", cost_today),
            format!("{}% of ${:.0} cap", pct_of_cap, DAY_CAP),
            if pct_of_cap > 80 {
                theme::RED
            } else if pct_of_cap > 50 {
                theme::YELLOW
            } else {
                theme::GREEN
            },
        ),
        (
            "FALLBACKS",
            "—".to_string(),
            "tracking lands w/ router fb".to_string(),
            theme::DIM,
        ),
        (
            "INSIGHTS",
            "0".to_string(),
            "engine in [0]".to_string(),
            theme::DIM,
        ),
    ];

    let tw = area.width / 5;
    for (i, (label, value, sub, color)) in tiles.iter().enumerate() {
        let cell = Rect {
            x: area.x + i as u16 * tw,
            y: area.y,
            width: tw,
            height: area.height,
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::FAINT))
            .title(Line::from(Span::styled(
                format!(" {} ", label),
                Style::default().fg(theme::DIM),
            )));
        let inner = block.inner(cell);
        f.render_widget(block, cell);
        f.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    Span::styled(" ● ", Style::default().fg(*color)),
                    Span::styled(
                        value.to_string(),
                        Style::default()
                            .fg(theme::FG)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(vec![Span::styled(
                    format!(" {}", sub),
                    Style::default().fg(theme::DIM),
                )]),
            ]),
            inner,
        );
    }
}

fn agent_strip(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::FAINT))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "AGENT STRIP",
                Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  today ", Style::default().fg(theme::DIM)),
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let header = Paragraph::new(Line::from(vec![Span::styled(
        "  AGENT        STATE     LAST MODEL                       CALLS    COST",
        Style::default().fg(theme::DIM),
    )]));
    f.render_widget(header, Rect { height: 1, ..inner });

    let body_area = Rect {
        y: inner.y + 1,
        height: inner.height - 1,
        ..inner
    };

    let aggs = app.invocations.by_agent_today();
    let mut lines = Vec::new();
    for name in AGENT_ORDER.iter() {
        let agg = aggs.iter().find(|a| a.agent == *name);
        let calls = agg.map(|a| a.calls).unwrap_or(0);
        let cost = agg.map(|a| a.cost).unwrap_or(0.0);
        let last_model = app
            .invocations
            .records
            .iter()
            .rev()
            .find(|r| r.agent == *name)
            .map(|r| short_model(&r.model))
            .unwrap_or_else(|| "—".to_string());
        let state = if calls > 0 { "—" } else { "idle" };
        let dot = if calls > 0 { theme::GREEN } else { theme::DIM };

        lines.push(Line::from(vec![
            Span::styled("  ●  ", Style::default().fg(dot)),
            Span::styled(format!("{:<10}  ", capitalize(name)), Style::default().fg(theme::FG)),
            Span::styled(format!("{:<8}  ", state), Style::default().fg(theme::DIM)),
            Span::styled(format!("{:<32}  ", truncate(&last_model, 32)), Style::default().fg(theme::FG)),
            Span::styled(format!("{:>4}  ", calls), Style::default().fg(theme::FG)),
            Span::styled(format!("${:>5.2}", cost), Style::default().fg(theme::FG)),
        ]));
    }
    f.render_widget(Paragraph::new(lines), body_area);
}

fn pipeline_mini(f: &mut Frame, area: Rect, _app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::FAINT))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "PIPELINE",
                Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  live ", Style::default().fg(theme::DIM)),
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "  (no active pipeline)",
                Style::default().fg(theme::DIM),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "  live DAG lands when neo PR #4",
                Style::default().fg(theme::DIM),
            )),
            Line::from(Span::styled(
                "  (PipelineEvents) is wired into",
                Style::default().fg(theme::DIM),
            )),
            Line::from(Span::styled(
                "  the orchestrator main loop",
                Style::default().fg(theme::DIM),
            )),
        ]),
        inner,
    );
}

fn recent_invocations(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::FAINT))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "RECENT INVOCATIONS",
                Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  newest first ", Style::default().fg(theme::DIM)),
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let header = Paragraph::new(Line::from(vec![Span::styled(
        "  TIME      AGENT       MODEL                       TOK     COST    LAT     STATUS",
        Style::default().fg(theme::DIM),
    )]));
    f.render_widget(header, Rect { height: 1, ..inner });

    let body_area = Rect {
        y: inner.y + 1,
        height: inner.height - 1,
        ..inner
    };

    let recent: Vec<&InvocationRecord> = app
        .invocations
        .records
        .iter()
        .rev()
        .take(body_area.height as usize)
        .collect();

    if recent.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                "  (no invocations recorded yet)",
                Style::default().fg(theme::DIM),
            )])),
            body_area,
        );
        return;
    }

    let mut lines = Vec::new();
    for r in recent {
        let dot = if r.status == "success" {
            theme::GREEN
        } else if r.status == "max_iterations" {
            theme::YELLOW
        } else {
            theme::RED
        };
        let time = chrono::Local
            .from_utc_datetime(&r.ts.naive_utc())
            .format("%H:%M:%S")
            .to_string();
        let toks = format!("{}", r.tokens_in + r.tokens_out);
        lines.push(Line::from(vec![
            Span::styled("  ●  ", Style::default().fg(dot)),
            Span::styled(format!("{:<9} ", time), Style::default().fg(theme::DIM)),
            Span::styled(format!("{:<10}  ", r.agent), Style::default().fg(theme::FG)),
            Span::styled(format!("{:<24}  ", truncate(&short_model(&r.model), 24)), Style::default().fg(theme::FG)),
            Span::styled(format!("{:>5}  ", toks), Style::default().fg(theme::FG)),
            Span::styled(format!("${:>5.3}  ", r.cost), Style::default().fg(theme::FG)),
            Span::styled(format!("{:>5}  ", format_latency(r.latency_ms)), Style::default().fg(theme::DIM)),
            Span::styled(r.status.clone(), Style::default().fg(dot)),
        ]));
    }
    f.render_widget(Paragraph::new(lines), body_area);
}

fn insights_strip(f: &mut Frame, area: Rect, _app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::FAINT))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "INSIGHTS",
                Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  [0] for detail ", Style::default().fg(theme::DIM)),
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "  (no active insights)",
                Style::default().fg(theme::DIM),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "  full rule engine lands in",
                Style::default().fg(theme::DIM),
            )),
            Line::from(Span::styled(
                "  M4 — Insights tab",
                Style::default().fg(theme::DIM),
            )),
        ]),
        inner,
    );
}

fn budget_strip(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::FAINT))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "BUDGET",
                Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let day = app.invocations.total_cost_today();
    let session = day.min(SESSION_CAP);

    let bw = (inner.width as usize).saturating_sub(36);
    let mut lines = Vec::new();
    for (label, spent, cap) in [("session", session, SESSION_CAP), ("day", day, DAY_CAP)] {
        let pct = if cap > 0.0 { (spent / cap).clamp(0.0, 1.0) } else { 0.0 };
        let filled = (pct * bw as f64).round() as usize;
        let bar: String = std::iter::repeat('█').take(filled).collect::<String>()
            + &std::iter::repeat('░').take(bw - filled).collect::<String>();
        let color = if pct > 0.8 {
            theme::RED
        } else if pct > 0.5 {
            theme::YELLOW
        } else {
            theme::GREEN
        };
        lines.push(Line::from(vec![
            Span::styled(format!(" {:<8}", label), Style::default().fg(theme::DIM)),
            Span::styled(bar, Style::default().fg(color)),
            Span::styled(
                format!(" ${:>5.2} / ${:>5.2}  {:>3.0}%", spent, cap, pct * 100.0),
                Style::default().fg(theme::FG),
            ),
        ]));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

use chrono::TimeZone;

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
