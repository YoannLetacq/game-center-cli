//! Graceful degradation when the terminal is too small to host the full UI.
//!
//! Three fit tiers keyed off the frame's usable area:
//! - `Ok`     → normal rendering.
//! - `Edge`   → just below the recommended size; show a compact icon panel.
//! - `TooSmall` → below the minimum; show a one-line "please enlarge" message.

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

/// Minimum dimensions for the full UI (header + board + footer).
pub const MIN_COLS: u16 = 80;
pub const MIN_ROWS: u16 = 24;

/// Edge threshold: below this, we refuse to render at all.
pub const EDGE_MIN_COLS: u16 = 30;
pub const EDGE_MIN_ROWS: u16 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalFit {
    Ok,
    Edge,
    TooSmall,
}

pub fn check_fit(area: Rect) -> TerminalFit {
    if area.width < EDGE_MIN_COLS || area.height < EDGE_MIN_ROWS {
        TerminalFit::TooSmall
    } else if area.width < MIN_COLS || area.height < MIN_ROWS {
        TerminalFit::Edge
    } else {
        TerminalFit::Ok
    }
}

/// Render the compact edge-case icon panel. Shown when the terminal is
/// borderline — big enough to read a sentence, too small for the game UI.
pub fn render_edge_icon(frame: &mut Frame, area: Rect) {
    let (w, h) = (area.width, area.height);
    let body = vec![
        Line::from(Span::styled(
            "[ ▢◊▢ ]",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Game Center",
            Style::default().fg(Color::White),
        )),
        Line::from(Span::styled(
            format!("Need {MIN_COLS}x{MIN_ROWS}, have {w}x{h}"),
            Style::default().fg(Color::Yellow),
        )),
        Line::from(Span::styled(
            "Please enlarge terminal",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let widget = Paragraph::new(body)
        .alignment(Alignment::Center)
        .block(block);
    frame.render_widget(widget, area);
}

/// Render the "too small" fallback: a single centered line. Safe for very
/// narrow frames (no borders, no multi-line layout).
pub fn render_too_small(frame: &mut Frame, area: Rect) {
    let msg = Paragraph::new(Line::from(Span::styled(
        "Terminal too small",
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
    )))
    .alignment(Alignment::Center);
    frame.render_widget(msg, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(w: u16, h: u16) -> Rect {
        Rect::new(0, 0, w, h)
    }

    #[test]
    fn ok_at_minimum_recommended_size() {
        assert_eq!(check_fit(rect(MIN_COLS, MIN_ROWS)), TerminalFit::Ok);
        assert_eq!(check_fit(rect(120, 40)), TerminalFit::Ok);
    }

    #[test]
    fn edge_when_narrow_or_short_but_above_floor() {
        assert_eq!(check_fit(rect(MIN_COLS - 1, MIN_ROWS)), TerminalFit::Edge);
        assert_eq!(check_fit(rect(MIN_COLS, MIN_ROWS - 1)), TerminalFit::Edge);
        assert_eq!(
            check_fit(rect(EDGE_MIN_COLS, EDGE_MIN_ROWS)),
            TerminalFit::Edge
        );
    }

    #[test]
    fn too_small_below_edge_floor() {
        assert_eq!(
            check_fit(rect(EDGE_MIN_COLS - 1, EDGE_MIN_ROWS)),
            TerminalFit::TooSmall
        );
        assert_eq!(
            check_fit(rect(EDGE_MIN_COLS, EDGE_MIN_ROWS - 1)),
            TerminalFit::TooSmall
        );
        assert_eq!(check_fit(rect(10, 5)), TerminalFit::TooSmall);
    }
}
