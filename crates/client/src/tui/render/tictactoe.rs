use gc_shared::game::tictactoe::{Cell, TicTacToeState};
use gc_shared::types::GameOutcome;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::tui::app::{App, ClientGameState};

pub fn render(frame: &mut Frame, app: &App) {
    let t = &app.translator;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(12),   // board
            Constraint::Length(3), // footer
        ])
        .split(frame.area());

    // Header — turn indicator
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

    // Board
    if let Some(ClientGameState::TicTacToe(ref state)) = app.game_state {
        let board_area = centered_rect(40, 80, chunks[1]);
        render_board(
            frame,
            state,
            app.cursor_row,
            app.cursor_col,
            app,
            board_area,
        );
    }

    // Footer
    let footer_text = if app.game_over.is_some() {
        format!(
            "R: {} | Esc: {}",
            t.get("game.rematch"),
            t.get("game.leave")
        )
    } else {
        format!(
            "Arrow keys: move | Enter: place | Esc: {}",
            t.get("game.leave")
        )
    };
    let footer = Paragraph::new(footer_text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, chunks[2]);
}

fn render_board(
    frame: &mut Frame,
    state: &TicTacToeState,
    cursor_row: u8,
    cursor_col: u8,
    app: &App,
    area: Rect,
) {
    // Determine which symbol we are
    let my_symbol = app
        .my_player_id
        .and_then(|pid| {
            if state.players[0] == pid {
                Some(Cell::X)
            } else if state.players[1] == pid {
                Some(Cell::O)
            } else {
                None
            }
        })
        .unwrap_or(Cell::X);

    let block = Block::default()
        .title("Tic-Tac-Toe")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let row_constraints = [
        Constraint::Ratio(1, 3),
        Constraint::Length(1), // separator
        Constraint::Ratio(1, 3),
        Constraint::Length(1), // separator
        Constraint::Ratio(1, 3),
    ];
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(inner);

    for display_row in 0..5usize {
        let row_area = rows[display_row];
        let col_constraints = [
            Constraint::Ratio(1, 3),
            Constraint::Length(1),
            Constraint::Ratio(1, 3),
            Constraint::Length(1),
            Constraint::Ratio(1, 3),
        ];
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(col_constraints)
            .split(row_area);

        if display_row % 2 == 1 {
            // Horizontal Separator row
            for c in 0..5usize {
                let sep = if c % 2 == 1 {
                    Paragraph::new("┼")
                } else {
                    Paragraph::new("─".repeat(cols[c].width as usize))
                };
                let sep = sep.alignment(Alignment::Center)
                    .style(Style::default().fg(Color::DarkGray));
                frame.render_widget(sep, cols[c]);
            }
        } else {
            // Cell row
            let board_row = (display_row / 2) as u8;
            for c in 0..5usize {
                if c % 2 == 1 {
                    // Vertical Separator
                    let height = cols[c].height as usize;
                    let mut v_line = String::new();
                    for _ in 0..height {
                        v_line.push_str("│\n");
                    }
                    let sep = Paragraph::new(v_line.trim_end().to_string())
                        .alignment(Alignment::Center)
                        .style(Style::default().fg(Color::DarkGray));
                    frame.render_widget(sep, cols[c]);
                } else {
                    // Cell
                    let board_col = (c / 2) as u8;
                    let cell = state.board[board_row as usize][board_col as usize];
                    let is_cursor = board_row == cursor_row && board_col == cursor_col;
                    let is_game_over = app.game_over.is_some();

                    let (symbol, color) = match cell {
                        Cell::X => (
                            "X",
                            if cell == my_symbol {
                                Color::Cyan
                            } else {
                                Color::Red
                            },
                        ),
                        Cell::O => (
                            "O",
                            if cell == my_symbol {
                                Color::Cyan
                            } else {
                                Color::Red
                            },
                        ),
                        Cell::Empty => {
                            if is_cursor && !is_game_over {
                                ("·", Color::Yellow)
                            } else {
                                (" ", Color::DarkGray)
                            }
                        }
                    };

                    let mut style = Style::default().fg(color);
                    if is_cursor && !is_game_over {
                        style = style.add_modifier(Modifier::BOLD);
                        style = style.bg(Color::Rgb(50, 50, 50));
                    }

                    let height = cols[c].height as usize;
                    let pad = if height > 1 { (height - 1) / 2 } else { 0 };
                    let mut content = String::new();
                    for _ in 0..pad {
                        content.push('\n');
                    }
                    content.push_str(symbol);

                    let cell_widget = Paragraph::new(content)
                        .alignment(Alignment::Center)
                        .block(Block::default().style(style));

                    frame.render_widget(cell_widget, cols[c]);
                }
            }
        }
    }
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
