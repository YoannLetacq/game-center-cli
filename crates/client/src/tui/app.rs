use gc_shared::game::tictactoe::{self, TicTacToe, TicTacToeState};
use gc_shared::game::traits::GameEngine;
use gc_shared::i18n::{Language, Translator};
use gc_shared::protocol::messages::RoomSummary;
use gc_shared::types::{Difficulty, GameOutcome, GameSettings, PlayerId, PlayerInfo, RoomId};

use crate::database::ClientDatabase;

/// Which screen/state the application is in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    Login,
    Lobby,
    Room,
    InGame,
}

/// The mode of the login screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginMode {
    Login,
    Register,
}

/// Whether we're playing locally or online.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameMode {
    /// Playing against the server (multiplayer).
    Online,
    /// Playing locally against a bot.
    Solo { difficulty: Difficulty },
}

/// Application state for the TUI.
pub struct App {
    pub running: bool,
    pub screen: Screen,
    pub login_mode: LoginMode,
    pub translator: Translator,
    pub db: ClientDatabase,
    pub server_url: String,

    // Login form state
    pub username_input: String,
    pub password_input: String,
    pub active_field: LoginField,
    pub login_error: Option<String>,
    pub login_loading: bool,

    // Auth state
    pub authenticated: bool,
    pub auth_token: Option<String>,

    // Lobby state
    pub rooms: Vec<RoomSummary>,
    pub selected_room: usize,

    // Room state
    pub current_room_id: Option<RoomId>,
    pub current_room_players: Vec<PlayerInfo>,

    // Game state
    pub game_mode: GameMode,
    pub game_state: Option<TicTacToeState>,
    pub cursor_row: u8,
    pub cursor_col: u8,
    pub game_over: Option<GameOutcome>,
    pub my_player_id: Option<PlayerId>,

    // Generic status error (shown on lobby/room screens)
    pub status_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginField {
    Username,
    Password,
}

impl App {
    pub fn new(language: Language, db: ClientDatabase, server_url: String) -> Self {
        // Try to restore username from profile
        let username = db.get_profile().unwrap_or(None).unwrap_or_default();

        Self {
            running: true,
            screen: Screen::Login,
            login_mode: LoginMode::Login,
            translator: Translator::new(language),
            db,
            server_url,
            username_input: username,
            password_input: String::new(),
            active_field: LoginField::Username,
            login_error: None,
            login_loading: false,
            authenticated: false,
            auth_token: None,
            rooms: Vec::new(),
            selected_room: 0,
            current_room_id: None,
            current_room_players: Vec::new(),
            game_mode: GameMode::Online,
            game_state: None,
            cursor_row: 1,
            cursor_col: 1,
            game_over: None,
            my_player_id: None,
            status_error: None,
        }
    }

    pub fn quit(&mut self) {
        self.running = false;
    }

    pub fn toggle_login_mode(&mut self) {
        self.login_mode = match self.login_mode {
            LoginMode::Login => LoginMode::Register,
            LoginMode::Register => LoginMode::Login,
        };
        self.login_error = None;
    }

    pub fn toggle_field(&mut self) {
        self.active_field = match self.active_field {
            LoginField::Username => LoginField::Password,
            LoginField::Password => LoginField::Username,
        };
    }

    #[allow(dead_code)]
    pub fn set_language(&mut self, lang: Language) {
        self.translator = Translator::new(lang);
    }

    /// Validate login form and return error if invalid.
    pub fn validate_login_form(&self) -> Option<String> {
        if self.username_input.trim().is_empty() {
            return Some(
                self.translator
                    .get("login.error_empty_username")
                    .to_string(),
            );
        }
        if self.login_mode == LoginMode::Register && self.password_input.len() < 8 {
            return Some(
                self.translator
                    .get("login.error_short_password")
                    .to_string(),
            );
        }
        None
    }

    /// Called when authentication succeeds.
    pub fn on_auth_success(&mut self, token: String) {
        self.auth_token = Some(token);
        self.authenticated = true;
        self.login_loading = false;
        self.login_error = None;
        self.password_input.clear();
        let _ = self.db.save_profile(&self.username_input);
        self.screen = Screen::Lobby;
    }

