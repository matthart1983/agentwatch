use chrono::TimeZone;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, JobCompletion, SubmittedPrompt, WORKFLOWS};
use crate::driver::LineSource;
use crate::theme;
use crate::ui::bot;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),                       // single-line status
            Constraint::Length(1),                       // horizontal rule
            Constraint::Min(0),                          // transcript
            Constraint::Length(1),                       // separator
            Constraint::Length(bot::height()),           // R2 bot + status text
            Constraint::Length(4),                       // PROMPT box
        ])
        .split(area);

    status_line(f, chunks[0], app);
    horizontal_rule(f, chunks[1]);
    transcript(f, chunks[2], app);
    horizontal_rule(f, chunks[3]);
    bot_pane(f, chunks[4], app);
    prompt_box(f, chunks[5], app);
}

fn bot_pane(f: &mut Frame, area: Rect, app: &App) {
    let working = app.waiting_for_runtime().is_some();
    bot::render(f, area, app.frame, working);

    // Status text to the right of the bot.
    let text_x = area.x + bot::width() + 2;
    if text_x >= area.x + area.width {
        return;
    }
    let text_area = Rect {
        x: text_x,
        y: area.y,
        width: area.width - (text_x - area.x),
        height: area.height,
    };

    let lines = if let Some(sp) = app.waiting_for_runtime() {
        let elapsed = (chrono::Utc::now() - sp.at).num_seconds().max(0);
        let lines_so_far = sp.response.len();
        vec![
            Line::raw(""),
            Line::from(vec![Span::styled(
                "neo is thinking…",
                Style::default()
                    .fg(theme::CYAN)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(vec![
                Span::styled(format!("{}s elapsed", elapsed), Style::default().fg(theme::FG)),
                Span::styled(
                    format!("  ·  {} lines streamed", lines_so_far),
                    Style::default().fg(theme::DIM),
                ),
            ]),
            Line::from(vec![Span::styled(
                format!("workflow={}", sp.workflow),
                Style::default().fg(theme::DIM),
            )]),
            Line::from(vec![Span::styled(
                "esc to cancel · ctrl+c to quit",
                Style::default().fg(theme::FAINT),
            )]),
        ]
    } else {
        match app.invocations.records.last() {
            Some(r) => vec![
                Line::raw(""),
                Line::from(vec![Span::styled(
                    "lens cooled",
                    Style::default().fg(theme::DIM).add_modifier(Modifier::BOLD),
                )]),
                Line::from(vec![
                    Span::styled("last  ", Style::default().fg(theme::DIM)),
                    Span::styled(
                        format!("{} · ${:.4}", r.agent, r.cost),
                        Style::default().fg(theme::FG),
                    ),
                ]),
                Line::from(vec![Span::styled(
                    format!("model {}", short_model(&r.model)),
                    Style::default().fg(theme::DIM),
                )]),
                Line::from(vec![Span::styled(
                    "type below · enter to send",
                    Style::default().fg(theme::FAINT),
                )]),
            ],
            None => vec![
                Line::raw(""),
                Line::from(vec![Span::styled(
                    "lens warmed · awaiting orders",
                    Style::default().fg(theme::DIM).add_modifier(Modifier::BOLD),
                )]),
                Line::from(vec![Span::styled(
                    "type below, hit enter, watch the lens scan",
                    Style::default().fg(theme::DIM),
                )]),
                Line::from(vec![Span::styled(
                    "ctrl+1..6 picks a workflow preset",
                    Style::default().fg(theme::FAINT),
                )]),
                Line::raw(""),
            ],
        }
    };
    f.render_widget(Paragraph::new(lines), text_area);
}

fn status_line(f: &mut Frame, area: Rect, app: &App) {
    let dim = Style::default().fg(theme::DIM);
    let cyan_bold = Style::default()
        .fg(theme::CYAN)
        .add_modifier(Modifier::BOLD);
    let fg = Style::default().fg(theme::FG);
    let faint = Style::default().fg(theme::FAINT);

    let session = app
        .threads
        .first()
        .map(|t| format!("T-...{}", &t.id[t.id.len().saturating_sub(6)..]))
        .unwrap_or_else(|| "(new)".to_string());
    let workspace_short = home_relative(&app.workspace.display().to_string());
    let spent = app.invocations.total_cost_today();

    let left = Line::from(vec![
        Span::styled(" ", Style::default()),
        Span::styled(session, cyan_bold),
        Span::styled("  ·  ", faint),
        Span::styled(app.workflow_name(), Style::default().fg(theme::CYAN)),
        Span::styled("  ·  ", faint),
        Span::styled(workspace_short, fg),
    ]);

    let last_invoc = app.invocations.records.last();
    let right = match last_invoc {
        Some(r) => Line::from(vec![
            Span::styled(format!("spent ${:.2}  ", spent), dim),
            Span::styled("·  ", faint),
            Span::styled(
                format!("{} ", r.agent),
                Style::default()
                    .fg(theme::GREEN)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(short_model(&r.model), fg),
            Span::raw(" "),
        ]),
        None => Line::from(vec![
            Span::styled(format!("spent ${:.2}  ", spent), dim),
            Span::styled("·  awaiting first prompt ", faint),
        ]),
    }
    .alignment(ratatui::layout::Alignment::Right);

    f.render_widget(Paragraph::new(left), area);
    f.render_widget(Paragraph::new(right), area);
}

fn horizontal_rule(f: &mut Frame, area: Rect) {
    let rule: String = std::iter::repeat('─').take(area.width as usize).collect();
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            rule,
            Style::default().fg(theme::FAINT),
        ))),
        area,
    );
}

