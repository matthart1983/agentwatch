use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::App;
use crate::data::ModelAgg;
use crate::theme;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // sort strip
            Constraint::Length(14), // leaderboard
            Constraint::Min(0),     // bottom split
        ])
        .split(area);

    sort_strip(f, chunks[0], app);
    leaderboard(f, chunks[1], app);

    let bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(80), Constraint::Min(0)])
        .split(chunks[2]);

    routing_decision(f, bottom[0], app);
    spend_breakdown(f, bottom[1], app);
}

fn sort_strip(f: &mut Frame, area: Rect, app: &App) {
    let total = app.invocations.records.len();
    let today = app.invocations.calls_today();

    let dim = Style::default().fg(theme::DIM);
    let on = Style::default()
        .bg(theme::SEL_BG)
        .fg(theme::FG)
        .add_modifier(Modifier::BOLD);
    let fg = Style::default().fg(theme::FG);

    let mut spans = vec![Span::styled(" sort ", dim)];
    let opts = [" spend ↓ ", "calls", "latency", "success"];
    for (i, label) in opts.iter().enumerate() {
        let style = if i == 0 { on } else { fg };
        spans.push(Span::styled(label.to_string(), style));
        spans.push(Span::raw("  "));
    }
    spans.push(Span::raw(" "));
    spans.push(Span::styled(
        format!("{} models active  {} calls today  ({} all-time)", app.invocations.by_model_today().len(), today, total),
        dim,
    ));
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn leaderboard(f: &mut Frame, area: Rect, app: &App) {
    let mut models = app.invocations.by_model_today();
    models.sort_by(|a, b| b.cost.partial_cmp(&a.cost).unwrap_or(std::cmp::Ordering::Equal));

    let title_count = models.len();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::FAINT))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "MODELS",
                Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {} in use today ", title_count),
                Style::default().fg(theme::DIM),
            ),
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 2 {
        return;
    }

    let header = Paragraph::new(Line::from(vec![Span::styled(
        "  MODEL                            PROV         CALLS  $TODAY    p50      p99      SUCC",
        Style::default().fg(theme::DIM),
    )]));
    f.render_widget(
        header,
        Rect { height: 1, ..inner },
    );

    let rows_area = Rect {
        y: inner.y + 1,
        height: inner.height - 1,
        ..inner
    };

    if models.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                "  (no invocations recorded yet — make a call in neo and reload with F5)",
                Style::default().fg(theme::DIM),
            )])),
            rows_area,
        );
        return;
    }

    let visible = rows_area.height as usize;
    let start = if app.models_selected >= visible {
        app.models_selected + 1 - visible
    } else {
        0
    };

    let mut lines = Vec::with_capacity(visible);
    for (i, m) in models.iter().enumerate().skip(start).take(visible) {
        lines.push(model_line(m, i == app.models_selected));
    }
    f.render_widget(Paragraph::new(lines), rows_area);
}

fn model_line<'a>(m: &'a ModelAgg, selected: bool) -> Line<'a> {
    let (dot_color, _) = health(m);
    let base = if selected {
        Style::default().bg(theme::SEL_BG).fg(theme::FG)
    } else {
        Style::default().fg(theme::FG)
    };
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
    let succ_style = if selected {
        Style::default().bg(theme::SEL_BG).fg(theme::GREEN)
    } else {
        Style::default().fg(theme::GREEN)
    };

    let model_label = truncate(&m.model, 32);
    let prov_label = truncate(&m.provider, 12);

    Line::from(vec![
        Span::styled("  ●  ", dot_style),
        Span::styled(format!("{:<32}  ", model_label), base),
        Span::styled(format!("{:<12} ", prov_label), Style::default().fg(theme::CYAN).bg(if selected { theme::SEL_BG } else { theme::BG })),
        Span::styled(format!("{:>5}  ", m.calls), base),
        Span::styled(format!("${:>6.2}  ", m.cost), base),
        Span::styled(format!("{:>6}  ", format_latency(m.p50_latency_ms)), dim),
        Span::styled(format!("{:>6}  ", format_latency(m.p99_latency_ms)), dim),
        Span::styled(format!("{:>4}%", (m.success_rate * 100.0) as u32), succ_style),
    ])
}

fn routing_decision(f: &mut Frame, area: Rect, _app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::FAINT))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "ROUTING DECISION",
                Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  most recent ", Style::default().fg(theme::DIM)),
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let dim = Style::default().fg(theme::DIM);
    let fg = Style::default().fg(theme::FG);

    let lines = vec![
        Line::from(vec![Span::styled("  (no routing decisions available yet)", dim)]),
        Line::raw(""),
        Line::from(vec![Span::styled(
            "  router decisions land with neo PR #2 (StateEmitter)",
            fg,
        )]),
        Line::raw(""),
        Line::from(vec![Span::styled(
            "  meanwhile the leaderboard above reflects which model the",
            dim,
        )]),
        Line::from(vec![Span::styled(
            "  router has been picking, derived from invocations.jsonl",
            dim,
        )]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  weights  ", dim),
            Span::styled("cap 40%  cost 25%  lat 20%  ctx 15%", fg),
        ]),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

fn spend_breakdown(f: &mut Frame, area: Rect, app: &App) {
    let mut models = app.invocations.by_model_today();
    models.sort_by(|a, b| b.cost.partial_cmp(&a.cost).unwrap_or(std::cmp::Ordering::Equal));
    let total: f64 = models.iter().map(|m| m.cost).sum();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::FAINT))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "SPEND",
                Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  by model  ${:.2} total ", total),
                Style::default().fg(theme::DIM),
            ),
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if models.is_empty() || total <= 0.0 {
        f.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                "  (no spend yet)",
                Style::default().fg(theme::DIM),
            )])),
            inner,
        );
        return;
    }

    let bw = (inner.width as usize).saturating_sub(20);
    let max = models.iter().map(|m| m.cost).fold(0.0_f64, f64::max).max(0.01);

    let mut lines = Vec::with_capacity(models.len() * 2);
    let palette = [
        theme::GREEN,
        theme::CYAN,
        theme::YELLOW,
        theme::MAGENTA,
        theme::DIM,
    ];
    for (i, m) in models.iter().take(6).enumerate() {
        let color = palette[i % palette.len()];
        let label = short_model(&m.model);
        let filled = ((m.cost / max) * bw as f64).round() as usize;
        let empty = bw.saturating_sub(filled);
        let bar: String = std::iter::repeat('█').take(filled).collect::<String>()
            + &std::iter::repeat('░').take(empty).collect::<String>();
        lines.push(Line::from(vec![
            Span::styled(format!("  {:<16}", truncate(&label, 16)), Style::default().fg(theme::FG)),
            Span::styled(format!(" ${:>5.2}  ", m.cost), Style::default().fg(theme::FG)),
            Span::styled(bar, Style::default().fg(color)),
        ]));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn health(m: &ModelAgg) -> (Color, &'static str) {
    if m.success_rate < 0.85 {
        (theme::YELLOW, "degraded")
    } else if m.calls == 0 {
        (theme::DIM, "—")
    } else {
        (theme::GREEN, "ok")
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let kept: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", kept)
    }
}

fn short_model(model: &str) -> String {
    model.split('/').next_back().unwrap_or(model).to_string()
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
