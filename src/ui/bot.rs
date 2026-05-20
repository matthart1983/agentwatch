//! R2-style "thinking" bot animation. Ported from `~/bot.sh`:
//! 5 rows tall, the body stays put, the lens scans left→right with a yellow
//! pulse on frame 5, and hologram dots fade in above the head. 6 frames at
//! roughly 5 fps (every 6 main-loop ticks).

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::theme;

const BOT_W: u16 = 9;
const BOT_H: u16 = 5;

/// Render the bot at the given (top-left aligned) rect. Pass `working=true`
/// for the scanning animation, `false` for an idle, dim, centered lens.
/// `frame` is the App tick counter.
pub fn render(f: &mut Frame, area: Rect, frame: u64, working: bool) {
    if area.width < BOT_W || area.height < BOT_H {
        return;
    }
    let lines = if working {
        frame_for(frame)
    } else {
        idle_frame()
    };
    let bot_area = Rect {
        x: area.x,
        y: area.y,
        width: BOT_W,
        height: BOT_H,
    };
    f.render_widget(Paragraph::new(lines), bot_area);
}

pub fn width() -> u16 { BOT_W }
pub fn height() -> u16 { BOT_H }

fn idle_frame() -> Vec<Line<'static>> {
    let body = Style::default().fg(theme::FG);
    let panel = Style::default().fg(theme::CYAN);
    let lens_off = Style::default().fg(theme::DIM);
    vec![
        Line::raw(""),
        Line::from(Span::styled("  ▗▄▄▖   ", body)),
        Line::from(vec![
            Span::styled("  ▐ ", body),
            Span::styled("◌", lens_off),
            Span::styled("  ▌  ", body),
        ]),
        Line::from(vec![
            Span::styled("  ▐", body),
            Span::styled("████", panel.add_modifier(Modifier::DIM)),
            Span::styled("▌  ", body),
        ]),
        Line::from(Span::styled("  ▐▌  ▐▌ ", body)),
    ]
}

fn frame_for(tick: u64) -> Vec<Line<'static>> {
    // 6 frames, advance every 6 ticks ≈ 5 fps.
    let idx = (tick / 6) % 6;
    match idx {
        0 => f1(),
        1 => f2(),
        2 => f3(),
        3 => f4(),
        4 => f5(),
        _ => f6(),
    }
}

// Styles --------------------------------------------------------------
fn body() -> Style { Style::default().fg(theme::FG) }
fn panel() -> Style { Style::default().fg(theme::CYAN).add_modifier(Modifier::BOLD) }
fn lens() -> Style { Style::default().fg(theme::RED).add_modifier(Modifier::BOLD) }
fn pulse() -> Style { Style::default().fg(theme::YELLOW).add_modifier(Modifier::BOLD) }
fn holo() -> Style { Style::default().fg(theme::CYAN) }
fn dim_holo() -> Style { Style::default().fg(theme::DIM) }

// Helpers to compose the body rows around a positioned lens (0..=3).
fn lens_row(pos: u8, glyph: &'static str, lens_style: Style) -> Line<'static> {
    let mut spans = vec![Span::styled("  ▐", body())];
    for i in 0..4u8 {
        if i == pos {
            spans.push(Span::styled(glyph, lens_style));
        } else {
            spans.push(Span::raw(" "));
        }
    }
    spans.push(Span::styled("▌  ", body()));
    Line::from(spans)
}

fn rim_top() -> Line<'static> {
    Line::from(Span::styled("  ▗▄▄▖   ", body()))
}
fn panel_row() -> Line<'static> {
    Line::from(vec![
        Span::styled("  ▐", body()),
        Span::styled("████", panel()),
        Span::styled("▌  ", body()),
    ])
}
fn legs() -> Line<'static> {
    Line::from(Span::styled("  ▐▌  ▐▌ ", body()))
}

// Hologram dot lines. The dots sit above the head; brightness varies per
// frame to imply they're materialising / dissipating.
fn dots(states: [Option<Style>; 4]) -> Line<'static> {
    let mut spans = Vec::with_capacity(8);
    spans.push(Span::raw(" "));
    for s in states {
        match s {
            Some(st) => spans.push(Span::styled("·", st)),
            None => spans.push(Span::raw(" ")),
        }
        spans.push(Span::raw(" "));
    }
    Line::from(spans)
}

// 6 frames matching bot.sh ------------------------------------------
fn f1() -> Vec<Line<'static>> {
    vec![
        dots([None, None, Some(dim_holo()), None]),
        rim_top(),
        lens_row(0, "◉", lens()),
        panel_row(),
        legs(),
    ]
}
fn f2() -> Vec<Line<'static>> {
    vec![
        dots([None, Some(holo()), Some(dim_holo()), None]),
        rim_top(),
        lens_row(1, "◉", lens()),
        panel_row(),
        legs(),
    ]
}
fn f3() -> Vec<Line<'static>> {
    vec![
        dots([Some(holo()), Some(holo()), Some(dim_holo()), None]),
        rim_top(),
        lens_row(2, "◉", lens()),
        panel_row(),
        legs(),
    ]
}
fn f4() -> Vec<Line<'static>> {
    vec![
        dots([Some(holo()), Some(holo()), Some(holo()), Some(dim_holo())]),
        rim_top(),
        lens_row(3, "◉", lens()),
        panel_row(),
        legs(),
    ]
}
fn f5() -> Vec<Line<'static>> {
    vec![
        dots([Some(holo()), Some(holo()), Some(dim_holo()), None]),
        rim_top(),
        lens_row(1, "◌", pulse()),
        panel_row(),
        legs(),
    ]
}
fn f6() -> Vec<Line<'static>> {
    vec![
        dots([None, Some(holo()), Some(dim_holo()), None]),
        rim_top(),
        lens_row(1, "◉", lens()),
        panel_row(),
        legs(),
    ]
}
