pub mod app;
pub mod event;
pub mod screens;

use std::io;

use crossterm::event::{KeyCode, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use app::{App, LoginField, LoginMode, Screen};
use event::{Event, EventHandler};

use crate::net::client::{NetCommand, NetEvent, NetworkClient};

/// Run the TUI application loop.
pub fn run(mut app: App) -> io::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let events = EventHandler::new(50);
    let net = NetworkClient::new();

    // Main loop
    while app.running {
        // Draw
        terminal.draw(|frame| match app.screen {
            Screen::Login => screens::login::render(frame, &app),
            Screen::Lobby => screens::lobby::render(frame, &app),
            Screen::Room => screens::room::render(frame, &app),
        })?;

        // Process network events (non-blocking)
        while let Some(event) = net.try_recv() {
            handle_net_event(&mut app, event, &net);
        }

        // Handle input events
        if let Ok(event) = events.recv() {
            match event {
                Event::Key(key) => handle_key(&mut app, key.code, key.modifiers, &net),
                Event::Tick => {}
            }
        }
    }

    // Disconnect before quitting
    let _ = net.send(NetCommand::Disconnect);

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

fn handle_net_event(app: &mut App, event: NetEvent, net: &NetworkClient) {
    match event {
        NetEvent::Connected => {
            let _ = net.send(NetCommand::ListRooms);
        }
        NetEvent::AuthResult(gc_shared::protocol::messages::ServerMsg::AuthOk {
            token,
            expires_at,
        }) => {
            let _ = app.db.save_token(&token, expires_at);
            app.on_auth_success(token);
            let _ = net.send(NetCommand::ListRooms);
        }
        NetEvent::RoomList(rooms) => {
            app.update_rooms(rooms);
        }
        NetEvent::RoomJoined { room_id, players } => {
            app.on_room_joined(room_id, players);
        }
        NetEvent::PlayerJoined(player) => {
            app.on_player_joined(player);
        }
        NetEvent::PlayerLeft(player_id) => {
            app.on_player_left(player_id);
        }
        NetEvent::Error(msg) => {
            // Route error to the right screen
            match app.screen {
                Screen::Login => app.on_auth_failure(msg),
                _ => app.status_error = Some(msg),
            }
        }
        NetEvent::Disconnected => {
            app.authenticated = false;
            app.auth_token = None;
            app.screen = Screen::Login;
            app.login_error = Some("Disconnected from server".to_string());
        }
        _ => {}
    }
}

fn handle_key(app: &mut App, code: KeyCode, modifiers: KeyModifiers, net: &NetworkClient) {
    if code == KeyCode::Esc {
        match app.screen {
            Screen::Room => {
                let _ = net.send(NetCommand::LeaveRoom);
                app.on_room_left();
                let _ = net.send(NetCommand::ListRooms);
                return;
            }
            _ => {
                app.quit();
                return;
            }
        }
    }
    if code == KeyCode::Char('c') && modifiers == KeyModifiers::CONTROL {
        app.quit();
        return;
    }

    match app.screen {
        Screen::Login => handle_login_key(app, code, net),
        Screen::Lobby => handle_lobby_key(app, code, net),
        Screen::Room => {}
    }
}

fn handle_login_key(app: &mut App, code: KeyCode, net: &NetworkClient) {
    if app.login_loading {
        return;
    }

    match code {
        KeyCode::Tab => app.toggle_field(),
        KeyCode::F(2) => app.toggle_login_mode(),
        KeyCode::Enter => {
            if let Some(err) = app.validate_login_form() {
                app.login_error = Some(err);
            } else {
                app.login_loading = true;
                app.login_error = None;

                let cmd = match app.login_mode {
                    LoginMode::Login => NetCommand::Login {
                        server_url: app.server_url.clone(),
                        username: app.username_input.clone(),
                        password: app.password_input.clone(),
                    },
                    LoginMode::Register => NetCommand::Register {
                        server_url: app.server_url.clone(),
                        username: app.username_input.clone(),
                        password: app.password_input.clone(),
                    },
                };

                if let Err(e) = net.send(cmd) {
                    app.on_auth_failure(e);
                }
            }
        }
        KeyCode::Backspace => match app.active_field {
            LoginField::Username => {
                app.username_input.pop();
            }
            LoginField::Password => {
                app.password_input.pop();
            }
        },
        KeyCode::Char(c) => match app.active_field {
            LoginField::Username => app.username_input.push(c),
            LoginField::Password => app.password_input.push(c),
        },
        _ => {}
    }
}

fn handle_lobby_key(app: &mut App, code: KeyCode, net: &NetworkClient) {
    app.status_error = None; // Clear error on any key press
    match code {
        KeyCode::Char('q') => app.quit(),
        KeyCode::Char('r') | KeyCode::Char('R') => {
            let _ = net.send(NetCommand::ListRooms);
        }
        KeyCode::Char('c') | KeyCode::Char('C') => {
            let _ = net.send(NetCommand::CreateRoom {
                game_type: gc_shared::types::GameType::TicTacToe,
                settings: gc_shared::types::GameSettings::default(),
            });
        }
        KeyCode::Up => {
            if app.selected_room > 0 {
                app.selected_room -= 1;
            }
        }
        KeyCode::Down => {
            if app.selected_room + 1 < app.rooms.len() {
                app.selected_room += 1;
            }
        }
        KeyCode::Enter => {
            if let Some(room) = app.rooms.get(app.selected_room) {
                let _ = net.send(NetCommand::JoinRoom { room_id: room.id });
            }
        }
        _ => {}
    }
}
