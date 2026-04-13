use gc_shared::game::connect4::{self, Connect4, Connect4State};
use gc_shared::game::tictactoe::{self, TicTacToe, TicTacToeState};
use gc_shared::game::traits::GameEngine;
use gc_shared::i18n::{Language, Translator};
use gc_shared::protocol::messages::RoomSummary;
use gc_shared::types::{
    Difficulty, GameOutcome, GameSettings, GameType, PlayerId, PlayerInfo, RoomId,
};

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

/// Client-side game state wrapper supporting multiple game types.
#[derive(Debug, Clone)]
pub enum ClientGameState {
    TicTacToe(TicTacToeState),
    Connect4(Connect4State),
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
    pub current_game_type: GameType,
    pub current_max_players: u8,

    // Game state
    pub game_mode: GameMode,
    pub selected_game_type: GameType,
    pub game_state: Option<ClientGameState>,
    pub cursor_row: u8,
    pub cursor_col: u8,
    pub game_over: Option<GameOutcome>,
    pub my_player_id: Option<PlayerId>,

    // Lobby sub-state: difficulty selection for solo mode
    pub selecting_solo_game: bool,
    pub selecting_difficulty: bool,

    // Lobby sub-state: game selection for multiplayer mode
    pub selecting_multiplayer_game: bool,

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
            current_game_type: GameType::TicTacToe,
            current_max_players: 2,
            game_mode: GameMode::Online,
            selected_game_type: GameType::TicTacToe,
            game_state: None,
            cursor_row: 1,
            cursor_col: 1,
            game_over: None,
            my_player_id: None,
            selecting_solo_game: false,
            selecting_difficulty: false,
            selecting_multiplayer_game: false,
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
        let decoded = match self.selected_game_type {
            GameType::TicTacToe => gc_shared::protocol::codec::decode::<TicTacToeState>(state_data)
                .ok()
                .map(ClientGameState::TicTacToe),
            GameType::Connect4 => gc_shared::protocol::codec::decode::<Connect4State>(state_data)
                .ok()
                .map(ClientGameState::Connect4),
            _ => None,
        };
        if let Some(state) = decoded {
            self.game_state = Some(state);
            if self.screen == Screen::Room {
                self.screen = Screen::InGame;
                self.cursor_row = match self.selected_game_type {
                    GameType::Connect4 => 0, // cursor not used for row in Connect4
                    _ => 1,
                };
                self.cursor_col = match self.selected_game_type {
                    GameType::Connect4 => 3, // center column
                    _ => 1,
                };
            }
        }
    }

    /// Called when game is over.
    pub fn on_game_over(&mut self, outcome: GameOutcome) {
        self.game_over = Some(outcome.clone());

        // Record match history
        let result_str = match &outcome {
            GameOutcome::Win(pid) if Some(*pid) == self.my_player_id => "win",
            GameOutcome::Win(_) => "loss",
            GameOutcome::Draw => "draw",
        };
        let game_type_str = format!("{}", self.current_game_type);
        let _ = self.db.record_match(&game_type_str, None, result_str);
    }

    /// Start a local solo game against the bot.
    pub fn start_solo_game(&mut self, difficulty: Difficulty) {
        let player_id = self.my_player_id.unwrap_or_else(|| {
            let id = PlayerId::new();
            self.my_player_id = Some(id);
            id
        });
        let bot_id = PlayerId::new();
        let players = [player_id, bot_id];
        let settings = GameSettings::default();

        let state = match self.selected_game_type {
            GameType::TicTacToe => {
                ClientGameState::TicTacToe(TicTacToe::initial_state(&players, &settings))
            }
            GameType::Connect4 => {
                ClientGameState::Connect4(Connect4::initial_state(&players, &settings))
            }
            _ => return,
        };

        self.game_mode = GameMode::Solo { difficulty };
        self.game_state = Some(state);
        self.game_over = None;
        self.cursor_row = match self.selected_game_type {
            GameType::Connect4 => 0, // cursor not used for row in Connect4
            _ => 1,
        };
        self.cursor_col = match self.selected_game_type {
            GameType::Connect4 => 3, // center column
            _ => 1,
        };
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

        match state {
            ClientGameState::TicTacToe(state) => {
                let mv = gc_shared::game::tictactoe::TicTacToeMove { row, col };
                if TicTacToe::validate_move(state, player_id, &mv).is_err() {
                    return;
                }
                TicTacToe::apply_move(state, player_id, &mv);
                if let Some(outcome) = TicTacToe::is_terminal(state) {
                    self.game_over = Some(outcome);
                    return;
                }
                let bot_mv = tictactoe::bot_move(state, difficulty);
                let bot_id = TicTacToe::current_player(state);
                TicTacToe::apply_move(state, bot_id, &bot_mv);
                if let Some(outcome) = TicTacToe::is_terminal(state) {
                    self.game_over = Some(outcome);
                }
            }
            ClientGameState::Connect4(state) => {
                let mv = gc_shared::game::connect4::Connect4Move { col };
                if Connect4::validate_move(state, player_id, &mv).is_err() {
                    return;
                }
                Connect4::apply_move(state, player_id, &mv);
                if let Some(outcome) = Connect4::is_terminal(state) {
                    self.game_over = Some(outcome);
                    return;
                }
                let bot_mv = connect4::bot_move(state, difficulty);
                let bot_id = Connect4::current_player(state);
                Connect4::apply_move(state, bot_id, &bot_mv);
                if let Some(outcome) = Connect4::is_terminal(state) {
                    self.game_over = Some(outcome);
                }
            }
        }
    }

    /// Restart the current solo game with the same difficulty.
    pub fn rematch_solo(&mut self) {
        let difficulty = match &self.game_mode {
            GameMode::Solo { difficulty } => *difficulty,
            _ => return,
        };
        self.start_solo_game(difficulty);
    }

    /// Leave a solo game and return to lobby (if authenticated) or login screen.
    pub fn leave_solo_game(&mut self) {
        self.game_mode = GameMode::Online;
        self.game_state = None;
        self.game_over = None;
        self.screen = if self.authenticated {
            Screen::Lobby
        } else {
            Screen::Login
        };
    }

    /// Check if it's our turn in the current game.
    pub fn is_our_turn(&self) -> bool {
        let state = match self.game_state.as_ref() {
            Some(s) => s,
            None => return false,
        };
        let current = match state {
            ClientGameState::TicTacToe(s) => TicTacToe::current_player(s),
            ClientGameState::Connect4(s) => Connect4::current_player(s),
        };
        Some(current) == self.my_player_id
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
        match state {
            ClientGameState::TicTacToe(s) => assert_eq!(s.move_count, 2),
            _ => panic!("expected TicTacToe state"),
        }
    }

    #[test]
    fn solo_rematch_resets_game() {
        let mut app = test_app();
        app.start_solo_game(Difficulty::Easy);
        // Play until bot responds
        app.play_solo_move(0, 0);
        match app.game_state.as_ref().unwrap() {
            ClientGameState::TicTacToe(s) => assert!(s.move_count >= 2),
            _ => panic!("expected TicTacToe state"),
        }

        // Simulate game over and rematch
        app.game_over = Some(GameOutcome::Draw);
        app.rematch_solo();

        assert!(app.game_over.is_none());
        match app.game_state.as_ref().unwrap() {
            ClientGameState::TicTacToe(s) => assert_eq!(s.move_count, 0),
            _ => panic!("expected TicTacToe state"),
        }
        assert_eq!(app.screen, Screen::InGame);
        assert!(matches!(
            app.game_mode,
            GameMode::Solo {
                difficulty: Difficulty::Easy
            }
        ));
    }

    #[test]
    fn solo_connect4_starts_and_moves() {
        let mut app = test_app();
        app.selected_game_type = GameType::Connect4;
        app.start_solo_game(Difficulty::Easy);
        assert_eq!(app.screen, Screen::InGame);
        assert!(matches!(app.game_state, Some(ClientGameState::Connect4(_))));
        // Play a move in center column
        app.play_solo_move(0, 3);
        match app.game_state.as_ref().unwrap() {
            ClientGameState::Connect4(s) => assert_eq!(s.move_count, 2),
            _ => panic!("expected Connect4 state"),
        }
    }

    #[test]
    fn difficulty_selection_state() {
        let mut app = test_app();
        assert!(!app.selecting_difficulty);
        app.selecting_difficulty = true;
        assert!(app.selecting_difficulty);
        app.selecting_difficulty = false;
        assert!(!app.selecting_difficulty);
    }

    #[test]
    fn leave_solo_game() {
        let mut app = test_app();
        // Unauthenticated: should return to login
        app.start_solo_game(Difficulty::Hard);
        app.leave_solo_game();
        assert_eq!(app.screen, Screen::Login);
        assert!(app.game_state.is_none());
        assert_eq!(app.game_mode, GameMode::Online);

        // Authenticated: should return to lobby
        app.authenticated = true;
        app.start_solo_game(Difficulty::Hard);
        app.leave_solo_game();
        assert_eq!(app.screen, Screen::Lobby);
    }
}
