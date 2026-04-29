use gc_shared::game::checkers::{self, BOARD_SIZE, CheckersState, Position, Side, Square};
use gc_shared::types::GameOutcome;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::tui::app::{App, CheckersInputStage, ClientGameState};

const CELL_W: u16 = 7;
const CELL_H: u16 = 3;

pub fn render(frame: &mut Frame, app: &App) {
    let t = &app.translator;

    let board_height = (BOARD_SIZE as u16) * CELL_H + 2;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),         // header
            Constraint::Min(board_height), // board
            Constraint::Length(1),         // hint
            Constraint::Length(3),         // footer
        ])
        .split(frame.area());

    // Header — turn indicator / game over.
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
    frame.render_widget(header, chunks[0]);

    // Board.
    let board_area = fixed_board_rect(chunks[1]);
    if let Some(ClientGameState::Checkers(ref state)) = app.game_state {
        render_board(frame, state, app, board_area);
    }

    // Hint line (status message or stage hint).
    let hint_text = if let Some(ref msg) = app.status_message {
        msg.clone()
    } else if app.game_over.is_some() {
        String::new()
    } else {
        match app.checkers_input.stage {
            CheckersInputStage::Idle => "Select piece (arrows + Enter)".to_string(),
            CheckersInputStage::TargetSelect => "Select target (arrows + Enter)".to_string(),
            CheckersInputStage::Chaining => "Continue jump or press Esc to cancel".to_string(),
        }
    };
    let hint = Paragraph::new(hint_text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Yellow));
    frame.render_widget(hint, chunks[2]);

    // Footer.
    let footer_text = if app.rematch_pending {
        format!("Waiting for opponent... | Esc: {}", t.get("game.leave"))
    } else if app.rematch_incoming {
        format!(
            "Y: Accept rematch | N: Decline | Esc: {}",
            t.get("game.leave")
        )
    } else if app.game_over.is_some() {
        format!(
            "R: {} | Esc: {}",
            t.get("game.rematch"),
            t.get("game.leave")
        )
    } else {
        "Arrows: move | Enter: select/target | Esc: cancel/leave".to_string()
    };
    let footer = Paragraph::new(footer_text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, chunks[3]);

    if app.rematch_pending || app.rematch_incoming {
        super::render_rematch_overlay(frame, app);
    }
}

fn fixed_board_rect(area: Rect) -> Rect {
    let board_width = (BOARD_SIZE as u16) * CELL_W + 2;
    let board_height = (BOARD_SIZE as u16) * CELL_H + 2;
    let x = area.x + area.width.saturating_sub(board_width) / 2;
    let y = area.y + area.height.saturating_sub(board_height) / 2;
    Rect::new(
        x,
        y,
        board_width.min(area.width),
        board_height.min(area.height),
    )
}

fn render_board(frame: &mut Frame, state: &CheckersState, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Checkers ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let row_constraints: Vec<Constraint> = (0..BOARD_SIZE)
        .map(|_| Constraint::Length(CELL_H))
        .collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(inner);

    let origin = app.checkers_input.origin;
    // Squares already committed as landings in the current partial_path
    // (skip element 0, which is the origin).
    let mid_path: Vec<Position> = app
        .checkers_input
        .partial_path
        .iter()
        .skip(1)
        .copied()
        .collect();

    // Legal next-step landings for the currently selected piece / chain prefix.
    // While a piece is selected, every move whose path starts with the current
    // partial_path is a candidate; the next position in that path is a valid target.
    let legal_targets: Vec<Position> = if origin.is_some() {
        let prefix = &app.checkers_input.partial_path;
        checkers::legal_moves(state)
            .into_iter()
            .filter_map(|mv| {
                if mv.path.len() > prefix.len() && mv.path.starts_with(prefix) {
                    Some(mv.path[prefix.len()])
                } else {
                    None
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    for board_row in 0..BOARD_SIZE {
        let row_area = rows[board_row];
        let col_constraints: Vec<Constraint> = (0..BOARD_SIZE)
            .map(|_| Constraint::Length(CELL_W))
            .collect();
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(col_constraints)
            .split(row_area);

        for board_col in 0..BOARD_SIZE {
            let cell_area = cols[board_col];
            let square = state.board[board_row][board_col];
            let is_dark = App::checkers_is_dark_square(board_row as u8, board_col as u8);
            let is_cursor = board_row as u8 == app.cursor_row
                && board_col as u8 == app.cursor_col
                && app.game_over.is_none();
            let is_origin = origin
                .map(|p| p.row as usize == board_row && p.col as usize == board_col)
                .unwrap_or(false);
            let is_mid = mid_path
                .iter()
                .any(|p| p.row as usize == board_row && p.col as usize == board_col);
            let is_legal_target = legal_targets
                .iter()
                .any(|p| p.row as usize == board_row && p.col as usize == board_col);

            // Color scheme mirrors the chess renderer for a consistent look.
            // Selection / target highlights take priority over the base square color.
            let bg = if is_origin || is_mid {
                Color::Rgb(170, 162, 58) // muted gold — selected piece / committed chain landings
            } else if is_cursor && is_legal_target {
                Color::Rgb(102, 190, 150) // cyan/green blend
            } else if is_cursor {
                Color::Rgb(92, 179, 204) // muted cyan
            } else if is_legal_target {
                Color::Rgb(130, 151, 105) // muted green — valid next step
            } else if is_dark {
                Color::Rgb(181, 136, 99) // chess.com dark
            } else {
                Color::Rgb(240, 217, 181) // chess.com light
            };

            // Use unicode draughts glyphs and white/black piece colors so the
            // two sides are clearly distinguishable on the new tile palette.
            let (glyph, fg) = match square {
                Square::Empty => (' ', Color::White),
                Square::Man(Side::Black) => ('\u{26C2}', Color::Black), // ⛂
                Square::King(Side::Black) => ('\u{26C3}', Color::Black), // ⛃
                Square::Man(Side::Red) => ('\u{26C0}', Color::White),   // ⛀ (rendered white)
                Square::King(Side::Red) => ('\u{26C1}', Color::White),  // ⛁
            };

            let style = Style::default().bg(bg).fg(fg).add_modifier(Modifier::BOLD);
            let mut lines: Vec<Line> = Vec::with_capacity(CELL_H as usize);
            for i in 0..CELL_H {
                let content = if i == CELL_H / 2 {
                    format!("   {}   ", glyph)
                } else {
                    "       ".to_string()
                };
                lines.push(Line::from(Span::styled(content, style)));
            }
            let cell_widget = Paragraph::new(lines).style(style);
            frame.render_widget(cell_widget, cell_area);
        }
    }
}
