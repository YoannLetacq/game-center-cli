// Key handlers use `KeyCode::X => { if cond { ... } }` patterns. Clippy's
// `collapsible_match` would prefer match guards, but the `match code { ... }`
// shape with explicit per-key blocks is intentional and matches every other
// game handler. Silence the lint at file scope.
#![allow(clippy::collapsible_match)]

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

    // 30 ms tick = ~33 Hz, the minimum needed by Block Breaker physics.
    // Other games (Snake at 100 ms, turn-based) gate on their own elapsed
    // time so they remain unaffected.
    let events = EventHandler::new(30);
    let net = NetworkClient::new();
    let mut lobby_refresh_counter: u8 = 0;

    // Main loop
    while app.running {
        // Draw
        terminal.draw(|frame| {
            let area = frame.area();
            match render::terminal_fit::check_fit(area) {
                render::terminal_fit::TerminalFit::TooSmall => {
                    render::terminal_fit::render_too_small(frame, area);
                    return;
                }
                render::terminal_fit::TerminalFit::Edge => {
                    render::terminal_fit::render_edge_icon(frame, area);
                    return;
                }
                render::terminal_fit::TerminalFit::Ok => {}
            }
            match app.screen {
                Screen::Login => screens::login::render(frame, &app),
                Screen::Lobby => screens::lobby::render(frame, &app),
                Screen::Room => screens::room::render(frame, &app),
                Screen::InGame => match &app.game_state {
                    Some(app::ClientGameState::Connect4(_)) => {
                        render::connect4::render(frame, &app)
                    }
                    Some(app::ClientGameState::Checkers(_)) => {
                        render::checkers::render(frame, &app)
                    }
                    Some(app::ClientGameState::Chess(_)) => render::chess::render(frame, &app),
                    Some(app::ClientGameState::Snake(_)) => render::snake::render(frame, &app),
                    Some(app::ClientGameState::BlockBreaker(_)) => {
                        render::blockbreaker::render(frame, &app)
                    }
                    _ => render::tictactoe::render(frame, &app),
                },
            }
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
                    // Auto-refresh lobby room list every ~2 seconds (67 ticks * 30ms)
                    if app.screen == Screen::Lobby && app.authenticated {
                        lobby_refresh_counter = lobby_refresh_counter.saturating_add(1);
                        if lobby_refresh_counter >= 67 {
                            lobby_refresh_counter = 0;
                            let _ = net.send(NetCommand::ListRooms);
                        }
                    } else {
                        lobby_refresh_counter = 0;
                    }
                }
            }
            // Drive realtime solo games unconditionally on every loop
            // iteration so heavy key-repeat traffic can't starve physics.
            // Each tick method gates internally on its own elapsed-ms
            // threshold, so calling it on key events too is cheap.
            if app.screen == Screen::InGame {
                app.solo_snake_tick();
                app.solo_blockbreaker_tick();
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
        NetEvent::GameDelta {
            tick: _,
            delta_data,
        } => {
            // Only Snake produces deltas today; decoder routes by active state.
            app.on_snake_delta(&delta_data);
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
        NetEvent::RematchRequested => {
            app.rematch_incoming = true;
        }
        NetEvent::RematchAccepted => {
            app.on_rematch_accepted();
        }
        NetEvent::RematchDeclined => {
            app.on_rematch_declined();
            let _ = net.send(NetCommand::ListRooms);
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
        if app.show_help {
            app.show_help = false;
            return;
        }
        match app.screen {
            Screen::InGame
                if matches!(&app.game_state, Some(app::ClientGameState::Checkers(_)))
                    && app.checkers_input.stage != app::CheckersInputStage::Idle =>
            {
                app.checkers_cancel_selection();
                return;
            }
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

    if app.screen == Screen::InGame && (code == KeyCode::Char('i') || code == KeyCode::Char('I')) {
        app.show_help = !app.show_help;
        return;
    }

    match app.screen {
        Screen::Login => handle_login_key(app, code, net),
        Screen::Lobby => handle_lobby_key(app, code, net),
        Screen::Room => {}
        Screen::InGame => {
            if !app.show_help {
                handle_game_key(app, code, net);
            }
        }
    }
}

fn handle_login_key(app: &mut App, code: KeyCode, net: &NetworkClient) {
    if app.login_loading {
        return;
    }

    // Sub-state: solo game selection on login screen
    if app.selecting_solo_game {
        match code {
            KeyCode::Char('t') | KeyCode::Char('T') => {
                app.selected_game_type = gc_shared::types::GameType::TicTacToe;
                app.selecting_solo_game = false;
                app.selecting_difficulty = true;
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                app.selected_game_type = gc_shared::types::GameType::Connect4;
                app.selecting_solo_game = false;
                app.selecting_difficulty = true;
            }
            KeyCode::Char('k') | KeyCode::Char('K') => {
                app.selected_game_type = gc_shared::types::GameType::Checkers;
                app.selecting_solo_game = false;
                app.selecting_difficulty = true;
            }
            KeyCode::Char('h') | KeyCode::Char('H') => {
                app.selected_game_type = gc_shared::types::GameType::Chess;
                app.selecting_solo_game = false;
                app.selecting_difficulty = true;
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                app.selected_game_type = gc_shared::types::GameType::Snake;
                app.selecting_solo_game = false;
                app.selecting_difficulty = true;
            }
            KeyCode::Char('l') | KeyCode::Char('L') => {
                app.selected_game_type = gc_shared::types::GameType::BlockBreaker;
                app.selecting_solo_game = false;
                app.selecting_difficulty = true;
            }
            KeyCode::Esc | KeyCode::Char('b') | KeyCode::Char('B') => {
                app.selecting_solo_game = false;
            }
            _ => {}
        }
        return;
    }

    // Sub-state: solo difficulty selection on login screen
    if app.selecting_difficulty {
        let is_bb = app.selected_game_type == gc_shared::types::GameType::BlockBreaker;
        match code {
            KeyCode::Char('e') | KeyCode::Char('E') => {
                app.selecting_difficulty = false;
                if is_bb {
                    app.bb_difficulty = gc_shared::game::blockbreaker::BBDifficulty::Easy;
                }
                app.start_solo_game(gc_shared::types::Difficulty::Easy);
            }
            KeyCode::Char('h') | KeyCode::Char('H') => {
                app.selecting_difficulty = false;
                if is_bb {
                    app.bb_difficulty = gc_shared::game::blockbreaker::BBDifficulty::Hard;
                }
                app.start_solo_game(gc_shared::types::Difficulty::Hard);
            }
            KeyCode::Char('x') | KeyCode::Char('X') if is_bb => {
                app.selecting_difficulty = false;
                app.bb_difficulty = gc_shared::game::blockbreaker::BBDifficulty::Hardcore;
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
        KeyCode::Tab => app.toggle_field(),
        KeyCode::F(2) => app.toggle_login_mode(),
        KeyCode::F(3) => {
            app.selecting_solo_game = true;
        }
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

    // Sub-state: solo game selection on lobby screen
    if app.selecting_solo_game {
        match code {
            KeyCode::Char('t') | KeyCode::Char('T') => {
                app.selected_game_type = gc_shared::types::GameType::TicTacToe;
                app.selecting_solo_game = false;
                app.selecting_difficulty = true;
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                app.selected_game_type = gc_shared::types::GameType::Connect4;
                app.selecting_solo_game = false;
                app.selecting_difficulty = true;
            }
            KeyCode::Char('k') | KeyCode::Char('K') => {
                app.selected_game_type = gc_shared::types::GameType::Checkers;
                app.selecting_solo_game = false;
                app.selecting_difficulty = true;
            }
            KeyCode::Char('h') | KeyCode::Char('H') => {
                app.selected_game_type = gc_shared::types::GameType::Chess;
                app.selecting_solo_game = false;
                app.selecting_difficulty = true;
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                app.selected_game_type = gc_shared::types::GameType::Snake;
                app.selecting_solo_game = false;
                app.selecting_difficulty = true;
            }
            KeyCode::Char('l') | KeyCode::Char('L') => {
                app.selected_game_type = gc_shared::types::GameType::BlockBreaker;
                app.selecting_solo_game = false;
                app.selecting_difficulty = true;
            }
            KeyCode::Esc | KeyCode::Char('b') | KeyCode::Char('B') => {
                app.selecting_solo_game = false;
            }
            _ => {}
        }
        return;
    }

    // Sub-state: multiplayer game selection on lobby screen
    if app.selecting_multiplayer_game {
        match code {
            KeyCode::Char('t') | KeyCode::Char('T') => {
                app.selected_game_type = gc_shared::types::GameType::TicTacToe;
                app.selecting_multiplayer_game = false;

                let settings = gc_shared::types::GameSettings::default();
                app.current_game_type = app.selected_game_type;
                app.current_max_players = settings.max_players;
                let _ = net.send(NetCommand::CreateRoom {
                    game_type: app.selected_game_type,
                    settings,
                });
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                app.selected_game_type = gc_shared::types::GameType::Connect4;
                app.selecting_multiplayer_game = false;

                let settings = gc_shared::types::GameSettings::default();
                app.current_game_type = app.selected_game_type;
                app.current_max_players = settings.max_players;
                let _ = net.send(NetCommand::CreateRoom {
                    game_type: app.selected_game_type,
                    settings,
                });
            }
            KeyCode::Char('k') | KeyCode::Char('K') => {
                app.selected_game_type = gc_shared::types::GameType::Checkers;
                app.selecting_multiplayer_game = false;

                let settings = gc_shared::types::GameSettings::default();
                app.current_game_type = app.selected_game_type;
                app.current_max_players = settings.max_players;
                let _ = net.send(NetCommand::CreateRoom {
                    game_type: app.selected_game_type,
                    settings,
                });
            }
            KeyCode::Char('h') | KeyCode::Char('H') => {
                app.selected_game_type = gc_shared::types::GameType::Chess;
                app.selecting_multiplayer_game = false;

                let settings = gc_shared::types::GameSettings::default();
                app.current_game_type = app.selected_game_type;
                app.current_max_players = settings.max_players;
                let _ = net.send(NetCommand::CreateRoom {
                    game_type: app.selected_game_type,
                    settings,
                });
            }
            KeyCode::Esc | KeyCode::Char('b') | KeyCode::Char('B') => {
                app.selecting_multiplayer_game = false;
            }
            _ => {}
        }
        return;
    }

    // Sub-state: difficulty selection
    if app.selecting_difficulty {
        let is_bb = app.selected_game_type == gc_shared::types::GameType::BlockBreaker;
        match code {
            KeyCode::Char('e') | KeyCode::Char('E') => {
                app.selecting_difficulty = false;
                if is_bb {
                    app.bb_difficulty = gc_shared::game::blockbreaker::BBDifficulty::Easy;
                }
                app.start_solo_game(gc_shared::types::Difficulty::Easy);
            }
            KeyCode::Char('h') | KeyCode::Char('H') => {
                app.selecting_difficulty = false;
                if is_bb {
                    app.bb_difficulty = gc_shared::game::blockbreaker::BBDifficulty::Hard;
                }
                app.start_solo_game(gc_shared::types::Difficulty::Hard);
            }
            KeyCode::Char('x') | KeyCode::Char('X') if is_bb => {
                app.selecting_difficulty = false;
                app.bb_difficulty = gc_shared::game::blockbreaker::BBDifficulty::Hardcore;
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
        KeyCode::Char('g') | KeyCode::Char('G') => {
            // Cycle through available game types
            app.selected_game_type = match app.selected_game_type {
                gc_shared::types::GameType::TicTacToe => gc_shared::types::GameType::Connect4,
                gc_shared::types::GameType::Connect4 => gc_shared::types::GameType::Checkers,
                gc_shared::types::GameType::Checkers => gc_shared::types::GameType::Chess,
                gc_shared::types::GameType::Chess => gc_shared::types::GameType::Snake,
                gc_shared::types::GameType::Snake => gc_shared::types::GameType::BlockBreaker,
                gc_shared::types::GameType::BlockBreaker => gc_shared::types::GameType::TicTacToe,
                _ => gc_shared::types::GameType::TicTacToe,
            };
        }
        KeyCode::Char('c') | KeyCode::Char('C') => {
            app.selecting_multiplayer_game = true;
        }
        KeyCode::Char('b') | KeyCode::Char('B') => {
            app.selecting_solo_game = true;
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
                app.selected_game_type = room.game_type;
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
        // Incoming rematch modal: Y/N only
        if app.rematch_incoming {
            match code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    app.rematch_incoming = false;
                    let _ = net.send(NetCommand::RematchResponse { accept: true });
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    app.rematch_incoming = false;
                    let _ = net.send(NetCommand::RematchResponse { accept: false });
                }
                _ => {}
            }
            return;
        }

        match code {
            KeyCode::Char('r') | KeyCode::Char('R') => {
                if matches!(app.game_mode, GameMode::Solo { .. }) {
                    app.rematch_solo();
                } else if !app.rematch_pending {
                    app.rematch_pending = true;
                    let _ = net.send(NetCommand::RequestRematch);
                }
            }
            _ => {}
        }
        return;
    }

    // Dispatch cursor movement based on game type
    match &app.game_state {
        Some(app::ClientGameState::Connect4(_)) => {
            handle_connect4_key(app, code, net);
        }
        Some(app::ClientGameState::Checkers(_)) => {
            handle_checkers_key(app, code, net);
        }
        Some(app::ClientGameState::Chess(_)) => {
            handle_chess_key(app, code, net);
        }
        Some(app::ClientGameState::Snake(_)) => {
            handle_snake_key(app, code, net);
        }
        Some(app::ClientGameState::BlockBreaker(_)) => {
            handle_blockbreaker_key(app, code);
        }
        _ => {
            handle_tictactoe_key(app, code, net);
        }
    }
}

fn handle_chess_key(app: &mut App, code: KeyCode, net: &NetworkClient) {
    use gc_shared::game::chess::{self as chess_engine, Chess, ChessMove, PieceKind, Position};
    use gc_shared::game::traits::GameEngine;

    // Pending promotion: only Q/R/B/N or Esc/Backspace are meaningful.
    if let Some((from, to)) = app.chess_input.pending_promotion {
        match code {
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                submit_chess_promotion(app, net, from, to, PieceKind::Queen);
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                submit_chess_promotion(app, net, from, to, PieceKind::Rook);
            }
            KeyCode::Char('b') | KeyCode::Char('B') => {
                submit_chess_promotion(app, net, from, to, PieceKind::Bishop);
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                submit_chess_promotion(app, net, from, to, PieceKind::Knight);
            }
            KeyCode::Backspace | KeyCode::Esc => {
                app.chess_input.pending_promotion = None;
            }
            _ => {}
        }
        return;
    }

    match code {
        KeyCode::Up => {
            if app.chess_input.cursor_row < 7 {
                app.chess_input.cursor_row += 1;
            }
        }
        KeyCode::Down => {
            if app.chess_input.cursor_row > 0 {
                app.chess_input.cursor_row -= 1;
            }
        }
        KeyCode::Left => {
            if app.chess_input.cursor_col > 0 {
                app.chess_input.cursor_col -= 1;
            }
        }
        KeyCode::Right => {
            if app.chess_input.cursor_col < 7 {
                app.chess_input.cursor_col += 1;
            }
        }
        KeyCode::Backspace => {
            app.chess_input.selected_from = None;
            app.status_message = None;
        }
        KeyCode::Enter => {
            if !app.is_our_turn() {
                return;
            }
            let cursor = Position::new(app.chess_input.cursor_row, app.chess_input.cursor_col);
            let Some(app::ClientGameState::Chess(state)) = app.game_state.as_ref() else {
                return;
            };
            let player_id = match app.my_player_id {
                Some(id) => id,
                None => return,
            };
            let our_side = if Chess::current_player(state) == player_id {
                if state.current_turn == 0 {
                    gc_shared::game::chess::Side::White
                } else {
                    gc_shared::game::chess::Side::Black
                }
            } else {
                return;
            };

            match app.chess_input.selected_from {
                None => {
                    // Pick a piece of our side.
                    if let Some(piece) = state.board[cursor.row as usize][cursor.col as usize]
                        && piece.side == our_side
                    {
                        app.chess_input.selected_from = Some(cursor);
                        app.status_message = None;
                    } else {
                        app.status_error = Some("Select one of your own pieces".to_string());
                    }
                }
                Some(from) => {
                    if from == cursor {
                        // Deselect.
                        app.chess_input.selected_from = None;
                        return;
                    }
                    // Check if any legal move from->cursor exists, and whether it
                    // requires promotion.
                    let legals: Vec<ChessMove> = chess_engine::legal_moves(state)
                        .into_iter()
                        .filter(|m| m.from == from && m.to == cursor)
                        .collect();
                    if legals.is_empty() {
                        app.status_error = Some("Illegal move".to_string());
                        return;
                    }
                    if legals.iter().any(|m| m.promotion.is_some()) {
                        // Defer until user picks the promotion piece.
                        app.chess_input.pending_promotion = Some((from, cursor));
                    } else {
                        let mv = legals[0];
                        finalize_chess_move(app, net, mv);
                    }
                }
            }
        }
        _ => {}
    }
}

fn submit_chess_promotion(
    app: &mut App,
    net: &NetworkClient,
    from: gc_shared::game::chess::Position,
    to: gc_shared::game::chess::Position,
    promotion: gc_shared::game::chess::PieceKind,
) {
    let mv = gc_shared::game::chess::ChessMove {
        from,
        to,
        promotion: Some(promotion),
    };
    app.chess_input.pending_promotion = None;
    finalize_chess_move(app, net, mv);
}

fn finalize_chess_move(app: &mut App, net: &NetworkClient, mv: gc_shared::game::chess::ChessMove) {
    match app.game_mode {
        GameMode::Solo { .. } => {
            if !app.submit_chess_move(mv) {
                app.status_error = Some("Illegal move".to_string());
            }
        }
        GameMode::Online => {
            if let Ok(data) = gc_shared::protocol::codec::encode(&mv) {
                let _ = net.send(NetCommand::GameAction { data });
                app.chess_input.selected_from = None;
            }
        }
    }
}

fn handle_checkers_key(app: &mut App, code: KeyCode, net: &NetworkClient) {
    match code {
        KeyCode::Up => app.checkers_cursor_step(-1, 0),
        KeyCode::Down => app.checkers_cursor_step(1, 0),
        KeyCode::Left => app.checkers_cursor_step(0, -1),
        KeyCode::Right => app.checkers_cursor_step(0, 1),
        KeyCode::Backspace | KeyCode::Char('c') | KeyCode::Char('C') => {
            app.checkers_cancel_selection()
        }
        KeyCode::Enter => {
            if !app.is_our_turn() {
                return;
            }
            if let Some(mv) = app.checkers_confirm()
                && matches!(app.game_mode, app::GameMode::Online)
                && let Ok(data) = gc_shared::protocol::codec::encode(&mv)
            {
                let _ = net.send(NetCommand::GameAction { data });
            }
        }
        _ => {}
    }
}

fn handle_tictactoe_key(app: &mut App, code: KeyCode, net: &NetworkClient) {
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
            if app.is_our_turn() {
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

fn handle_snake_key(app: &mut App, code: KeyCode, net: &NetworkClient) {
    use gc_shared::game::snake::{Direction as SnakeDir, SnakeInput};

    let dir = match code {
        KeyCode::Up | KeyCode::Char('w') | KeyCode::Char('W') => Some(SnakeDir::Up),
        KeyCode::Down | KeyCode::Char('s') | KeyCode::Char('S') => Some(SnakeDir::Down),
        KeyCode::Left | KeyCode::Char('a') | KeyCode::Char('A') => Some(SnakeDir::Left),
        KeyCode::Right | KeyCode::Char('d') | KeyCode::Char('D') => Some(SnakeDir::Right),
        _ => None,
    };
    let Some(new_dir) = dir else {
        return;
    };

    // Reject 180° reversals client-side — server also re-checks.
    if let Some(app::ClientGameState::Snake(ref state)) = app.game_state
        && let Some(me) = app.my_player_id
        && let Some(my_snake) = state.snakes.iter().find(|s| s.player_id == me)
        && is_opposite(my_snake.direction, new_dir)
    {
        return;
    }

    let changed = app.snake_queue_direction(new_dir);

    if matches!(app.game_mode, GameMode::Online) && changed {
        let input = SnakeInput { direction: new_dir };
        if let Ok(data) = gc_shared::protocol::codec::encode(&input) {
            let _ = net.send(NetCommand::GameAction { data });
            app.snake_last_sent_direction = Some(new_dir);
        }
    }
}

fn is_opposite(a: gc_shared::game::snake::Direction, b: gc_shared::game::snake::Direction) -> bool {
    use gc_shared::game::snake::Direction as D;
    matches!(
        (a, b),
        (D::Up, D::Down) | (D::Down, D::Up) | (D::Left, D::Right) | (D::Right, D::Left)
    )
}

fn handle_blockbreaker_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Left | KeyCode::Char('a') | KeyCode::Char('A') => app.bb_set_dir(-1),
        KeyCode::Right | KeyCode::Char('d') | KeyCode::Char('D') => app.bb_set_dir(1),
        KeyCode::Char(' ') | KeyCode::Enter => app.bb_request_launch(),
        _ => {}
    }
}

fn handle_connect4_key(app: &mut App, code: KeyCode, net: &NetworkClient) {
    use gc_shared::game::connect4::COLS;
    match code {
        KeyCode::Left => {
            if app.cursor_col > 0 {
                app.cursor_col -= 1;
            }
        }
        KeyCode::Right => {
            if app.cursor_col < COLS as u8 - 1 {
                app.cursor_col += 1;
            }
        }
        KeyCode::Enter => {
            if app.is_our_turn() {
                match &app.game_mode {
                    GameMode::Solo { .. } => {
                        app.play_solo_move(0, app.cursor_col);
                    }
                    GameMode::Online => {
                        let mv = gc_shared::game::connect4::Connect4Move {
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
