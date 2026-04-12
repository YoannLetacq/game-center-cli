use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::tui::app::App;

pub fn render(frame: &mut Frame, app: &App) {
    let t = &app.translator;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(5),    // player list
            Constraint::Length(3), // footer
        ])
        .split(frame.area());

    // Header — room info
    let room_title = match app.current_room_id {
        Some(id) => format!("Room {}", &id.to_string()[..8]),
        None => "Room".to_string(),
    };
    let header = Paragraph::new(room_title)
        .alignment(Alignment::Center)
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(header, chunks[0]);

    // Player list
    let items: Vec<ListItem> = app
        .current_room_players
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let marker = if i == 0 { " (host)" } else { "" };
            ListItem::new(format!("  {} {}{}", "●", p.username, marker))
                .style(Style::default().fg(Color::Green))
        })
        .collect();

    let waiting = format!(
        "{} ({}/{})",
        t.get("lobby.rooms"),
        app.current_room_players.len(),
        2 // default max for now
    );

    let player_list = List::new(items).block(
        Block::default()
            .title(waiting)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::White)),
    );
    frame.render_widget(player_list, chunks[1]);

    // Footer
    let footer = Paragraph::new(Line::from(vec![
        Span::styled("Esc", Style::default().fg(Color::Red)),
        Span::raw(": "),
        Span::raw(t.get("game.leave")),
        Span::raw("  |  Waiting for players..."),
    ]))
    .alignment(Alignment::Center);
    frame.render_widget(footer, chunks[2]);
}
