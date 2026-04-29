use gc_shared::game::chess::{
    self, ChessState, Piece, PieceKind, Position, Side,
};
use gc_shared::types::GameOutcome;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::tui::app::{App, ClientGameState};

const BOARD_SIZE: usize = 8;
const CELL_W: u16 = 5;
const CELL_H: u16 = 3;

pub fn render(frame: &mut Frame, app: &App) {
    let t = &app.translator;

    let board_height = (BOARD_SIZE as u16) * CELL_H + 2;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(board_height),
            Constraint::Length(1),
            Constraint::Length(3),
        ])
        .split(frame.area());

    // Header — turn / game over / check.
    let in_check = match app.game_state {
        Some(ClientGameState::Chess(ref s)) => {
            let side = if s.current_turn == 0 { Side::White } else { Side::Black };
            chess::in_check(s, side)
        }
        _ => false,
    };

    let header_text = if let Some(ref outcome) = app.game_over {
        match outcome {
            GameOutcome::Win(winner) => {
                if Some(*winner) == app.my_player_id {
                    format!("Checkmate! {}", t.get("game.you_win"))
                } else {
                    format!("Checkmate! {}", t.get("game.you_lose"))
                }
            }
            GameOutcome::Draw => {
                // Could be stalemate, 50-move, repetition, insufficient material.
                t.get("game.draw").to_string()
            }
        }
    } else if app.is_our_turn() {
        if in_check {
            format!("{} — Check!", t.get("game.your_turn"))
        } else {
            t.get("game.your_turn").to_string()
        }
    } else if app.game_state.is_some() {
        if in_check {
            format!("{} — Check!", t.get("game.opponent_turn"))
        } else {
            t.get("game.opponent_turn").to_string()
        }
    } else {
        "Waiting for game...".to_string()
    };

    let header_color = if app.game_over.is_some() {
        // Checkmate is more urgent than check — make it equally bright.
        match app.game_over {
            Some(GameOutcome::Win(_)) => Color::Rgb(255, 85, 85),
            _ => Color::Rgb(240, 200, 80),
        }
    } else if in_check {
        Color::Rgb(255, 85, 85)
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
    if let Some(ClientGameState::Chess(ref state)) = app.game_state {
        render_board(frame, state, app, board_area);
    }

    // Hint line.
    let hint_text = if app.chess_input.pending_promotion.is_some() {
        "Promote: Q=Queen R=Rook B=Bishop N=Knight | Esc/Backspace: cancel".to_string()
    } else if app.game_over.is_some() {
        String::new()
    } else if app.chess_input.selected_from.is_some() {
        "Select target square (Enter) | Backspace: clear".to_string()
    } else {
        "Select your piece (arrows + Enter)".to_string()
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
        "Arrows: move | Enter: select/target | Backspace: clear | Esc: leave".to_string()
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

fn render_board(frame: &mut Frame, state: &ChessState, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Chess ")
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

    // Compute legal targets when a piece is selected.
    let selected_from = app.chess_input.selected_from;
    let legal_targets: Vec<Position> = if let Some(from) = selected_from {
        chess::legal_moves(state)
            .into_iter()
            .filter(|m| m.from == from)
            .map(|m| m.to)
            .collect()
    } else {
        Vec::new()
    };

    // Highlight king square in red when its side is in check.
    let side_in_check_king: Option<Position> = {
        let mut found = None;
        for side in [Side::White, Side::Black] {
            if chess::in_check(state, side) {
                if let Some(kpos) = chess::king_position(state, side) {
                    found = Some(kpos);
                    break;
                }
            }
        }
        found
    };

    // Render top-down so rank 8 (row 7) is on top, rank 1 (row 0) on the bottom.
    for visual_row in 0..BOARD_SIZE {
        let board_row = BOARD_SIZE - 1 - visual_row;
        let row_area = rows[visual_row];
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
            let is_light = (board_row + board_col) % 2 == 1;
            let is_cursor = board_row as u8 == app.chess_input.cursor_row
                && board_col as u8 == app.chess_input.cursor_col
                && app.game_over.is_none();
            let is_selected = selected_from
                .map(|p| p.row as usize == board_row && p.col as usize == board_col)
                .unwrap_or(false);
            let is_legal_target = legal_targets
                .iter()
                .any(|p| p.row as usize == board_row && p.col as usize == board_col);

            let is_check_king = side_in_check_king
                .map(|p| p.row as usize == board_row && p.col as usize == board_col)
                .unwrap_or(false);

            // Cursor + legal target: blend so both are visible.
            let bg = if is_selected {
                Color::Rgb(170, 162, 58) // muted gold — keeps both white/black pieces readable
            } else if is_cursor && is_legal_target {
                Color::Rgb(102, 190, 150) // cyan/green blend — distinct from either alone
            } else if is_cursor {
                Color::Rgb(92, 179, 204) // muted cyan
            } else if is_legal_target {
                Color::Rgb(130, 151, 105) // muted green
            } else if is_check_king {
                Color::Rgb(220, 80, 80) // red highlight on the checked king's square
            } else if is_light {
                Color::Rgb(240, 217, 181) // chess.com light
            } else {
                Color::Rgb(181, 136, 99) // chess.com dark
            };

            let (glyph, fg) = piece_glyph(square);
            let style = Style::default().bg(bg).fg(fg).add_modifier(Modifier::BOLD);
            let mut lines: Vec<Line> = Vec::with_capacity(CELL_H as usize);
            for i in 0..CELL_H {
                let content = if i == CELL_H / 2 {
                    format!("  {}  ", glyph)
                } else {
                    "     ".to_string()
                };
                lines.push(Line::from(Span::styled(content, style)));
            }
            let cell_widget = Paragraph::new(lines).style(style);
            frame.render_widget(cell_widget, cell_area);
        }
    }
}

fn piece_glyph(square: Option<Piece>) -> (char, Color) {
    match square {
        None => (' ', Color::White),
        Some(Piece { kind, side }) => {
            let glyph = match (side, kind) {
                (Side::White, PieceKind::King)   => '\u{2654}',
                (Side::White, PieceKind::Queen)  => '\u{2655}',
                (Side::White, PieceKind::Rook)   => '\u{2656}',
                (Side::White, PieceKind::Bishop) => '\u{2657}',
                (Side::White, PieceKind::Knight) => '\u{2658}',
                (Side::White, PieceKind::Pawn)   => '\u{2659}',
                (Side::Black, PieceKind::King)   => '\u{265A}',
                (Side::Black, PieceKind::Queen)  => '\u{265B}',
                (Side::Black, PieceKind::Rook)   => '\u{265C}',
                (Side::Black, PieceKind::Bishop) => '\u{265D}',
                (Side::Black, PieceKind::Knight) => '\u{265E}',
                (Side::Black, PieceKind::Pawn)   => '\u{265F}',
            };
            let color = match side {
                Side::White => Color::White,
                Side::Black => Color::Black,
            };
            (glyph, color)
        }
    }
}
