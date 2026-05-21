//! Hero Panel renderer. Full-screen overlay that takes over the
//! entire frame when `app.builder.is_some()`.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::app::{model_picker_entries, App};
use crate::builder::BuilderFocus;
use crate::data::{
    team::{Team, TeamMember},
    team_tags::{tags_for, warnings_for, Severity},
    Provider,
};
use crate::theme;

pub fn render(f: &mut Frame, app: &App) {
    let area = f.size();
    // Wipe whatever's behind us so we get a clean canvas.
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::CYAN))
        .title(header_title(app));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(11),  // ROSTER + PRESETS row
            Constraint::Length(11),  // SELECTED + COST row
            Constraint::Min(0),      // strengths + warnings
            Constraint::Length(2),   // footer
        ])
        .split(inner);

    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(chunks[0]);
    roster(f, top[0], app);
    presets(f, top[1], app);

    let mid = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(chunks[1]);
    selected(f, mid[0], app);
    cost_preview(f, mid[1], app);

    strengths_warnings(f, chunks[2], app);
    footer(f, chunks[3], app);
}

fn header_title(app: &App) -> Line<'static> {
    let Some(b) = app.builder.as_ref() else {
        return Line::raw(" TEAM BUILDER ");
    };
    let dirty_marker = if b.is_dirty() { "*" } else { "" };
    let bold = Style::default()
        .fg(theme::CYAN)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(theme::DIM);
    Line::from(vec![
        Span::styled(" TEAM BUILDER  ▸ ", dim),
        Span::styled(format!("{}{}", b.editing.name, dirty_marker), bold),
        Span::styled(
            if b.is_dirty() {
                "   unsaved · esc cancel · s save "
            } else {
                "   esc to exit · s save "
            },
            dim,
        ),
    ])
}

