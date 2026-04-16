use gc_shared::game::connect4::{COLS, Cell, Connect4State, ROWS};
use gc_shared::types::GameOutcome;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Clear};

use crate::tui::app::{App, ClientGameState};

// Each cell is CELL_W chars wide and CELL_H lines tall
const CELL_W: u16 = 7;
const CELL_H: u16 = 3;

pub fn render(frame: &mut Frame, app: &App) {
    let t = &app.translator;

    let board_height = (ROWS as u16) * CELL_H + 2;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),           // header
            Constraint::Length(2),           // column selector
            Constraint::Min(board_height),   // board
            Constraint::Length(3),           // footer
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

    // Board area — fixed size, centered
    let board_area = fixed_board_rect(chunks[2]);

    // Column selector arrow
    if app.game_over.is_none() {
        render_column_selector(frame, app.cursor_col, board_area, chunks[1]);
    }

    // Board
    if let Some(ClientGameState::Connect4(ref state)) = app.game_state {
        render_board(frame, state, app.cursor_col, app, board_area);
    }

    // Footer
    let footer_text = if app.rematch_pending {
        format!("Waiting for opponent... | Esc: {}", t.get("game.leave"))
    } else if app.rematch_incoming {
        format!("Y: Accept rematch | N: Decline | Esc: {}", t.get("game.leave"))
    } else if app.game_over.is_some() {
        format!(
            "R: {} | Esc: {} | I: Help",
            t.get("game.rematch"),
            t.get("game.leave")
        )
    } else {
        format!(
            "Left/Right: move | Enter: drop | Esc: {} | I: Help",
            t.get("game.leave")
        )
    };
    let footer = Paragraph::new(footer_text)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, chunks[3]);

    if app.show_help {
        render_help_modal(frame, app);
    }

    if app.rematch_pending || app.rematch_incoming {
        super::render_rematch_overlay(frame, app);
    }
}

fn fixed_board_rect(area: Rect) -> Rect {
    let board_width = (COLS as u16) * CELL_W + 2;
    let board_height = (ROWS as u16) * CELL_H + 2;
    let x = area.x + area.width.saturating_sub(board_width) / 2;
    let y = area.y + area.height.saturating_sub(board_height) / 2;
    Rect::new(x, y, board_width.min(area.width), board_height.min(area.height))
}

fn render_column_selector(frame: &mut Frame, cursor_col: u8, board_area: Rect, area: Rect) {
    // Align arrows with board columns (skip 1-char left border)
    let mut spans = Vec::new();
    for c in 0..COLS as u8 {
        // Fill entire column width with highlight so the selected column is unmissable
        let (text, style) = if c == cursor_col {
            (
                " ▼▼▼▼▼ ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            ("       ", Style::default())
        };
        spans.push(Span::styled(text, style));
    }

    let selector = Paragraph::new(Line::from(spans));
    let selector_area = Rect {
        x: board_area.x + 1,
        y: area.y,
        width: (COLS as u16) * CELL_W,
        height: area.height,
    };
    frame.render_widget(selector, selector_area);
}

fn render_board(frame: &mut Frame, state: &Connect4State, cursor_col: u8, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Connect 4 ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue))
        .style(Style::default().bg(Color::Blue));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let row_constraints: Vec<Constraint> = (0..ROWS).map(|_| Constraint::Length(CELL_H)).collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(inner);

    for display_row in 0..ROWS {
        let board_row = ROWS - 1 - display_row;
        let row_area = rows[display_row];

        let col_constraints: Vec<Constraint> =
            (0..COLS).map(|_| Constraint::Length(CELL_W)).collect();
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(col_constraints)
            .split(row_area);

        for board_col in 0..COLS {
            let cell = state.board[board_row][board_col];
            let is_cursor_col = board_col == cursor_col as usize;
            let is_game_over = app.game_over.is_some();
            let cell_area = cols[board_col];

            // 3-line circle rendering per cell
            let lines: Vec<Line> = match cell {
                Cell::Red => {
                    let s = Style::default().fg(Color::Red).bg(Color::Blue);
                    vec![
                        Line::from(Span::styled(" ╭───╮ ", s)),
                        Line::from(Span::styled(" │███│ ", s)),
                        Line::from(Span::styled(" ╰───╯ ", s)),
                    ]
                }
                Cell::Yellow => {
                    let s = Style::default().fg(Color::Rgb(255, 140, 0)).bg(Color::Blue);
                    vec![
                        Line::from(Span::styled(" ╭───╮ ", s)),
                        Line::from(Span::styled(" │███│ ", s)),
                        Line::from(Span::styled(" ╰───╯ ", s)),
                    ]
                }
                Cell::Empty => {
                    let fg = if is_cursor_col && !is_game_over {
                        Color::Yellow
                    } else {
                        Color::White
                    };
                    let s = Style::default().fg(fg).bg(Color::Blue);
                    vec![
                        Line::from(Span::styled(" ╭───╮ ", s)),
                        Line::from(Span::styled(" │   │ ", s)),
                        Line::from(Span::styled(" ╰───╯ ", s)),
                    ]
                }
            };

            let cell_widget = Paragraph::new(lines).style(Style::default().bg(Color::Blue));
            frame.render_widget(cell_widget, cell_area);
        }
    }
}

fn render_help_modal(frame: &mut Frame, app: &App) {
    let area = centered_rect(60, 60, frame.area());
    frame.render_widget(Clear, area);

    let (title, rules) = match &app.game_state {
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
                "- I: Close help",
                "- Esc: Leave game",
            ],
        ),
        _ => (
            "Tic-Tac-Toe Rules",
            vec![
                "Goal: Connect 3 of your marks in a row.",
                "Line can be horizontal, vertical, or diagonal.",
                "",
                "Controls:",
                "- Arrow Keys: Move cursor",
                "- Enter: Place mark",
                "- I: Close help",
                "- Esc: Leave game",
            ],
        ),
    };

    let help_text: Vec<Line> = rules.into_iter().map(|l| Line::from(l)).collect();
    
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let paragraph = Paragraph::new(help_text)
        .block(block)
        .alignment(Alignment::Left);

    frame.render_widget(paragraph, area);
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

