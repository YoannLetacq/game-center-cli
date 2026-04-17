use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::tui::app::App;

pub fn render(frame: &mut Frame, app: &App) {
    let t = &app.translator;

    let has_error = app.status_error.is_some();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),                             // header
            Constraint::Length(if has_error { 1 } else { 0 }), // error bar
            Constraint::Min(5),                                // room list
            Constraint::Length(3),                             // footer
        ])
        .split(frame.area());

    // Header
    let header_text = format!(
        "{} — {} | Game: {}",
        t.get("lobby.title"),
        app.username_input.as_str(),
        app.selected_game_type,
    );
    let header = Paragraph::new(header_text)
        .alignment(Alignment::Center)
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(header, chunks[0]);

    // Error bar
    if let Some(ref err) = app.status_error {
        let error = Paragraph::new(err.as_str())
            .style(Style::default().fg(Color::Red))
            .alignment(Alignment::Center);
        frame.render_widget(error, chunks[1]);
    }

    // Room list
    if app.rooms.is_empty() {
        let no_rooms = Paragraph::new(t.get("lobby.no_rooms"))
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray))
            .block(
                Block::default()
                    .title(t.get("lobby.rooms"))
                    .borders(Borders::ALL),
            );
        frame.render_widget(no_rooms, chunks[2]);
    } else {
        let items: Vec<ListItem> = app
            .rooms
            .iter()
            .enumerate()
            .map(|(i, room)| {
                let selected = i == app.selected_room;
                let prefix = if selected { "▸ " } else { "  " };
                let state_str = match &room.state {
                    gc_shared::types::RoomState::Waiting => "waiting",
                    gc_shared::types::RoomState::InProgress => "in progress",
                    gc_shared::types::RoomState::Finished => "finished",
                };
                let text = format!(
                    "{}{} — {} ({}/{}) [{}]",
                    prefix,
                    room.game_type,
                    room.host_name,
                    room.player_count,
                    room.max_players,
                    state_str,
                );
                let style = if selected {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(text).style(style)
            })
            .collect();

        let room_list = List::new(items).block(
            Block::default()
                .title(format!("{} ({})", t.get("lobby.rooms"), app.rooms.len()))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::White)),
        );
        frame.render_widget(room_list, chunks[2]);
    }

    // Footer
    let footer = if app.selecting_solo_game || app.selecting_multiplayer_game {
        Paragraph::new(Line::from(vec![
            Span::styled(
                "T",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(": TicTacToe | "),
            Span::styled(
                "C",
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(": Connect4 | "),
            Span::styled(
                "K",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(": Checkers | "),
            Span::styled("Esc", Style::default().fg(Color::Yellow)),
            Span::raw(": Cancel"),
        ]))
    } else if app.selecting_difficulty {
        Paragraph::new(Line::from(vec![
            Span::styled("E", Style::default().fg(Color::Green)),
            Span::raw(": Easy  "),
            Span::styled("H", Style::default().fg(Color::Red)),
            Span::raw(": Hard  "),
            Span::styled("Esc", Style::default().fg(Color::DarkGray)),
            Span::raw(": Cancel"),
        ]))
    } else {
        Paragraph::new(Line::from(vec![
            Span::styled("G", Style::default().fg(Color::Cyan)),
            Span::raw(": Game  "),
            Span::styled("B", Style::default().fg(Color::Magenta)),
            Span::raw(": vs Bot  "),
            Span::styled("C", Style::default().fg(Color::Green)),
            Span::raw(": "),
            Span::raw(t.get("lobby.create_room")),
            Span::raw("  "),
            Span::styled("Enter", Style::default().fg(Color::Green)),
            Span::raw(": "),
            Span::raw(t.get("lobby.join_room")),
            Span::raw("  "),
            Span::styled("R", Style::default().fg(Color::Yellow)),
            Span::raw(": "),
            Span::raw(t.get("lobby.refresh")),
            Span::raw("  "),
            Span::styled("Esc", Style::default().fg(Color::Red)),
            Span::raw(": "),
            Span::raw(t.get("app.quit")),
        ]))
    }
    .alignment(Alignment::Center);
    frame.render_widget(footer, chunks[3]);
}
