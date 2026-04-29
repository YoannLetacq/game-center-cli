//! Graceful degradation when the terminal is too small to host the full UI.
//!
//! Three fit tiers keyed off the frame's usable area:
//! - `Ok`     → normal rendering.
//! - `Edge`   → just below the recommended size; show a compact icon panel.
//! - `TooSmall` → below the minimum; show a one-line "please enlarge" message.

use gc_shared::types::GameType;
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

/// Get per-game minimum dimensions. Returns (min_cols, min_rows).
fn game_minimums(game_type: Option<GameType>) -> (u16, u16) {
    match game_type {
        Some(GameType::TicTacToe) => (30, 12),
        Some(GameType::Connect4) => (50, 18),
        Some(GameType::Snake) | Some(GameType::Chess) | Some(GameType::Checkers) => (80, 24),
        // Default for other games and None (lobby/login screens)
        _ => (MIN_COLS, MIN_ROWS),
    }
}

/// Check terminal fit with default minimums (backward compatibility for legacy callers).
/// Prefer `check_fit_for_game` for game-specific sizing.
#[allow(dead_code)]
pub fn check_fit(area: Rect) -> TerminalFit {
    check_fit_for_game(area, None)
}

pub fn check_fit_for_game(area: Rect, game_type: Option<GameType>) -> TerminalFit {
    let (min_cols, min_rows) = game_minimums(game_type);
    // First check if we're below the absolute floor (can't render anything useful)
    if area.width < EDGE_MIN_COLS || area.height < EDGE_MIN_ROWS {
        TerminalFit::TooSmall
    }
    // Then check if we're below the game-specific minimum
    else if area.width < min_cols || area.height < min_rows {
        TerminalFit::Edge
    }
    // Otherwise we're Ok
    else {
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

    #[test]
    fn per_game_minimums_tictactoe() {
        use gc_shared::types::GameType;
        // TicTacToe minimum: 30x12, but EDGE_MIN_COLS=30, EDGE_MIN_ROWS=10
        // So 30x12 should be Ok
        assert_eq!(check_fit_for_game(rect(30, 12), Some(GameType::TicTacToe)), TerminalFit::Ok);
        // Below the floor is TooSmall, not Edge
        assert_eq!(check_fit_for_game(rect(29, 12), Some(GameType::TicTacToe)), TerminalFit::TooSmall);
        // Between floor (10) and game minimum (12) is Edge
        assert_eq!(check_fit_for_game(rect(30, 11), Some(GameType::TicTacToe)), TerminalFit::Edge);
    }

    #[test]
    fn per_game_minimums_connect4() {
        use gc_shared::types::GameType;
        // Connect4 minimum: 50x18
        assert_eq!(check_fit_for_game(rect(50, 18), Some(GameType::Connect4)), TerminalFit::Ok);
        assert_eq!(check_fit_for_game(rect(49, 18), Some(GameType::Connect4)), TerminalFit::Edge);
        assert_eq!(check_fit_for_game(rect(50, 17), Some(GameType::Connect4)), TerminalFit::Edge);
    }

    #[test]
    fn per_game_minimums_snake_chess_checkers() {
        use gc_shared::types::GameType;
        // Snake, Chess, Checkers: 80x24
        assert_eq!(check_fit_for_game(rect(80, 24), Some(GameType::Snake)), TerminalFit::Ok);
        assert_eq!(check_fit_for_game(rect(79, 24), Some(GameType::Snake)), TerminalFit::Edge);
        assert_eq!(check_fit_for_game(rect(80, 24), Some(GameType::Chess)), TerminalFit::Ok);
        assert_eq!(check_fit_for_game(rect(80, 24), Some(GameType::Checkers)), TerminalFit::Ok);
    }
}