    /// Called when authentication fails.
    pub fn on_auth_failure(&mut self, reason: String) {
        self.login_loading = false;
        self.login_error = Some(reason);
    }

    /// Update the room list.
    pub fn update_rooms(&mut self, rooms: Vec<RoomSummary>) {
        self.rooms = rooms;
        if self.selected_room >= self.rooms.len() && !self.rooms.is_empty() {
            self.selected_room = self.rooms.len() - 1;
        }
    }

    /// Called when we join a room.
    pub fn on_room_joined(&mut self, room_id: RoomId, players: Vec<PlayerInfo>) {
        self.current_room_id = Some(room_id);
        self.current_room_players = players;
        self.screen = Screen::Room;
    }

    /// Called when we leave a room.
    pub fn on_room_left(&mut self) {
        self.current_room_id = None;
        self.current_room_players.clear();
        self.game_state = None;
        self.game_over = None;
        self.cursor_row = 1;
        self.cursor_col = 1;
        self.screen = Screen::Lobby;
    }

    /// Called when game state is received from server.
    pub fn on_game_state(&mut self, state_data: &[u8]) {
        if let Ok(state) = gc_shared::protocol::codec::decode::<TicTacToeState>(state_data) {
            self.game_state = Some(state);
            if self.screen == Screen::Room {
                self.screen = Screen::InGame;
            }
        }
    }

    /// Called when game is over.
    pub fn on_game_over(&mut self, outcome: GameOutcome) {
        self.game_over = Some(outcome);
    }

    /// Start a local solo game against the bot.
    pub fn start_solo_game(&mut self, difficulty: Difficulty) {
        let player_id = self.my_player_id.unwrap_or_else(|| {
            let id = PlayerId::new();
            self.my_player_id = Some(id);
            id
        });
        let bot_id = PlayerId::new();

        let state = TicTacToe::initial_state(&[player_id, bot_id], &GameSettings::default());
        self.game_mode = GameMode::Solo { difficulty };
        self.game_state = Some(state);
        self.game_over = None;
        self.cursor_row = 1;
        self.cursor_col = 1;
        self.screen = Screen::InGame;
    }

    /// Apply a player's move in solo mode, then let the bot respond.
    pub fn play_solo_move(&mut self, row: u8, col: u8) {
        let difficulty = match &self.game_mode {
            GameMode::Solo { difficulty } => *difficulty,
            _ => return,
        };

        let state = match self.game_state.as_mut() {
            Some(s) => s,
            None => return,
        };

        let player_id = match self.my_player_id {
            Some(id) => id,
            None => return,
        };

        // Validate and apply player's move
        let mv = gc_shared::game::tictactoe::TicTacToeMove { row, col };
        if TicTacToe::validate_move(state, player_id, &mv).is_err() {
            return;
        }
        TicTacToe::apply_move(state, player_id, &mv);

        // Check if game ended after player's move
        if let Some(outcome) = TicTacToe::is_terminal(state) {
            self.game_over = Some(outcome);
            return;
        }

        // Bot's turn
        let bot_mv = tictactoe::bot_move(state, difficulty);
        let bot_id = TicTacToe::current_player(state);
        TicTacToe::apply_move(state, bot_id, &bot_mv);

        // Check if game ended after bot's move
        if let Some(outcome) = TicTacToe::is_terminal(state) {
            self.game_over = Some(outcome);
        }
    }

    /// Leave a solo game and return to lobby.
    pub fn leave_solo_game(&mut self) {
        self.game_mode = GameMode::Online;
        self.game_state = None;
        self.game_over = None;
        self.screen = Screen::Lobby;
    }

    /// Called when a player joins our room.
    pub fn on_player_joined(&mut self, player: PlayerInfo) {
        if !self.current_room_players.iter().any(|p| p.id == player.id) {
            self.current_room_players.push(player);
        }
    }

