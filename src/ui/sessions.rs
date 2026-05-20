use chrono::{Local, TimeZone, Utc};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::App;
use crate::data::ThreadSummary;
use crate::theme;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // filter strip
            Constraint::Length(16), // SESSIONS box
            Constraint::Min(0),     // DETAIL box
        ])
        .split(area);

    filter_strip(f, chunks[0], app);
    sessions_table(f, chunks[1], app);
    detail_box(f, chunks[2], app);
}

fn filter_strip(f: &mut Frame, area: Rect, app: &App) {
    let total = app.threads.len();
    let now = Utc::now();
    let today = app
        .threads
        .iter()
        .filter(|t| (now - t.updated_at).num_hours() < 24)
        .count();
    let week = app
        .threads
        .iter()
        .filter(|t| (now - t.updated_at).num_days() < 7)
        .count();

    // Highlight the smallest non-empty bucket — today wins if it has any,
    // otherwise week, otherwise all. Honest about an empty data set.
    let selected_bucket = if today > 0 {
        Bucket::Today
    } else if week > 0 {
        Bucket::Week
    } else {
        Bucket::All
    };

    let dim = Style::default().fg(theme::DIM);
    let on = Style::default()
        .bg(theme::SEL_BG)
        .fg(theme::FG)
        .add_modifier(Modifier::BOLD);
    let fg = Style::default().fg(theme::FG);

    let mut spans = vec![Span::styled(" show ", dim)];
    let buckets = [
        (Bucket::Today, format!(" today {} ", today), format!("today {}", today)),
        (Bucket::Week, format!(" week {} ", week), format!("week {}", week)),
        (Bucket::All, format!(" all {} ", total), format!("all {}", total)),
    ];
    for (b, hot, cold) in buckets {
        if b == selected_bucket {
            spans.push(Span::styled(hot, on));
        } else {
            spans.push(Span::styled(cold, fg));
        }
        spans.push(Span::raw("  "));
    }

    let summary_threads = match selected_bucket {
        Bucket::Today => today,
        Bucket::Week => week,
        Bucket::All => total,
    };
    let summary_cost: f64 = app
        .threads
        .iter()
        .filter(|t| match selected_bucket {
            Bucket::Today => (now - t.updated_at).num_hours() < 24,
            Bucket::Week => (now - t.updated_at).num_days() < 7,
            Bucket::All => true,
        })
        .map(|t| t.cost_total)
        .sum();
    let label = match selected_bucket {
        Bucket::Today => "today",
        Bucket::Week => "this week",
        Bucket::All => "total",
    };

    spans.push(Span::raw(" "));
    spans.push(Span::styled(
        format!("{} threads {}  ${:.2}", summary_threads, label, summary_cost),
        dim,
    ));

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bucket {
    Today,
    Week,
    All,
}

fn sessions_table(f: &mut Frame, area: Rect, app: &App) {
    let count = app.threads.len();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::FAINT))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "SESSIONS",
                Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  {} threads ", count), Style::default().fg(theme::DIM)),
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 2 {
        return;
    }

    let header_area = Rect { height: 1, ..inner };
    let header = Paragraph::new(Line::from(vec![Span::styled(
        "  THREAD            UPDATED   COST     MSGS  MODELS                                  STATUS",
        Style::default().fg(theme::DIM),
    )]));
    f.render_widget(header, header_area);

    let rows_area = Rect {
        y: inner.y + 1,
        height: inner.height - 1,
        ..inner
    };

    if app.threads.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                "  (no threads found in ~/Library/Application Support/neo/threads/)",
                Style::default().fg(theme::DIM),
            )])),
            rows_area,
        );
        return;
    }

    let mut lines = Vec::with_capacity(rows_area.height as usize);
    let visible = rows_area.height as usize;

    let start = if app.sessions_selected >= visible {
        app.sessions_selected + 1 - visible
    } else {
        0
    };

    for (i, t) in app
        .threads
        .iter()
        .enumerate()
        .skip(start)
        .take(visible)
    {
        lines.push(session_line(t, i == app.sessions_selected));
    }
    f.render_widget(Paragraph::new(lines), rows_area);
}

