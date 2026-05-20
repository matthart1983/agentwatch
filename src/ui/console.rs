use chrono::{TimeZone, Utc};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, JobCompletion, SubmittedPrompt, AGENT_ORDER, WORKFLOWS};
use crate::data::ThreadSummary;
use crate::driver::LineSource;
use crate::theme;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // status strip
            Constraint::Min(0),    // three columns + prompt
        ])
        .split(area);

    status_strip(f, chunks[0], app);

    let body = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // 3-col content
            Constraint::Length(7), // PROMPT box
        ])
        .split(chunks[1]);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(34), // HISTORY rail
            Constraint::Min(0),     // WORKING centre
            Constraint::Length(42), // PIPELINE rail
        ])
        .split(body[0]);

    history_rail(f, cols[0], app);
    working_pane(f, cols[1], app);
    pipeline_rail(f, cols[2], app);

    prompt_box(f, body[1], app);
}

// ──────────────────────────────────────────────────────────────────────
// Status strip
// ──────────────────────────────────────────────────────────────────────
fn status_strip(f: &mut Frame, area: Rect, app: &App) {
    let dim = Style::default().fg(theme::DIM);
    let fg = Style::default().fg(theme::FG);
    let faint = Style::default().fg(theme::FAINT);
    let cyan_bold = Style::default()
        .fg(theme::CYAN)
        .add_modifier(Modifier::BOLD);
    let green = Style::default().fg(theme::GREEN);

    let session = app
        .threads
        .first()
        .map(|t| format!("T-...{}", &t.id[t.id.len().saturating_sub(6)..]))
        .unwrap_or_else(|| "(new)".to_string());

    let ws = app.workspace.display().to_string();
    let ws_short = home_relative(&ws);

    let agents_active = app.invocations.by_agent_today().iter().filter(|a| a.calls > 0).count();
    let total_agents = AGENT_ORDER.len();
    let spent_today = app.invocations.total_cost_today();

    let workflow = app.workflow_name();

    let left = Line::from(vec![
        Span::styled(" workflow ", dim),
        Span::styled(workflow, cyan_bold),
        Span::styled("   │ ", faint),
        Span::styled(" session ", dim),
        Span::styled(session, fg),
        Span::styled("   │ ", faint),
        Span::styled(" workspace ", dim),
        Span::styled(ws_short, fg),
    ]);

    let right = Line::from(vec![
        Span::styled(format!("agents {}/{} ", agents_active, total_agents), green),
        Span::styled("· ", faint),
        Span::styled(format!("spent ${:.2}", spent_today), dim),
        Span::raw(" "),
    ])
    .alignment(ratatui::layout::Alignment::Right);

    f.render_widget(Paragraph::new(left), area);
    f.render_widget(Paragraph::new(right), area);
}

// ──────────────────────────────────────────────────────────────────────
// HISTORY rail (left)
// ──────────────────────────────────────────────────────────────────────
fn history_rail(f: &mut Frame, area: Rect, app: &App) {
    let count = app.threads.len();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::FAINT))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "HISTORY",
                Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  {} sessions ", count), Style::default().fg(theme::DIM)),
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 3 {
        return;
    }

    // Current pinned row (uses workspace as a stand-in for "what's working")
    let pinned_area = Rect { height: 2.min(inner.height), ..inner };
    pinned_current(f, pinned_area, app);

    if inner.height < 4 {
        return;
    }

    // Divider
    let rule_area = Rect {
        y: inner.y + 2,
        height: 1,
        ..inner
    };
    let rule: String = std::iter::repeat('─').take(inner.width as usize).collect();
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(rule, Style::default().fg(theme::FAINT)))),
        rule_area,
    );

    // Past sessions
    let list_area = Rect {
        y: inner.y + 3,
        height: inner.height.saturating_sub(3),
        ..inner
    };

    if app.threads.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "  (no past sessions)",
                Style::default().fg(theme::DIM),
            ))),
            list_area,
        );
        return;
    }

    let rows_per_session = 2;
    let max = (list_area.height as usize) / rows_per_session;
    let mut lines = Vec::with_capacity(max * 2);
    for t in app.threads.iter().take(max) {
        let (line1, line2) = history_row(t);
        lines.push(line1);
        lines.push(line2);
    }
    f.render_widget(Paragraph::new(lines), list_area);
}

