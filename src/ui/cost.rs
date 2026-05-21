use chrono::{Duration, Timelike, Utc};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::App;
use crate::theme;

// Default budget caps — neo's config has the real ones but we don't read
// neo's config today. Tracked in PLAN.md as a follow-up.
const SESSION_CAP: f64 = 5.0;
const DAY_CAP: f64 = 20.0;
const WEEK_CAP: f64 = 100.0;
const MONTH_CAP: f64 = 400.0;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // period strip
            Constraint::Length(6),  // BUDGETS
            Constraint::Length(10), // BY MODEL / BY AGENT
            Constraint::Min(0),     // PROJECTION
        ])
        .split(area);

    period_strip(f, chunks[0], app);
    budgets(f, chunks[1], app);

    let row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(35),
            Constraint::Percentage(25),
        ])
        .split(chunks[2]);
    by_model(f, row[0], app);
    by_agent(f, row[1], app);
    by_provider(f, row[2], app);

    projection(f, chunks[3], app);
}

fn period_strip(f: &mut Frame, area: Rect, _app: &App) {
    let dim = Style::default().fg(theme::DIM);
    let on = Style::default()
        .bg(theme::SEL_BG)
        .fg(theme::FG)
        .add_modifier(Modifier::BOLD);
    let fg = Style::default().fg(theme::FG);

    let mut spans = vec![Span::styled(" period ", dim)];
    let opts = ["session", " today ", "week", "month"];
    for (i, label) in opts.iter().enumerate() {
        spans.push(Span::styled(label.to_string(), if i == 1 { on } else { fg }));
        spans.push(Span::raw("  "));
    }
    spans.push(Span::raw(" "));
    spans.push(Span::styled(
        "budget caps enforced  hard-stop on exceed",
        dim,
    ));
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn budgets(f: &mut Frame, area: Rect, app: &App) {
    let day_spent = app.invocations.total_cost_today();
    let session_spent = day_spent.min(SESSION_CAP); // best proxy without session boundaries
    let week_spent = window_cost(app, Duration::days(7));
    let month_spent = window_cost(app, Duration::days(30));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::FAINT))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "BUDGETS",
                Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let gauges = [
        ("session", session_spent, SESSION_CAP),
        ("day", day_spent, DAY_CAP),
        ("week", week_spent, WEEK_CAP),
        ("month", month_spent, MONTH_CAP),
    ];

    // 2-column, 2-row layout inside the box
    let half_w = inner.width / 2;
    for (i, (label, spent, cap)) in gauges.iter().enumerate() {
        let row = i / 2;
        let col = i % 2;
        let cell = Rect {
            x: inner.x + col as u16 * half_w,
            y: inner.y + row as u16,
            width: half_w,
            height: 1,
        };
        f.render_widget(gauge_line(label, *spent, *cap, (cell.width as usize).saturating_sub(28)), cell);
    }
}

fn gauge_line<'a>(label: &'a str, spent: f64, cap: f64, bw: usize) -> Paragraph<'a> {
    let pct = if cap <= 0.0 { 0.0 } else { (spent / cap).clamp(0.0, 1.0) };
    let filled = (pct * bw as f64).round() as usize;
    let empty = bw.saturating_sub(filled);
    let bar: String = std::iter::repeat('█').take(filled).collect::<String>()
        + &std::iter::repeat('░').take(empty).collect::<String>();
    let color = if pct > 0.8 {
        theme::RED
    } else if pct > 0.5 {
        theme::YELLOW
    } else {
        theme::GREEN
    };

    Paragraph::new(Line::from(vec![
        Span::styled(format!(" {:<8}", label), Style::default().fg(theme::DIM)),
        Span::styled(bar, Style::default().fg(color)),
        Span::styled(format!(" ${:>5.2} / ${:>5.2} ", spent, cap), Style::default().fg(theme::FG)),
    ]))
}

