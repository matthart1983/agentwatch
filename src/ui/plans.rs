use chrono::{TimeZone, Utc};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::app::App;
use crate::data::invocations::{PastPipeline, PipelineStep};
use crate::theme;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let pipelines = app.invocations.recent_pipelines(60, 6);
    let current = pipelines.first();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // header strip
            Constraint::Min(8),     // DAG / timeline
            Constraint::Length(10), // detail box
        ])
        .split(area);

    header_strip(f, chunks[0], app, current);

    let mid = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(28)])
        .split(chunks[1]);

    timeline(f, mid[0], current);
    task_list(f, mid[1], current);

    detail(f, chunks[2], current.and_then(|p| p.steps.last()));
}

fn header_strip(f: &mut Frame, area: Rect, _app: &App, current: Option<&PastPipeline>) {
    let dim = Style::default().fg(theme::DIM);
    let fg = Style::default().fg(theme::FG);

    let line = match current {
        Some(p) => {
            let age = format_age(p.finished_at);
            Line::from(vec![
                Span::styled(" pipeline ", dim),
                Span::styled(
                    format!("#{}", p.steps.len()),
                    Style::default()
                        .fg(theme::CYAN)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("  {} steps", p.steps.len()), dim),
                Span::styled(format!("   spend ${:.4}", p.total_cost), fg),
                Span::styled(
                    format!(
                        "   tokens {} in / {} out",
                        p.total_tokens_in, p.total_tokens_out
                    ),
                    dim,
                ),
                Span::styled(format!("   {} ago ", age), dim),
            ])
        }
        None => Line::from(vec![
            Span::styled(" pipeline ", dim),
            Span::styled(
                "(no recent activity)",
                Style::default().fg(theme::DIM).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "   timeline reconstructs from invocations.jsonl",
                dim,
            ),
        ]),
    };
    f.render_widget(Paragraph::new(line), area);
}

fn timeline(f: &mut Frame, area: Rect, pipeline: Option<&PastPipeline>) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::FAINT))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "PIPELINE",
                Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  reconstructed timeline ", Style::default().fg(theme::DIM)),
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(p) = pipeline else {
        f.render_widget(empty_pipeline_view(), inner);
        return;
    };

    let mut lines = Vec::new();
    for (i, step) in p.steps.iter().enumerate() {
        let (color, glyph) = status_visual(&step.status);
        let connector = if i == 0 { "  " } else { "│ " };
        let prefix_line = Line::from(vec![Span::styled(
            format!("  {}                                       ", connector),
            Style::default().fg(theme::FAINT),
        )]);
        if i > 0 {
            lines.push(prefix_line);
        }
        let time = chrono::Local
            .from_utc_datetime(&step.at.naive_utc())
            .format("%H:%M:%S")
            .to_string();
        lines.push(Line::from(vec![
            Span::styled(format!("  {}  ", glyph), Style::default().fg(color)),
            Span::styled(
                format!("{:02}  ", step.idx),
                Style::default().fg(theme::DIM),
            ),
            Span::styled(
                format!("{:<10}", step.agent),
                Style::default()
                    .fg(color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}", short_model(&step.model)),
                Style::default().fg(theme::DIM),
            ),
            Span::styled(format!("  {}", time), Style::default().fg(theme::DIM)),
        ]));
        lines.push(Line::from(vec![
            Span::raw("     "),
            Span::styled(
                format!(
                    "{}↑ {}↓  ${:.4}  {}  {}  tools={}",
                    step.tokens_in,
                    step.tokens_out,
                    step.cost,
                    format_latency(step.latency_ms),
                    step.status,
                    step.tool_calls,
                ),
                Style::default().fg(theme::FG),
            ),
        ]));
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn task_list(f: &mut Frame, area: Rect, pipeline: Option<&PastPipeline>) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::FAINT))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "TASKS",
                Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(p) = pipeline else {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "  —",
                Style::default().fg(theme::DIM),
            ))),
            inner,
        );
        return;
    };

    let mut lines = Vec::new();
    for step in &p.steps {
        let (color, _) = status_visual(&step.status);
        lines.push(Line::from(vec![
            Span::styled(format!(" {:02} ", step.idx), Style::default().fg(theme::DIM)),
            Span::styled(
                format!("{:<10}", step.agent),
                Style::default().fg(theme::FG),
            ),
            Span::styled(
                step.status.clone(),
                Style::default().fg(color),
            ),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        " review cycles",
        Style::default().fg(theme::DIM),
    )));
    lines.push(Line::from(Span::styled(
        " 0 / 3 (no DAG yet)",
        Style::default().fg(theme::DIM),
    )));
    f.render_widget(Paragraph::new(lines), inner);
}