fn pinned_current(f: &mut Frame, area: Rect, app: &App) {
    let bg = Style::default().bg(theme::SEL_BG);
    let label = match app.threads.first() {
        Some(t) => format!("T-...{}", &t.id[t.id.len().saturating_sub(6)..]),
        None => "(new session)".to_string(),
    };
    let task = app
        .threads
        .first()
        .and_then(|t| t.first_user_message.clone())
        .unwrap_or_else(|| "type a prompt below to begin →".to_string());

    let line1 = Line::from(vec![
        Span::styled(" ▸ ", Style::default().fg(theme::CYAN).bg(theme::SEL_BG)),
        Span::styled(
            label,
            Style::default()
                .fg(theme::CYAN)
                .bg(theme::SEL_BG)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  now", Style::default().fg(theme::DIM).bg(theme::SEL_BG)),
    ]);
    let line2 = Line::from(vec![
        Span::styled(
            format!("   {}", truncate(&task, area.width.saturating_sub(4) as usize)),
            Style::default().fg(theme::FG).bg(theme::SEL_BG),
        ),
    ]);
    f.render_widget(Paragraph::new(vec![line1, line2]).style(bg), area);
}

fn history_row(t: &ThreadSummary) -> (Line<'static>, Line<'static>) {
    let id_short = format!("T-...{}", &t.id[t.id.len().saturating_sub(6)..]);
    let age = relative_age(t.updated_at);
    let cost = format!("${:.2}", t.cost_total);
    let (dot_color, status) = if t.cost_total > 0.0 {
        (theme::GREEN, "done")
    } else {
        (theme::DIM, "—")
    };
    let task = t
        .first_user_message
        .as_deref()
        .map(|s| truncate(s, 24))
        .unwrap_or_else(|| "(no message)".to_string());

    let line1 = Line::from(vec![
        Span::styled(" ● ", Style::default().fg(dot_color)),
        Span::styled(
            format!("{:<14}", id_short),
            Style::default().fg(theme::FG),
        ),
        Span::styled(format!("{:>4}  ", age), Style::default().fg(theme::DIM)),
        Span::styled(format!("{:>5}", cost), Style::default().fg(theme::DIM)),
    ]);
    let line2 = Line::from(vec![
        Span::styled(
            format!("   {:<20}", task),
            Style::default().fg(theme::FG),
        ),
        Span::styled(format!(" {}", status), Style::default().fg(dot_color)),
    ]);
    (line1, line2)
}

// ──────────────────────────────────────────────────────────────────────
// WORKING centre pane
// ──────────────────────────────────────────────────────────────────────
fn working_pane(f: &mut Frame, area: Rect, app: &App) {
    let title_session = app
        .threads
        .first()
        .map(|t| format!("T-...{}", &t.id[t.id.len().saturating_sub(6)..]))
        .unwrap_or_else(|| "(new)".to_string());

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::FAINT))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "WORKING",
                Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  {} ", title_session), Style::default().fg(theme::DIM)),
        ]))
        .title(
            Line::from(vec![Span::styled(
                if app.runtime_online { " live " } else { " offline " },
                Style::default().fg(if app.runtime_online {
                    theme::GREEN
                } else {
                    theme::DIM
                }),
            )])
            .alignment(ratatui::layout::Alignment::Right),
        );
    let inner = block.inner(area);
    f.render_widget(block, area);

    let body_h = inner.height.saturating_sub(2);
    let body_area = Rect { height: body_h, ..inner };
    let status_area = Rect {
        y: inner.y + body_h,
        height: 2.min(inner.height),
        ..inner
    };

    // Transcript: until we have live streaming, surface the most recent
    // invocations as a transcript-like feed. This is real data and feels
    // alive, even when no session is active.
    let lines = transcript_lines(app, body_area.width as usize);
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        body_area,
    );

    // Bottom status strip inside the box
    let rule: String = std::iter::repeat('─').take(inner.width as usize).collect();
    let rule_line = Line::from(Span::styled(rule, Style::default().fg(theme::FAINT)));

    let status_line = working_status_line(app);

    f.render_widget(Paragraph::new(vec![rule_line, status_line]), status_area);
}

