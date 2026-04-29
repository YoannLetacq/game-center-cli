pub mod checkers;
pub mod chess;
pub mod connect4;
pub mod snake;
pub mod terminal_fit;
pub mod tictactoe;

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::tui::app::App;
use gc_shared::types::GameOutcome;

/// Renders a rematch overlay when a rematch request is pending or incoming.
pub fn render_rematch_overlay(frame: &mut Frame, app: &App) {
    if app.rematch_pending {
        render_modal(
            frame,
            " Rematch ",
            &[
                "Rematch requested.",
                "",
                "Waiting for opponent...",
                "C: Cancel",
            ],
            Color::Yellow,
        );
    } else if app.rematch_incoming {
        render_modal(
            frame,
            " Rematch Request ",
            &["Opponent wants a rematch!", "", "Y: Accept   N: Decline"],
            Color::Cyan,
        );
    }
}

pub fn render_help_overlay(frame: &mut Frame, app: &App) {
    use crate::tui::app::ClientGameState;

    let (title, rules) = match &app.game_state {
        Some(ClientGameState::TicTacToe(_)) => (
            "Tic-Tac-Toe Rules",
            vec![
                "Goal: Connect 3 of your marks in a row.",
                "Line can be horizontal, vertical, or diagonal.",
                "",
                "Controls:",
                "- Arrow Keys: Move cursor",
                "- Enter: Place mark",
                "- ?: Close help",
                "- Esc: Leave game",
            ],
        ),
        Some(ClientGameState::Connect4(_)) => (
            "Connect 4 Rules",
            vec![
                "Goal: Connect 4 of your pieces in a row.",
                "Line can be horizontal, vertical, or diagonal.",
                "Players take turns dropping one piece from the top",
                "into one of the seven columns.",
                "",
                "Controls:",
                "- Left/Right: Select column",
                "- Enter: Drop piece",
                "- ?: Close help",
                "- Esc: Leave game",
            ],
        ),
        Some(ClientGameState::Checkers(_)) => (
            "Checkers Rules",
            vec![
                "Goal: Capture all opponent pieces or block them.",
                "Pieces move diagonally forward one square.",
                "Capture by jumping over an opponent's piece.",
                "Reach the back row to become a king (move any direction).",
                "",
                "Controls:",
                "- Arrow Keys: Move cursor",
                "- Enter: Select/move piece",
                "- Esc: Cancel selection",
                "- ?: Close help",
                "- Esc: Leave game",
            ],
        ),
        Some(ClientGameState::Chess(_)) => (
            "Chess Rules",
            vec![
                "Goal: Checkmate the opponent's king.",
                "Each piece has unique movement rules.",
                "Castling: King + Rook (both unmoved, no check).",
                "En passant: Special pawn capture on the 4th rank.",
                "",
                "Controls:",
                "- Arrow Keys: Move cursor",
                "- Enter: Select/move piece",
                "- Esc: Cancel selection",
                "- ?: Close help",
                "- Esc: Leave game",
            ],
        ),
        Some(ClientGameState::Snake(_)) => (
            "Snake Rules",
            vec![
                "Goal: Grow your snake and outscore your opponent.",
                "Eat food to grow and gain points.",
                "Avoid walls and your own body.",
                "Opponent's body counts as obstacles.",
                "",
                "Controls:",
                "- Arrow Keys: Change direction",
                "- ?: Close help",
                "- Esc: Leave game",
            ],
        ),
        _ => (
            "Help",
            vec![
                "Game controls vary by game type.",
                "Press ? to see help anytime during a game.",
            ],
        ),
    };

    let area = centered_rect(60, 60, frame.area());
    frame.render_widget(Clear, area);

    let help_text: Vec<Line> = rules.into_iter().map(Line::from).collect();

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let paragraph = Paragraph::new(help_text)
        .block(block)
        .alignment(Alignment::Left);

    frame.render_widget(paragraph, area);
}

fn render_modal(frame: &mut Frame, title: &str, lines: &[&str], border_color: Color) {
    let area = centered_rect(50, 30, frame.area());
    frame.render_widget(Clear, area);

    let text: Vec<Line> = lines.iter().map(|l| Line::from(*l)).collect();
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(border_color)
                .add_modifier(Modifier::BOLD),
        );
    let paragraph = Paragraph::new(text)
        .block(block)
        .alignment(Alignment::Center);
    frame.render_widget(paragraph, area);
}

#[allow(dead_code)]
pub fn render_header(frame: &mut Frame, area: Rect, _title: &str, app: &App) {
    let t = &app.translator;

    let header_text = if let Some(ref outcome) = app.game_over {
        match outcome {
            GameOutcome::Win(winner) => {
                if Some(*winner) == app.my_player_id {
                    t.get("game.you_win").to_string()
                } else {
                    t.get("game.you_lose").to_string()
                }
            }
            GameOutcome::Draw => t.get("game.draw").to_string(),
        }
    } else if app.is_our_turn() {
        t.get("game.your_turn").to_string()
    } else if app.game_state.is_some() {
        t.get("game.opponent_turn").to_string()
    } else {
        "Waiting for game...".to_string()
    };

    let header_color = if app.game_over.is_some() {
        Color::Yellow
    } else if app.is_our_turn() {
        Color::Green
    } else {
        Color::DarkGray
    };

    let header = Paragraph::new(header_text)
        .alignment(Alignment::Center)
        .style(
            Style::default()
                .fg(header_color)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(header, area);
}

#[allow(dead_code)]
pub fn render_footer(frame: &mut Frame, area: Rect, hint: &str, app: &App) {
    let t = &app.translator;

    let footer_text = if app.rematch_pending {
        format!("Waiting for opponent... | Esc: {}", t.get("game.leave"))
    } else if app.rematch_incoming {
        format!(
            "Y: Accept rematch | N: Decline | Esc: {}",
            t.get("game.leave")
        )
    } else if app.game_over.is_some() {
        format!(
            "R: {} | {} | Esc: {}",
            t.get("game.rematch"),
            hint,
            t.get("game.leave")
        )
    } else {
        format!("{} | Esc: {}", hint, t.get("game.leave"))
    };

    let footer = Paragraph::new(footer_text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
