pub mod checkers;
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

/// Renders a rematch overlay when a rematch request is pending or incoming.
pub fn render_rematch_overlay(frame: &mut Frame, app: &App) {
    if app.rematch_pending {
        render_modal(
            frame,
            " Rematch ",
            &["Rematch requested.", "", "Waiting for opponent..."],
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