fn transcript(f: &mut Frame, area: Rect, app: &App) {
    let width = area.width as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();

    if app.submitted.is_empty() && app.invocations.records.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "  ▸ Thread — distraction-free driver.",
            Style::default()
                .fg(theme::CYAN)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "    Type below, press Enter. Same prompt engine as Console.",
            Style::default().fg(theme::FG),
        )));
        lines.push(Line::from(Span::styled(
            "    Switch to [2] Console for the full instrument panel.",
            Style::default().fg(theme::DIM),
        )));
        f.render_widget(Paragraph::new(lines), area);
        return;
    }

    for sp in &app.submitted {
        push_user_block(&mut lines, sp, width);
        push_response_block(&mut lines, sp, width);
        lines.push(Line::raw(""));
    }

    // Take the most-recent slice that fits the visible area so we always
    // show the newest activity.
    let visible_h = area.height as usize;
    if lines.len() > visible_h {
        let drop = lines.len() - visible_h;
        lines.drain(..drop);
    }

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn push_user_block(lines: &mut Vec<Line<'static>>, sp: &SubmittedPrompt, _width: usize) {
    let time = chrono::Local
        .from_utc_datetime(&sp.at.naive_utc())
        .format("%H:%M:%S")
        .to_string();
    lines.push(Line::from(vec![
        Span::styled(" ▸ ", Style::default().fg(theme::CYAN)),
        Span::styled(
            "you",
            Style::default()
                .fg(theme::CYAN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("                            {}", time),
            Style::default().fg(theme::DIM),
        ),
    ]));
    for body_line in sp.text.lines() {
        lines.push(Line::from(vec![
            Span::raw("   "),
            Span::styled(body_line.to_string(), Style::default().fg(theme::FG)),
        ]));
    }
}

fn push_response_block(lines: &mut Vec<Line<'static>>, sp: &SubmittedPrompt, _width: usize) {
    if sp.response.is_empty() && sp.completed.is_none() {
        return;
    }
    lines.push(Line::raw(""));
    for line in &sp.response {
        let (color, prefix) = match line.source {
            LineSource::Stdout => (theme::FG, "   "),
            LineSource::Stderr => (theme::DIM, "   "),
        };
        let cleaned = strip_ansi(&line.text);
        lines.push(Line::from(vec![
            Span::raw(prefix),
            Span::styled(cleaned, Style::default().fg(color)),
        ]));
    }
    if let Some(comp) = &sp.completed {
        let (color, glyph, text) = match comp {
            JobCompletion::Success => (theme::GREEN, "✓", "neo completed".to_string()),
            JobCompletion::Failure { code: Some(c) } => {
                (theme::RED, "✗", format!("neo exited {}", c))
            }
            JobCompletion::Failure { code: None } => {
                (theme::RED, "✗", "neo terminated".to_string())
            }
            JobCompletion::SpawnError { reason } => {
                (theme::RED, "✗", format!("spawn error: {}", reason))
            }
        };
        lines.push(Line::from(vec![
            Span::styled(format!(" {} ", glyph), Style::default().fg(color)),
            Span::styled(
                text,
                Style::default()
                    .fg(color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    }
}

fn prompt_box(f: &mut Frame, area: Rect, app: &App) {
    let wf = &WORKFLOWS[app.workflow.min(WORKFLOWS.len() - 1)];
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::CYAN))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "PROMPT",
                Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  enter to send  ·  router → {} ", wf.blurb),
                Style::default().fg(theme::DIM),
            ),
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let indicator_w: u16 = 4;
    let indicator_area = Rect {
        x: inner.x,
        y: inner.y,
        width: indicator_w,
        height: inner.height,
    };
    let textarea_area = Rect {
        x: inner.x + indicator_w,
        y: inner.y,
        width: inner.width.saturating_sub(indicator_w),
        height: inner.height,
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " ▸ ",
            Style::default()
                .fg(theme::GREEN)
                .add_modifier(Modifier::BOLD),
        ))),
        indicator_area,
    );
    f.render_widget(&app.prompt, textarea_area);
}

// ──────────────────────────────────────────────────────────────────────
fn home_relative(p: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(rel) = std::path::Path::new(p).strip_prefix(&home) {
            return format!("~/{}", rel.display());
        }
    }
    p.to_string()
}

fn short_model(model: &str) -> String {
    model.split('/').next_back().unwrap_or(model).to_string()
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            while let Some(&p) = chars.peek() {
                chars.next();
                if p.is_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}
