use gc_shared::game::connect4::{COLS, Cell, Connect4State, ROWS};
use gc_shared::types::GameOutcome;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::tui::app::{App, ClientGameState};

pub fn render(frame: &mut Frame, app: &App) {
    let t = &app.translator;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Length(2), // column selector
            Constraint::Min(14),   // board
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

    // Column selector arrow
    if app.game_over.is_none() {
        let board_area = centered_rect(60, 100, chunks[2]);
        render_column_selector(
            frame,
            app.cursor_col,
            board_area.x,
            board_area.width,
            chunks[1],
        );
    }

    // Board
    if let Some(ClientGameState::Connect4(ref state)) = app.game_state {
        let board_area = centered_rect(60, 100, chunks[2]);
        render_board(frame, state, app.cursor_col, app, board_area);
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
            "Left/Right: move | Enter: drop | Esc: {}",
            t.get("game.leave")
        )
    };
    let footer = Paragraph::new(footer_text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, chunks[3]);
}

fn render_column_selector(
    frame: &mut Frame,
    cursor_col: u8,
    board_x: u16,
    board_width: u16,
    area: Rect,
) {
    let col_width = board_width / COLS as u16;
    let mut spans = Vec::new();

    for c in 0..COLS as u8 {
        let text = if c == cursor_col { " v " } else { "   " };
        let style = if c == cursor_col {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        spans.push(Span::styled(text, style));
        // Add padding between columns
        if (c as usize) < COLS - 1 {
            let pad = col_width.saturating_sub(3);
            spans.push(Span::raw(" ".repeat(pad as usize)));
        }
    }

    // Center the selector by offsetting from board_x
    let selector = Paragraph::new(Line::from(spans)).alignment(Alignment::Center);

    let selector_area = Rect {
        x: board_x,
        y: area.y,
        width: board_width,
        height: area.height,
    };
    frame.render_widget(selector, selector_area);
}

fn render_board(frame: &mut Frame, state: &Connect4State, cursor_col: u8, app: &App, area: Rect) {
    // Determine which color we are
    let my_cell = app
        .my_player_id
        .and_then(|pid| {
            if state.players[0] == pid {
                Some(Cell::Red)
            } else if state.players[1] == pid {
                Some(Cell::Yellow)
            } else {
                None
            }
        })
        .unwrap_or(Cell::Red);

    let block = Block::default()
        .title("Connect 4")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue))
        .style(Style::default().bg(Color::Blue));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Each row is 2 lines tall (symbol + gap)
    let mut row_constraints: Vec<Constraint> = Vec::new();
    for i in 0..ROWS {
        row_constraints.push(Constraint::Length(2));
        if i < ROWS - 1 {
            row_constraints.push(Constraint::Length(0)); // no separator needed
        }
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(inner);

    // Render rows top-to-bottom (board row 5 = top, row 0 = bottom)
    for display_row in 0..ROWS {
        let board_row = ROWS - 1 - display_row;
        let row_area = rows[display_row * 2];

        let col_constraints: Vec<Constraint> = (0..COLS)
            .map(|_| Constraint::Ratio(1, COLS as u32))
            .collect();
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(col_constraints)
            .split(row_area);

        for board_col in 0..COLS {
            let cell = state.board[board_row][board_col];
            let is_cursor_col = board_col == cursor_col as usize;
            let is_game_over = app.game_over.is_some();

            let (symbol, color) = match cell {
                Cell::Red => (
                    "\u{25cf}", // filled circle ●
                    if cell == my_cell {
                        Color::LightRed
                    } else {
                        Color::Red
                    },
                ),
                Cell::Yellow => (
                    "\u{25cf}",
                    if cell == my_cell {
                        Color::LightYellow
                    } else {
                        Color::Yellow
                    },
                ),
                Cell::Empty => {
                    if is_cursor_col && !is_game_over {
                        ("\u{25cf}", Color::Rgb(50, 50, 50)) // highlight column slightly
                    } else {
                        ("\u{25cf}", Color::Black)
                    }
                }
            };

            let style = Style::default().fg(color).bg(Color::Blue);

            let cell_widget = Paragraph::new(Line::from(Span::styled(symbol, style)))
                .alignment(Alignment::Center);

            frame.render_widget(cell_widget, cols[board_col]);
        }
    }

    // Column numbers at the bottom
    if inner.height > ROWS as u16 * 2 {
        let numbers_area = Rect {
            x: inner.x,
            y: inner.y + inner.height - 1,
            width: inner.width,
            height: 1,
        };
        let col_nums: Vec<Span> = (1..=COLS)
            .map(|c| {
                let style = if c - 1 == cursor_col as usize {
                    Style::default()
                        .fg(Color::Yellow)
                        .bg(Color::Blue)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Black).bg(Color::Blue)
                };
                Span::styled(format!("  {}  ", c), style)
            })
            .collect();
        let numbers = Paragraph::new(Line::from(col_nums)).alignment(Alignment::Center);
        frame.render_widget(numbers, numbers_area);
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
