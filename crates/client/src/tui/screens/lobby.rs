use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::tui::app::App;

pub fn render(frame: &mut Frame, app: &App) {
    let t = &app.translator;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(5),    // room list
            Constraint::Length(3), // footer
        ])
        .split(frame.area());

    // Header
    let header_text = format!("{} - {}", t.get("lobby.title"), app.username_input.as_str());
    let header = Paragraph::new(header_text)
        .alignment(Alignment::Center)
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(header, chunks[0]);

    // Room list (placeholder for Phase 3)
    let rooms_block = Block::default()
        .title(t.get("lobby.rooms"))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::White));
    let no_rooms = Paragraph::new(t.get("lobby.no_rooms"))
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray))
        .block(rooms_block);
    frame.render_widget(no_rooms, chunks[1]);

    // Footer
    let footer = Paragraph::new(Span::styled(
        "Esc: Quit | C: Create Room | R: Refresh",
        Style::default().fg(Color::DarkGray),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(footer, chunks[2]);
}