    /// Called when a player leaves our room.
    pub fn on_player_left(&mut self, player_id: gc_shared::types::PlayerId) {
        self.current_room_players.retain(|p| p.id != player_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> App {
        let db = ClientDatabase::open_in_memory().unwrap();
        App::new(Language::English, db, "wss://localhost:8443".to_string())
    }

    #[test]
    fn initial_state() {
        let app = test_app();
        assert!(app.running);
        assert_eq!(app.screen, Screen::Login);
        assert_eq!(app.login_mode, LoginMode::Login);
        assert!(!app.authenticated);
        assert!(app.rooms.is_empty());
    }

    #[test]
    fn toggle_login_mode() {
        let mut app = test_app();
        assert_eq!(app.login_mode, LoginMode::Login);
        app.toggle_login_mode();
        assert_eq!(app.login_mode, LoginMode::Register);
        app.toggle_login_mode();
        assert_eq!(app.login_mode, LoginMode::Login);
    }

    #[test]
    fn validate_empty_username() {
        let app = test_app();
        assert!(app.validate_login_form().is_some());
    }

    #[test]
    fn validate_short_password_register() {
        let mut app = test_app();
        app.login_mode = LoginMode::Register;
        app.username_input = "alice".to_string();
        app.password_input = "short".to_string();
        assert!(app.validate_login_form().is_some());
    }

    #[test]
    fn validate_valid_login() {
        let mut app = test_app();
        app.username_input = "alice".to_string();
        app.password_input = "password123".to_string();
        assert!(app.validate_login_form().is_none());
    }

    #[test]
    fn auth_success_transitions_to_lobby() {
        let mut app = test_app();
        app.username_input = "alice".to_string();
        app.on_auth_success("jwt-token".to_string());
        assert_eq!(app.screen, Screen::Lobby);
        assert!(app.authenticated);
        assert!(app.password_input.is_empty());
    }

    #[test]
    fn auth_failure_shows_error() {
        let mut app = test_app();
        app.login_loading = true;
        app.on_auth_failure("bad password".to_string());
        assert!(!app.login_loading);
        assert_eq!(app.login_error.as_deref(), Some("bad password"));
    }

    #[test]
    fn language_switch() {
        let mut app = test_app();
        assert_eq!(app.translator.get("app.title"), "Game Center");
        app.set_language(Language::French);
        assert_eq!(app.translator.get("app.title"), "Centre de Jeux");
    }

    #[test]
    fn room_joined_transitions_to_room_screen() {
        let mut app = test_app();
        let room_id = RoomId::new();
        let players = vec![PlayerInfo {
            id: gc_shared::types::PlayerId::new(),
            username: "alice".to_string(),
        }];
        app.on_room_joined(room_id, players.clone());
        assert_eq!(app.screen, Screen::Room);
        assert_eq!(app.current_room_id, Some(room_id));
        assert_eq!(app.current_room_players.len(), 1);
    }

    #[test]
    fn room_left_transitions_to_lobby() {
        let mut app = test_app();
        app.screen = Screen::Room;
        app.current_room_id = Some(RoomId::new());
        app.on_room_left();
        assert_eq!(app.screen, Screen::Lobby);
        assert!(app.current_room_id.is_none());
    }

    #[test]
    fn solo_game_starts() {
        let mut app = test_app();
        app.start_solo_game(Difficulty::Hard);
        assert_eq!(app.screen, Screen::InGame);
        assert!(app.game_state.is_some());
        assert!(matches!(app.game_mode, GameMode::Solo { .. }));
    }

    #[test]
    fn solo_move_applies_and_bot_responds() {
        let mut app = test_app();
        app.start_solo_game(Difficulty::Easy);
        // Player places at (0,0)
        app.play_solo_move(0, 0);
        let state = app.game_state.as_ref().unwrap();
        // Player's move + bot's response = 2 moves
        assert_eq!(state.move_count, 2);
    }

    #[test]
    fn leave_solo_game() {
        let mut app = test_app();
        app.start_solo_game(Difficulty::Hard);
        app.leave_solo_game();
        assert_eq!(app.screen, Screen::Lobby);
        assert!(app.game_state.is_none());
        assert_eq!(app.game_mode, GameMode::Online);
    }
}
