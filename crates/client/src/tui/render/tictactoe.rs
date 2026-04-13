use gc_shared::game::tictactoe::{Cell, TicTacToeState};
use gc_shared::types::GameOutcome;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::tui::app::App;

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
    } else if let Some(ref state) = app.game_state {
        let current = state.players[state.current_turn];
        if Some(current) == app.my_player_id {
            t.get("game.your_turn").to_string()
        } else {
            t.get("game.opponent_turn").to_string()
        }
    } else {
        "Waiting for game...".to_string()
    };

    let header_color = if app.game_over.is_some() {
        Color::Yellow
    } else if app
        .game_state
        .as_ref()
        .is_some_and(|s| Some(s.players[s.current_turn]) == app.my_player_id)
    {
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
    if let Some(ref state) = app.game_state {
        let board_area = centered_rect(30, 80, chunks[1]);
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

    // Each cell is 5 chars wide, 3 lines tall
    let row_constraints = [
        Constraint::Length(3),
        Constraint::Length(1), // separator
        Constraint::Length(3),
        Constraint::Length(1), // separator
        Constraint::Length(3),
    ];
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(inner);

    for board_row in 0..3u8 {
        let row_area = rows[board_row as usize * 2];
        let col_constraints = [
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
        ];
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(col_constraints)
            .split(row_area);

        for board_col in 0..3u8 {
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
            }

            let cell_widget = Paragraph::new(Line::from(Span::styled(symbol, style)))
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(if is_cursor && !is_game_over {
                            Borders::ALL
                        } else {
                            Borders::NONE
                        })
                        .border_style(Style::default().fg(Color::Yellow)),
                );

            frame.render_widget(cell_widget, cols[board_col as usize]);
        }

        // Draw separator line between rows
        if board_row < 2 {
            let sep_area = rows[board_row as usize * 2 + 1];
            let sep = Paragraph::new("─────┼─────┼─────")
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(sep, sep_area);
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