fn by_model(f: &mut Frame, area: Rect, app: &App) {
    let mut models = app.invocations.by_model_today();
    models.sort_by(|a, b| b.cost.partial_cmp(&a.cost).unwrap_or(std::cmp::Ordering::Equal));
    let total: f64 = models.iter().map(|m| m.cost).sum();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::FAINT))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "BY MODEL",
                Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  today  ${:.2} ", total), Style::default().fg(theme::DIM)),
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if models.is_empty() {
        f.render_widget(empty_message(), inner);
        return;
    }

    let bw = (inner.width as usize).saturating_sub(30);
    let max = models.iter().map(|m| m.cost).fold(0.0_f64, f64::max).max(0.01);

    let mut lines = Vec::new();
    let palette = bar_palette();
    for (i, m) in models.iter().take(6).enumerate() {
        let color = palette[i % palette.len()];
        let pct = if total > 0.0 { (m.cost / total) * 100.0 } else { 0.0 };
        lines.push(bar_line(
            &short_model(&m.model),
            m.cost,
            pct,
            color,
            bw,
            max,
        ));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn by_agent(f: &mut Frame, area: Rect, app: &App) {
    let mut agents = app.invocations.by_agent_today();
    agents.sort_by(|a, b| b.cost.partial_cmp(&a.cost).unwrap_or(std::cmp::Ordering::Equal));
    let total: f64 = agents.iter().map(|a| a.cost).sum();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::FAINT))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "BY AGENT",
                Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  today  ${:.2} ", total), Style::default().fg(theme::DIM)),
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if agents.is_empty() {
        f.render_widget(empty_message(), inner);
        return;
    }

    let bw = (inner.width as usize).saturating_sub(30);
    let max = agents.iter().map(|a| a.cost).fold(0.0_f64, f64::max).max(0.01);

    let mut lines = Vec::new();
    let palette = bar_palette();
    for (i, a) in agents.iter().take(6).enumerate() {
        let color = palette[i % palette.len()];
        let pct = if total > 0.0 { (a.cost / total) * 100.0 } else { 0.0 };
        lines.push(bar_line(&a.agent, a.cost, pct, color, bw, max));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn by_provider(f: &mut Frame, area: Rect, app: &App) {
    let mut providers = app.invocations.by_provider_today();
    providers.sort_by(|a, b| b.cost.partial_cmp(&a.cost).unwrap_or(std::cmp::Ordering::Equal));
    let total: f64 = providers.iter().map(|p| p.cost).sum();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::FAINT))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "BY PROVIDER",
                Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  today  ${:.2} ", total), Style::default().fg(theme::DIM)),
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if providers.is_empty() {
        f.render_widget(empty_message(), inner);
        return;
    }

    let bw = (inner.width as usize).saturating_sub(22);
    let max = providers.iter().map(|p| p.cost).fold(0.0_f64, f64::max).max(0.01);
    let mut lines = Vec::new();
    for p in providers.iter().take(6) {
        let prov = crate::data::Provider::from_str(&p.provider);
        let color = prov.color();
        if p.subscription_only {
            lines.push(Line::from(vec![
                Span::styled(format!(" [{}] ", prov.badge()), Style::default().fg(color)),
                Span::styled(format!("{:<10}", prov.name()), Style::default().fg(theme::FG)),
                Span::styled(
                    format!(" {} calls · sub.", p.calls),
                    Style::default().fg(theme::DIM),
                ),
            ]));
            continue;
        }
        let filled = ((p.cost / max) * bw as f64).round() as usize;
        let empty = bw.saturating_sub(filled);
        let bar: String = std::iter::repeat('█').take(filled).collect::<String>()
            + &std::iter::repeat('░').take(empty).collect::<String>();
        let pct = if total > 0.0 { (p.cost / total) * 100.0 } else { 0.0 };
        lines.push(Line::from(vec![
            Span::styled(format!(" [{}] ", prov.badge()), Style::default().fg(color)),
            Span::styled(format!("{:<10}", prov.name()), Style::default().fg(theme::FG)),
        ]));
        lines.push(Line::from(vec![
            Span::raw("      "),
            Span::styled(format!("${:>5.2} ", p.cost), Style::default().fg(theme::FG)),
            Span::styled(bar, Style::default().fg(color)),
            Span::styled(format!(" {:>3.0}%", pct), Style::default().fg(theme::DIM)),
        ]));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn bar_line<'a>(label: &str, cost: f64, pct: f64, color: Color, bw: usize, max: f64) -> Line<'a> {
    let filled = ((cost / max) * bw as f64).round() as usize;
    let empty = bw.saturating_sub(filled);
    let bar: String = std::iter::repeat('█').take(filled).collect::<String>()
        + &std::iter::repeat('░').take(empty).collect::<String>();
    Line::from(vec![
        Span::styled(format!(" {:<14}", truncate(label, 14)), Style::default().fg(theme::FG)),
        Span::styled(format!(" ${:>5.2} ", cost), Style::default().fg(theme::FG)),
        Span::styled(bar, Style::default().fg(color)),
        Span::styled(format!(" {:>4.0}%", pct), Style::default().fg(theme::DIM)),
    ])
}

fn projection(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::FAINT))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "PROJECTION",
                Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  today  current rate ", Style::default().fg(theme::DIM)),
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let spent_today = app.invocations.total_cost_today();
    let last_hour = window_cost(app, Duration::hours(1));
    let rate_per_hour = last_hour; // already 1h window
    let now = chrono::Local::now();
    let hours_left = (24 - now.hour()) as f64;
    let projected = spent_today + rate_per_hour * hours_left;
    let pct_of_cap = if DAY_CAP > 0.0 { projected / DAY_CAP * 100.0 } else { 0.0 };

    let dim = Style::default().fg(theme::DIM);
    let fg = Style::default().fg(theme::FG);
    let warn = if pct_of_cap > 80.0 {
        Style::default().fg(theme::RED)
    } else if pct_of_cap > 50.0 {
        Style::default().fg(theme::YELLOW)
    } else {
        fg
    };

    let lines = vec![
        Line::from(vec![
            Span::styled("  spent so far ", dim),
            Span::styled(format!("${:.2}", spent_today), fg),
            Span::styled(
                format!("  at {}", now.format("%H:%M")),
                dim,
            ),
        ]),
        Line::from(vec![
            Span::styled("  current rate ", dim),
            Span::styled(format!("${:.2}/hr", rate_per_hour), fg),
            Span::styled("  (last 60 min)", dim),
        ]),
        Line::from(vec![
            Span::styled("  projection   ", dim),
            Span::styled(
                format!("${:.2} by midnight", projected),
                fg,
            ),
            Span::styled(
                format!("  ({:.0}% of ${:.0} cap)", pct_of_cap, DAY_CAP),
                warn,
            ),
        ]),
        Line::raw(""),
        Line::from(vec![Span::styled(
            "  full projection curve lands with neo PR #2 (StateEmitter) — today's view is a flat extrapolation",
            dim,
        )]),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

fn window_cost(app: &App, window: Duration) -> f64 {
    let since = Utc::now() - window;
    app.invocations.in_window(since).map(|r| r.cost).sum()
}

fn empty_message<'a>() -> Paragraph<'a> {
    Paragraph::new(Line::from(vec![Span::styled(
        "  (no spend yet)",
        Style::default().fg(theme::DIM),
    )]))
}

fn bar_palette() -> [Color; 5] {
    [
        theme::GREEN,
        theme::CYAN,
        theme::YELLOW,
        theme::MAGENTA,
        theme::DIM,
    ]
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