fn session_line<'a>(t: &'a ThreadSummary, selected: bool) -> Line<'a> {
    let (dot_color, status_text) = status_for(t);
    let id_short = format!("T-...{}", &t.id[t.id.len().saturating_sub(6)..]);
    let updated = Local
        .from_utc_datetime(&t.updated_at.naive_utc())
        .format("%H:%M")
        .to_string();
    let cost = format!("${:.2}", t.cost_total);
    let msgs = format!("{:>4}", t.message_count);
    let models = abbreviate_models(&t.models_used);
    let row = format!(
        "{:<14}  {:<6}  {:<7} {}  {:<38}  ",
        id_short, updated, cost, msgs, truncate(&models, 38)
    );

    let base_style = if selected {
        Style::default().bg(theme::SEL_BG).fg(theme::FG)
    } else {
        Style::default().fg(theme::FG)
    };
    let dot_style = if selected {
        Style::default().bg(theme::SEL_BG).fg(dot_color)
    } else {
        Style::default().fg(dot_color)
    };
    let status_style = dot_style;

    Line::from(vec![
        Span::styled("  ●  ", dot_style),
        Span::styled(row, base_style),
        Span::styled(status_text, status_style),
    ])
}

fn detail_box(f: &mut Frame, area: Rect, app: &App) {
    let title = match app.selected_thread() {
        Some(t) => format!(
            " T-...{}  DETAIL ",
            &t.id[t.id.len().saturating_sub(6)..]
        ),
        None => " DETAIL ".to_string(),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::FAINT))
        .title(Line::from(Span::styled(
            title,
            Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(t) = app.selected_thread() else {
        f.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                "  (no session selected)",
                Style::default().fg(theme::DIM),
            )])),
            inner,
        );
        return;
    };

    let dim = Style::default().fg(theme::DIM);
    let fg = Style::default().fg(theme::FG);

    let task = t
        .first_user_message
        .as_deref()
        .map(|s| format!("\"{}\"", truncate(s, 110)))
        .unwrap_or_else(|| "(no user message)".to_string());

    let models = if t.models_used.is_empty() {
        "—".to_string()
    } else {
        t.models_used.join("  ")
    };
    let tags = if t.tags.is_empty() {
        "—".to_string()
    } else {
        t.tags.join(", ")
    };
    let created = Local
        .from_utc_datetime(&t.created_at.naive_utc())
        .format("%Y-%m-%d %H:%M")
        .to_string();
    let updated = format!("{} ago", format_age(t.updated_at));

    let lines = vec![
        kv("request", task, dim, fg),
        kv("workspace", t.workspace.clone(), dim, fg),
        kv("models", models, dim, fg),
        kv("cost", format!("${:.4}", t.cost_total), dim, fg),
        kv("messages", t.message_count.to_string(), dim, fg),
        kv("created", created, dim, fg),
        kv("updated", updated, dim, fg),
        kv("tags", tags, dim, fg),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

fn kv<'a>(k: &'a str, v: String, k_style: Style, v_style: Style) -> Line<'a> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(format!("{:<11}", k), k_style),
        Span::styled(v, v_style),
    ])
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}

fn abbreviate_models(models: &[String]) -> String {
    if models.is_empty() {
        return "—".to_string();
    }
    // Strip provider prefix for compactness — "anthropic/claude-sonnet-4" → "claude-sonnet-4".
    models
        .iter()
        .map(|m| m.split('/').next_back().unwrap_or(m.as_str()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn status_for(t: &ThreadSummary) -> (ratatui::style::Color, &'static str) {
    // Without state.json we have no live signal. Persisted threads are
    // either complete or the runtime died mid-call — we can't distinguish.
    // Use cost as a weak indicator: 0 cost usually means "no recorded work."
    if t.cost_total > 0.0 {
        (theme::GREEN, "done")
    } else {
        (theme::DIM, "—")
    }
}

fn format_age(updated: chrono::DateTime<Utc>) -> String {
    let d = Utc::now() - updated;
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
