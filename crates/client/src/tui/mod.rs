pub mod app;
pub mod event;
pub mod render;
pub mod screens;

use std::io;

use crossterm::event::{KeyCode, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use app::{App, GameMode, LoginField, LoginMode, Screen};
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
    let mut lobby_refresh_counter: u8 = 0;

    // Main loop
    while app.running {
        // Draw
        terminal.draw(|frame| match app.screen {
            Screen::Login => screens::login::render(frame, &app),
            Screen::Lobby => screens::lobby::render(frame, &app),
            Screen::Room => screens::room::render(frame, &app),
            Screen::InGame => render::tictactoe::render(frame, &app),
        })?;

        // Process network events (non-blocking)
        while let Some(event) = net.try_recv() {
            handle_net_event(&mut app, event, &net);
        }

        // Handle input events
        if let Ok(event) = events.recv() {
            match event {
                Event::Key(key) => handle_key(&mut app, key.code, key.modifiers, &net),
                Event::Tick => {
                    // Auto-refresh lobby room list every ~2 seconds (40 ticks * 50ms)
                    if app.screen == Screen::Lobby && app.authenticated {
                        lobby_refresh_counter += 1;
                        if lobby_refresh_counter >= 40 {
                            lobby_refresh_counter = 0;
                            let _ = net.send(NetCommand::ListRooms);
                        }
                    } else {
                        lobby_refresh_counter = 0;
                    }
                }
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
            player_id,
        }) => {
            app.my_player_id = Some(player_id);
            let _ = app.db.save_token(&token, expires_at);
            app.on_auth_success(token);
            let _ = net.send(NetCommand::ListRooms);
        }
        NetEvent::RoomList(rooms) => {
            app.update_rooms(rooms);
        }
        NetEvent::RoomJoined {
            room_id,
            players,
            state: _room_state,
        } => {
            app.on_room_joined(room_id, players);
        }
        NetEvent::GameStateUpdate { state_data } => {
            app.on_game_state(&state_data);
        }
        NetEvent::GameOver { outcome } => {
            app.on_game_over(outcome);
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
            Screen::InGame if app.game_mode != GameMode::Online => {
                app.leave_solo_game();
                return;
            }
            Screen::Room | Screen::InGame => {
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
        Screen::InGame => handle_game_key(app, code, net),
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

    // Sub-state: difficulty selection
    if app.selecting_difficulty {
        match code {
            KeyCode::Char('e') | KeyCode::Char('E') => {
                app.selecting_difficulty = false;
                app.start_solo_game(gc_shared::types::Difficulty::Easy);
            }
            KeyCode::Char('h') | KeyCode::Char('H') => {
                app.selecting_difficulty = false;
                app.start_solo_game(gc_shared::types::Difficulty::Hard);
            }
            KeyCode::Esc | KeyCode::Char('b') | KeyCode::Char('B') => {
                app.selecting_difficulty = false;
            }
            _ => {}
        }
        return;
    }

    match code {
        KeyCode::Char('q') => app.quit(),
        KeyCode::Char('r') | KeyCode::Char('R') => {
            let _ = net.send(NetCommand::ListRooms);
        }
        KeyCode::Char('c') | KeyCode::Char('C') => {
            let settings = gc_shared::types::GameSettings::default();
            app.current_game_type = gc_shared::types::GameType::TicTacToe;
            app.current_max_players = settings.max_players;
            let _ = net.send(NetCommand::CreateRoom {
                game_type: gc_shared::types::GameType::TicTacToe,
                settings,
            });
        }
        KeyCode::Char('b') | KeyCode::Char('B') => {
            app.selecting_difficulty = true;
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
                app.current_game_type = room.game_type;
                app.current_max_players = room.max_players;
                let _ = net.send(NetCommand::JoinRoom { room_id: room.id });
            }
        }
        _ => {}
    }
}

fn handle_game_key(app: &mut App, code: KeyCode, net: &NetworkClient) {
    // When game is over, only allow rematch or leave
    if app.game_over.is_some() {
        match code {
            KeyCode::Char('r') | KeyCode::Char('R') => {
                if matches!(app.game_mode, GameMode::Solo { .. }) {
                    app.rematch_solo();
                }
                // Online rematch: TODO in later phase
            }
            _ => {} // Esc is handled in handle_key before this function
        }
        return;
    }

    let is_our_turn = app
        .game_state
        .as_ref()
        .is_some_and(|s| Some(s.players[s.current_turn]) == app.my_player_id);

    match code {
        KeyCode::Up => {
            if app.cursor_row > 0 {
                app.cursor_row -= 1;
            }
        }
        KeyCode::Down => {
            if app.cursor_row < 2 {
                app.cursor_row += 1;
            }
        }
        KeyCode::Left => {
            if app.cursor_col > 0 {
                app.cursor_col -= 1;
            }
        }
        KeyCode::Right => {
            if app.cursor_col < 2 {
                app.cursor_col += 1;
            }
        }
        KeyCode::Enter => {
            if is_our_turn {
                match &app.game_mode {
                    GameMode::Solo { .. } => {
                        app.play_solo_move(app.cursor_row, app.cursor_col);
                    }
                    GameMode::Online => {
                        let mv = gc_shared::game::tictactoe::TicTacToeMove {
                            row: app.cursor_row,
                            col: app.cursor_col,
                        };
                        if let Ok(data) = gc_shared::protocol::codec::encode(&mv) {
                            let _ = net.send(NetCommand::GameAction { data });
                        }
                    }
                }
            }
        }
        _ => {}
    }
}