fn roster(f: &mut Frame, area: Rect, app: &App) {
    let Some(b) = app.builder.as_ref() else {
        return;
    };
    let active = b.editing.full_roster();
    let included_count = active.iter().filter(|m| m.included).count();
    let focused = b.focus == BuilderFocus::Roster;

    let title = Line::from(vec![
        Span::styled(" ROSTER ", Style::default().fg(theme::DIM)),
        Span::styled(
            format!("{}/8 ", included_count),
            Style::default()
                .fg(theme::CYAN)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style(focused))
        .title(title);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::with_capacity(8);
    for (i, m) in active.iter().enumerate() {
        let selected = focused && i == b.roster_idx;
        let bg = if selected { theme::SEL_BG } else { theme::BG };

        let cursor = if selected { "▸" } else { " " };
        let check = if m.included { "✓" } else { "□" };
        let check_color = if m.included { theme::GREEN } else { theme::DIM };
        let agent_style = if m.included {
            Style::default().fg(theme::FG).bg(bg)
        } else {
            Style::default().fg(theme::DIM).bg(bg)
        };
        let count = if m.count > 1 {
            format!("×{}", m.count)
        } else {
            "  ".to_string()
        };
        let prov = if m.model == "auto" {
            Provider::Unknown
        } else {
            crate::data::provider_for(&m.model)
        };
        let model_label = if m.model == "auto" {
            "(router)".to_string()
        } else {
            short_model(&m.model)
        };
        lines.push(Line::from(vec![
            Span::styled(format!(" {} ", cursor), Style::default().fg(theme::CYAN).bg(bg)),
            Span::styled(format!("{} ", check), Style::default().fg(check_color).bg(bg)),
            Span::styled(format!("{:<10} ", m.agent), agent_style),
            Span::styled(
                format!("{:<3} ", count),
                Style::default().fg(theme::YELLOW).bg(bg),
            ),
            Span::styled(
                format!("[{}] ", prov.badge()),
                Style::default().fg(prov.color()).bg(bg),
            ),
            Span::styled(model_label, agent_style),
        ]));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn presets(f: &mut Frame, area: Rect, app: &App) {
    let Some(b) = app.builder.as_ref() else {
        return;
    };
    let focused = b.focus == BuilderFocus::Presets;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style(focused))
        .title(Line::from(vec![Span::styled(
            " PRESETS ",
            Style::default().fg(theme::DIM),
        )]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();
    for (i, t) in app.teams.iter().enumerate() {
        let selected = focused && i == b.presets_idx;
        let active = i == app.active_team;
        let bg = if selected { theme::SEL_BG } else { theme::BG };
        let cursor = if selected {
            "▸"
        } else if active {
            "●"
        } else {
            " "
        };
        let cursor_color = if active { theme::CYAN } else { theme::DIM };
        let size = t.active_size();
        let tier = cost_tier(t);
        let name_style = Style::default().fg(theme::FG).bg(bg);
        let marker = if t.is_preset {
            ""
        } else {
            "*"
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {} ", cursor),
                Style::default().fg(cursor_color).bg(bg),
            ),
            Span::styled(format!("{:<14}", format!("{}{}", t.name, marker)), name_style),
            Span::styled(
                format!(" {}", size),
                Style::default().fg(theme::DIM).bg(bg),
            ),
            Span::styled("  ·  ", Style::default().fg(theme::FAINT).bg(bg)),
            Span::styled(tier, Style::default().fg(tier_color(t)).bg(bg)),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        " n  new from blank",
        Style::default().fg(theme::FAINT),
    )));
    f.render_widget(Paragraph::new(lines), inner);
}

fn selected(f: &mut Frame, area: Rect, app: &App) {
    let Some(b) = app.builder.as_ref() else {
        return;
    };
    let focused = b.focus == BuilderFocus::Models;
    let member = b.selected_member();
    let title_agent = member.as_ref().map(|m| m.agent.clone()).unwrap_or_default();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style(focused))
        .title(Line::from(vec![
            Span::styled(" SELECTED ", Style::default().fg(theme::DIM)),
            Span::styled(
                title_agent,
                Style::default()
                    .fg(theme::CYAN)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                if focused { "  m·exit picker  enter·apply " } else { "  m·models " },
                Style::default().fg(theme::DIM),
            ),
        ]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let entries = model_picker_entries();
    let current_model = member
        .as_ref()
        .map(|m| m.model.clone())
        .unwrap_or_else(|| "auto".to_string());

    let mut lines = Vec::new();
    let visible_h = inner.height as usize;
    let start = if focused && b.models_idx >= visible_h {
        b.models_idx + 1 - visible_h
    } else {
        0
    };

    for (i, (prov, model)) in entries.iter().enumerate().skip(start).take(visible_h) {
        let selected = focused && i == b.models_idx;
        let is_current = *model == current_model;
        let bg = if selected { theme::SEL_BG } else { theme::BG };
        let cursor = if selected {
            "▸"
        } else if is_current {
            "●"
        } else {
            " "
        };
        let cursor_color = if is_current { theme::GREEN } else { theme::DIM };
        let badge = if *model == "auto" {
            "auto".to_string()
        } else {
            format!("[{}]", prov.badge())
        };
        let model_label = if *model == "auto" {
            "router auto".to_string()
        } else {
            short_model(model)
        };
        let rate = if *model == "auto" {
            "(decided per task)".to_string()
        } else if let Some(p) = crate::data::pricing::lookup_with(*prov, model) {
            if p.is_subscription {
                "sub.".to_string()
            } else if p.input_per_mtok == 0.0 && p.output_per_mtok == 0.0 {
                "free".to_string()
            } else {
                format!("${}/${} per Mtok", p.input_per_mtok, p.output_per_mtok)
            }
        } else {
            "—".to_string()
        };

        lines.push(Line::from(vec![
            Span::styled(format!(" {} ", cursor), Style::default().fg(cursor_color).bg(bg)),
            Span::styled(
                format!("{:<6} ", badge),
                Style::default().fg(prov.color()).bg(bg),
            ),
            Span::styled(
                format!("{:<24}", model_label),
                Style::default().fg(theme::FG).bg(bg),
            ),
            Span::styled(format!(" {}", rate), Style::default().fg(theme::DIM).bg(bg)),
        ]));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn cost_preview(f: &mut Frame, area: Rect, app: &App) {
    let Some(b) = app.builder.as_ref() else {
        return;
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::FAINT))
        .title(Line::from(vec![Span::styled(
            " COST PREVIEW  per task ",
            Style::default().fg(theme::DIM),
        )]));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let team = &b.editing;
    let mut total = 0.0_f64;
    let mut lines = Vec::new();
    for m in team.members.iter().filter(|m| m.included) {
        let cost = member_cost(m);
        total += cost;
        let prov = if m.model == "auto" {
            Provider::Unknown
        } else {
            crate::data::provider_for(&m.model)
        };
        let model_label = if m.model == "auto" {
            "(auto)".to_string()
        } else {
            short_model(&m.model)
        };
        let cost_label = if matches!(prov, Provider::Copilot) {
            "sub.".to_string()
        } else if matches!(prov, Provider::Ollama) {
            "free".to_string()
        } else {
            format!("${:.3}", cost)
        };
        let count_label = if m.count > 1 {
            format!("×{}", m.count)
        } else {
            "   ".to_string()
        };
        lines.push(Line::from(vec![
            Span::styled(format!(" {:<10}", m.agent), Style::default().fg(theme::FG)),
            Span::styled(format!("{:<4}", count_label), Style::default().fg(theme::YELLOW)),
            Span::styled(format!("{:<16}", model_label), Style::default().fg(theme::FG)),
            Span::styled(cost_label, Style::default().fg(theme::DIM)),
        ]));
    }
    let rule: String = std::iter::repeat('─').take(inner.width as usize).collect();
    lines.push(Line::from(Span::styled(rule, Style::default().fg(theme::FAINT))));
    let (tier_label, tier_clr) = tier_for_total(total);
    lines.push(Line::from(vec![
        Span::styled(" TOTAL      ", Style::default().fg(theme::DIM)),
        Span::styled(
            format!("${:.3}", total),
            Style::default()
                .fg(theme::FG)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("   tier {}", tier_label),
            Style::default().fg(tier_clr),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled(
            " daily ×50",
            Style::default().fg(theme::DIM),
        ),
        Span::styled(
            format!("  ${:.2}", total * 50.0),
            Style::default().fg(theme::FG),
        ),
    ]));
    f.render_widget(Paragraph::new(lines), inner);
}

fn strengths_warnings(f: &mut Frame, area: Rect, app: &App) {
    let Some(b) = app.builder.as_ref() else {
        return;
    };
    let tags = tags_for(&b.editing);
    let warns = warnings_for(&b.editing);

    let mut lines = Vec::new();
    let mut tags_line = vec![Span::styled(
        " STRENGTHS  ",
        Style::default().fg(theme::DIM).add_modifier(Modifier::BOLD),
    )];
    if tags.is_empty() {
        tags_line.push(Span::styled(
            "—",
            Style::default().fg(theme::DIM),
        ));
    } else {
        for (i, t) in tags.iter().enumerate() {
            if i > 0 {
                tags_line.push(Span::raw("  "));
            }
            tags_line.push(Span::styled(
                t.label.to_string(),
                Style::default().fg(theme::CYAN),
            ));
        }
    }
    lines.push(Line::from(tags_line));

    if warns.is_empty() {
        lines.push(Line::from(vec![
            Span::styled(
                " WARNINGS   ",
                Style::default().fg(theme::DIM).add_modifier(Modifier::BOLD),
            ),
            Span::styled("none", Style::default().fg(theme::DIM)),
        ]));
    } else {
        for (i, w) in warns.iter().enumerate() {
            let prefix = if i == 0 {
                Span::styled(
                    " WARNINGS   ",
                    Style::default().fg(theme::DIM).add_modifier(Modifier::BOLD),
                )
            } else {
                Span::raw("            ")
            };
            let color = match w.severity {
                Severity::Crit => theme::RED,
                Severity::Warn => theme::YELLOW,
                Severity::Info => theme::DIM,
            };
            lines.push(Line::from(vec![
                prefix,
                Span::styled(w.message.clone(), Style::default().fg(color)),
            ]));
        }
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn footer(f: &mut Frame, area: Rect, _app: &App) {
    let dim = Style::default().fg(theme::DIM);
    let key = Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD);
    let lines = vec![
        Line::from(vec![
            Span::styled(" ↑↓ ", key),
            Span::styled("navigate  ", dim),
            Span::styled("tab ", key),
            Span::styled("cycle pane  ", dim),
            Span::styled("space ", key),
            Span::styled("toggle  ", dim),
            Span::styled("+/- ", key),
            Span::styled("count  ", dim),
            Span::styled("m ", key),
            Span::styled("models  ", dim),
            Span::styled("p ", key),
            Span::styled("presets  ", dim),
            Span::styled("s ", key),
            Span::styled("save  ", dim),
            Span::styled("n ", key),
            Span::styled("new  ", dim),
            Span::styled("esc ", key),
            Span::styled("out", dim),
        ]),
        Line::raw(""),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

// ── helpers ──────────────────────────────────────────────────────────

fn border_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(theme::CYAN)
    } else {
        Style::default().fg(theme::FAINT)
    }
}

fn short_model(model: &str) -> String {
    model.split('/').next_back().unwrap_or(model).to_string()
}

fn member_cost(m: &TeamMember) -> f64 {
    use crate::data::pricing;
    const TYP_IN: u32 = 3_000;
    const TYP_OUT: u32 = 800;
    const AUTO_FALLBACK: &str = "anthropic/claude-sonnet-4";
    let model = if m.model == "auto" { AUTO_FALLBACK } else { &m.model };
    pricing::compute_cost(model, TYP_IN, TYP_OUT) * m.count as f64
}

fn cost_tier(t: &Team) -> &'static str {
    let total: f64 = t.members.iter().filter(|m| m.included).map(member_cost).sum();
    tier_for_total(total).0
}

fn tier_color(t: &Team) -> Color {
    let total: f64 = t.members.iter().filter(|m| m.included).map(member_cost).sum();
    tier_for_total(total).1
}

fn tier_for_total(total: f64) -> (&'static str, Color) {
    if total >= 1.0 {
        ("$$$$", theme::RED)
    } else if total >= 0.20 {
        ("$$$", theme::YELLOW)
    } else if total >= 0.05 {
        ("$$", theme::CYAN)
    } else if total > 0.0 {
        ("$", theme::GREEN)
    } else {
        ("free", theme::GREEN)
    }
}