fn detail(f: &mut Frame, area: Rect, step: Option<&PipelineStep>) {
    let title = match step {
        Some(s) => format!(" {:02}  {}  detail ", s.idx, s.agent),
        None => " detail ".to_string(),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::FAINT))
        .title(Line::from(Span::styled(
            title,
            Style::default()
                .fg(theme::YELLOW)
                .add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(s) = step else {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "  (no step selected)",
                Style::default().fg(theme::DIM),
            ))),
            inner,
        );
        return;
    };

    let dim = Style::default().fg(theme::DIM);
    let fg = Style::default().fg(theme::FG);

    let lines = vec![
        kv("model", short_model(&s.model), dim, fg),
        kv("status", s.status.clone(), dim, fg),
        kv(
            "tokens",
            format!("{} in  /  {} out", s.tokens_in, s.tokens_out),
            dim,
            fg,
        ),
        kv("cost", format!("${:.6}", s.cost), dim, fg),
        kv("latency", format_latency(s.latency_ms), dim, fg),
        kv("tool calls", format!("{}", s.tool_calls), dim, fg),
        Line::raw(""),
        Line::from(vec![Span::styled(
            "  full dependency DAG + per-tool log land when neo PR #4 (PipelineEvents)",
            Style::default().fg(theme::DIM),
        )]),
        Line::from(vec![Span::styled(
            "  is wired into the orchestrator main loop.",
            Style::default().fg(theme::DIM),
        )]),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

fn kv<'a>(k: &'a str, v: String, k_style: Style, v_style: Style) -> Line<'a> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(format!("{:<12}", k), k_style),
        Span::styled(v, v_style),
    ])
}

fn empty_pipeline_view<'a>() -> Paragraph<'a> {
    Paragraph::new(vec![
        Line::raw(""),
        Line::from(Span::styled(
            "  no recent invocations to reconstruct a pipeline from",
            Style::default().fg(theme::DIM),
        )),
        Line::raw(""),
        Line::from(Span::styled(
            "  submit a prompt on [1] Thread or [2] Console — the timeline",
            Style::default().fg(theme::DIM),
        )),
        Line::from(Span::styled(
            "  will populate as neo's agents run.",
            Style::default().fg(theme::DIM),
        )),
        Line::raw(""),
        Line::from(Span::styled(
            "  live DAG with parent/child arrows lands when neo PR #4",
            Style::default().fg(theme::DIM),
        )),
        Line::from(Span::styled(
            "  (PipelineEvents) is wired into the orchestrator main loop.",
            Style::default().fg(theme::DIM),
        )),
    ])
}

fn status_visual(status: &str) -> (Color, &'static str) {
    match status {
        "success" => (theme::GREEN, "✓"),
        "max_iterations" => (theme::YELLOW, "◐"),
        "error" => (theme::RED, "✗"),
        _ => (theme::DIM, "·"),
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

fn format_age(t: chrono::DateTime<Utc>) -> String {
    let d = Utc::now() - t;
    if d.num_seconds() < 60 {
        format!("{}s", d.num_seconds().max(0))
    } else if d.num_minutes() < 60 {
        format!("{}m", d.num_minutes())
    } else if d.num_hours() < 48 {
        format!("{}h", d.num_hours())
    } else {
        format!("{}d", d.num_days())
    }
}