fn transcript_lines(app: &App, width: usize) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    // Each submitted prompt + its streaming response is one transcript
    // block. Reads top-down: you said X → neo printed Y → next prompt.
    for sp in &app.submitted {
        push_user_block(&mut lines, sp, width);
        push_response_block(&mut lines, sp, width);
        lines.push(Line::raw(""));
    }

    // Empty state — no submits, no invocations.
    if app.submitted.is_empty() && app.invocations.records.is_empty() {
        return welcome_lines(app);
    }

    // Fallback: if there are invocations on disk but no submits yet (e.g.
    // they ran `neo` directly outside AgentWatch), surface the last few so
    // the pane isn't empty.
    if app.submitted.is_empty() {
        let recent: Vec<_> = app.invocations.records.iter().rev().take(10).collect();
        for r in recent.iter().rev() {
            let time = chrono::Local
                .from_utc_datetime(&r.ts.naive_utc())
                .format("%H:%M:%S")
                .to_string();
            lines.push(Line::from(vec![
                Span::styled(" ● ", Style::default().fg(theme::GREEN)),
                Span::styled(
                    format!("{:<10}", r.agent),
                    Style::default().fg(theme::GREEN).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" {}", time), Style::default().fg(theme::DIM)),
                Span::styled(
                    format!("   {}  ${:.4}", short_model(&r.model), r.cost),
                    Style::default().fg(theme::DIM),
                ),
            ]));
        }
    }

    lines
}

fn welcome_lines(_app: &App) -> Vec<Line<'static>> {
    vec![
        Line::raw(""),
        Line::from(Span::styled(
            "  ▸ Console — full instrument panel.",
            Style::default()
                .fg(theme::CYAN)
                .add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        Line::from(Span::styled(
            "    Type a prompt below and press Enter to send.",
            Style::default().fg(theme::FG),
        )),
        Line::from(Span::styled(
            "    Pick a workflow on the right rail (Ctrl+1..6).",
            Style::default().fg(theme::FG),
        )),
        Line::raw(""),
        Line::from(Span::styled(
            "    AgentWatch will spawn `neo` for each prompt and stream",
            Style::default().fg(theme::DIM),
        )),
        Line::from(Span::styled(
            "    its output back into this pane.",
            Style::default().fg(theme::DIM),
        )),
        Line::raw(""),
        Line::from(Span::styled(
            "    Set AGENTWATCH_NEO_BIN if neo isn't on PATH or in the",
            Style::default().fg(theme::DIM),
        )),
        Line::from(Span::styled(
            "    default ~/projects/active/neo/target/release/ location.",
            Style::default().fg(theme::DIM),
        )),
    ]
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
            Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("        {}", time), Style::default().fg(theme::DIM)),
        Span::styled(
            format!("   workflow={}", sp.workflow),
            Style::default().fg(theme::DIM),
        ),
    ]));
    // No pre-truncation — the WORKING Paragraph wraps with Wrap { trim: false }.
    for body_line in sp.text.lines() {
        lines.push(Line::from(vec![
            Span::raw("     "),
            Span::styled(body_line.to_string(), Style::default().fg(theme::FG)),
        ]));
    }
}

fn push_response_block(lines: &mut Vec<Line<'static>>, sp: &SubmittedPrompt, _width: usize) {
    if sp.response.is_empty() && sp.completed.is_none() {
        // No output yet — caller renders the spinner separately.
        return;
    }
    lines.push(Line::raw(""));
    if let Some(cmd) = &sp.command {
        lines.push(Line::from(vec![
            Span::raw("   "),
            Span::styled("$ ", Style::default().fg(theme::DIM)),
            Span::styled(cmd.clone(), Style::default().fg(theme::DIM)),
        ]));
    }
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
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
        ]));
    }
}

/// Tiny ANSI-CSI stripper. neo uses `colored!` heavily for its progress
/// output; if we pipe that to a terminal it'd print escape sequences. This
/// strips them so the WORKING pane stays clean.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip until we hit a final byte (a letter typically).
            if chars.peek() == Some(&'[') {
                chars.next();
                while let Some(&p) = chars.peek() {
                    chars.next();
                    if p.is_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn working_status_line(app: &App) -> Line<'static> {
    // Waiting takes priority — animated spinner so the user sees the
    // app is alive even when no real agent activity is happening.
    if let Some(sp) = app.waiting_for_runtime() {
        let spinner = app.spinner_frame();
        let elapsed = (chrono::Utc::now() - sp.at).num_seconds().max(0);
        let lines_so_far = sp.response.len();
        return Line::from(vec![
            Span::styled(
                format!(" {} ", spinner),
                Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "neo working",
                Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" · {}s · {} lines so far", elapsed, lines_so_far),
                Style::default().fg(theme::DIM),
            ),
            Span::styled(
                format!(" · workflow={}", sp.workflow),
                Style::default().fg(theme::DIM),
            ),
        ]);
    }

    let latest = app.invocations.records.last();
    match latest {
        Some(r) => Line::from(vec![
            Span::styled(" ▸ ", Style::default().fg(theme::GREEN)),
            Span::styled(
                format!("{} ", r.agent),
                Style::default()
                    .fg(theme::GREEN)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("· last {} ", short_model(&r.model)),
                Style::default().fg(theme::FG),
            ),
            Span::styled(
                format!("· {} ", format_latency(r.latency_ms)),
                Style::default().fg(theme::DIM),
            ),
            Span::styled(
                format!("· ${:.4}", r.cost),
                Style::default().fg(theme::DIM),
            ),
        ]),
        None => Line::from(vec![Span::styled(
            " ▸ ready · awaiting first prompt",
            Style::default().fg(theme::DIM),
        )]),
    }
}

// ──────────────────────────────────────────────────────────────────────
// PIPELINE rail (right)
// ──────────────────────────────────────────────────────────────────────
fn pipeline_rail(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::FAINT))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "PIPELINE",
                Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  cycle 0/3 ", Style::default().fg(theme::DIM)),
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(11), // TASKS placeholder
            Constraint::Length(6),  // ACTIVE AGENTS
            Constraint::Min(0),     // WORKFLOWS picker
        ])
        .split(inner);

    pipeline_tasks(f, sections[0]);
    active_agents(f, sections[1], app);
    workflow_picker(f, sections[2], app);
}

fn pipeline_tasks(f: &mut Frame, area: Rect) {
    let dim = Style::default().fg(theme::DIM);
    let lines = vec![
        Line::from(Span::styled(" TASKS", Style::default().fg(theme::DIM).add_modifier(Modifier::BOLD))),
        Line::from(vec![Span::styled(" ○  pending", dim)]),
        Line::from(vec![Span::styled(" ▸  running", Style::default().fg(theme::YELLOW))]),
        Line::from(vec![Span::styled(" ✓  done", Style::default().fg(theme::GREEN))]),
        Line::raw(""),
        Line::from(Span::styled(" (no active pipeline)", dim)),
        Line::raw(""),
        Line::from(Span::styled(
            " live DAG renders when neo PR #4",
            dim,
        )),
        Line::from(Span::styled(
            " is wired into orchestrator",
            dim,
        )),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

fn active_agents(f: &mut Frame, area: Rect, app: &App) {
    let rule: String = std::iter::repeat('─').take(area.width as usize).collect();
    let mut lines = vec![
        Line::from(Span::styled(rule, Style::default().fg(theme::FAINT))),
        Line::from(Span::styled(
            " ACTIVE AGENTS",
            Style::default().fg(theme::DIM).add_modifier(Modifier::BOLD),
        )),
    ];

    let mut agents = app.invocations.by_agent_today();
    agents.sort_by(|a, b| b.cost.partial_cmp(&a.cost).unwrap_or(std::cmp::Ordering::Equal));
    let top: Vec<_> = agents.into_iter().filter(|a| a.calls > 0).take(3).collect();
    if top.is_empty() {
        lines.push(Line::from(Span::styled(
            " (none active today)",
            Style::default().fg(theme::DIM),
        )));
    } else {
        for a in top {
            lines.push(Line::from(vec![
                Span::styled(" ● ", Style::default().fg(theme::GREEN)),
                Span::styled(
                    format!("{:<10}", a.agent),
                    Style::default().fg(theme::FG),
                ),
                Span::styled(
                    format!(" ${:>5.2}", a.cost),
                    Style::default().fg(theme::DIM),
                ),
            ]));
        }
    }
    f.render_widget(Paragraph::new(lines), area);
}

fn workflow_picker(f: &mut Frame, area: Rect, app: &App) {
    let rule: String = std::iter::repeat('─').take(area.width as usize).collect();
    let mut lines = vec![
        Line::from(Span::styled(rule, Style::default().fg(theme::FAINT))),
        Line::from(Span::styled(
            " WORKFLOWS  (ctrl+1..6) ",
            Style::default().fg(theme::DIM).add_modifier(Modifier::BOLD),
        )),
    ];

    for (i, w) in WORKFLOWS.iter().enumerate() {
        let selected = i == app.workflow;
        let bg = if selected { theme::SEL_BG } else { theme::BG };
        let key_style = if selected {
            Style::default()
                .fg(theme::CYAN)
                .bg(bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::DIM).bg(bg)
        };
        let name_style = if selected {
            Style::default()
                .fg(theme::FG)
                .bg(bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::FG).bg(bg)
        };
        let cap_style = Style::default().fg(theme::DIM).bg(bg);

        lines.push(Line::from(vec![
            Span::styled(format!(" [{}] ", i + 1), key_style),
            Span::styled(format!("{:<14}", w.name), name_style),
            Span::styled(format!("${:.0} cap", w.cap), cap_style),
        ]));
    }
    f.render_widget(Paragraph::new(lines), area);
}

// ──────────────────────────────────────────────────────────────────────
// PROMPT box
// ──────────────────────────────────────────────────────────────────────
fn prompt_box(f: &mut Frame, area: Rect, app: &App) {
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
                "  enter send  ·  esc clear  ·  esc on empty / ctrl+c to quit  ·  ctrl+1..6 workflow ",
                Style::default().fg(theme::DIM),
            ),
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 4 {
        return;
    }

    // ▸ indicator + textarea
    let indicator_w: u16 = 4;
    let indicator_area = Rect {
        x: inner.x,
        y: inner.y,
        width: indicator_w,
        height: 2.min(inner.height),
    };
    let textarea_area = Rect {
        x: inner.x + indicator_w,
        y: inner.y,
        width: inner.width.saturating_sub(indicator_w),
        height: 2.min(inner.height),
    };

    let lines: Vec<Line<'static>> = (0..indicator_area.height)
        .map(|i| {
            if i == 0 {
                Line::from(Span::styled(
                    " ▸ ",
                    Style::default()
                        .fg(theme::GREEN)
                        .add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::raw("")
            }
        })
        .collect();
    f.render_widget(Paragraph::new(lines), indicator_area);
    f.render_widget(&app.prompt, textarea_area);

    // horizontal rule
    let rule_area = Rect {
        x: inner.x,
        y: inner.y + 2,
        width: inner.width,
        height: 1,
    };
    let rule: String = std::iter::repeat('─').take(rule_area.width as usize).collect();
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            rule,
            Style::default().fg(theme::FAINT),
        ))),
        rule_area,
    );

    // router prediction line
    let hint_area = Rect {
        x: inner.x,
        y: inner.y + 3,
        width: inner.width,
        height: 1,
    };

    let prediction = router_prediction(app);
    f.render_widget(Paragraph::new(prediction), hint_area);
}

fn router_prediction(app: &App) -> Line<'static> {
    let wf = &WORKFLOWS[app.workflow.min(WORKFLOWS.len() - 1)];
    let est = format!("~${:.2} est", wf.cap * 0.10); // wild guess for now
    Line::from(vec![
        Span::styled(" router → ", Style::default().fg(theme::DIM)),
        Span::styled(
            wf.blurb,
            Style::default()
                .fg(theme::CYAN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("   ", Style::default()),
        Span::styled(est, Style::default().fg(theme::FG)),
        Span::styled(
            format!("   cap ${:.0}  budget OK", wf.cap),
            Style::default().fg(theme::DIM),
        ),
    ])
}

// ──────────────────────────────────────────────────────────────────────
// helpers
// ──────────────────────────────────────────────────────────────────────
fn home_relative(p: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(rel) = std::path::Path::new(p).strip_prefix(&home) {
            return format!("~/{}", rel.display());
        }
    }
    p.to_string()
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

fn relative_age(updated: chrono::DateTime<Utc>) -> String {
    let d = Utc::now() - updated;
    if d.num_seconds() < 60 {
        "now".to_string()
    } else if d.num_minutes() < 60 {
        format!("{}m", d.num_minutes())
    } else if d.num_hours() < 24 {
        format!("{}h", d.num_hours())
    } else {
        format!("{}d", d.num_days())
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
fn _placeholder() -> Color {
    theme::FG
}
