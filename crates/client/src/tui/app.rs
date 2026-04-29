use std::collections::HashMap;
use std::time::Instant;

use gc_shared::game::checkers::{
    self, BOARD_SIZE, Checkers, CheckersMove, CheckersState, Position, Side, Square,
};
use gc_shared::game::chess::{
    self, Chess, ChessMove, ChessState, Position as ChessPosition,
};
use gc_shared::game::connect4::{self, Connect4, Connect4State};
use gc_shared::game::snake::{self, Direction, SnakeDelta, SnakeEngine, SnakeInput, SnakeState};
use gc_shared::game::tictactoe::{self, TicTacToe, TicTacToeState};
use gc_shared::game::traits::{GameEngine, RealtimeGameEngine};
use gc_shared::i18n::{Language, Translator};
use gc_shared::protocol::messages::RoomSummary;
use gc_shared::types::{
    Difficulty, GameOutcome, GameSettings, GameType, PlayerId, PlayerInfo, RoomId,
};

/// Solo Snake tick period. Matches server-side cadence for consistent feel.
pub const SNAKE_TICK_MS: u64 = 100;

/// Derive a non-deterministic u64 seed from a fresh UUID.
/// The client crate doesn't pull `rand`; UUID v4 already uses OS entropy.
fn rand_seed() -> u64 {
    let bytes = *uuid::Uuid::new_v4().as_bytes();
    u64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ])
}

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
    Checkers(CheckersState),
    Chess(ChessState),
    Snake(SnakeState),
}

/// Input sub-state for chess: cursor position, current selection, and any
/// pending pawn-promotion target awaiting the user's piece choice.
#[derive(Debug, Clone)]
pub struct ChessInputState {
    pub selected_from: Option<ChessPosition>,
    pub pending_promotion: Option<(ChessPosition, ChessPosition)>,
    pub cursor_row: u8,
    pub cursor_col: u8,
}

impl ChessInputState {
    pub fn new() -> Self {
        Self {
            selected_from: None,
            pending_promotion: None,
            cursor_row: 0,
            cursor_col: 4,
        }
    }

    pub fn reset(&mut self) {
        self.selected_from = None;
        self.pending_promotion = None;
        self.cursor_row = 0;
        self.cursor_col = 4;
    }
}

impl Default for ChessInputState {
    fn default() -> Self {
        Self::new()
    }
}

/// Stage of the checkers input state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckersInputStage {
    /// No piece selected — arrow keys free-move the cursor.
    Idle,
    /// A piece has been picked; cursor is choosing the target square.
    TargetSelect,
    /// After at least one jump — cursor is choosing the next landing or submitting.
    Chaining,
}

/// Input sub-state for checkers, layered on top of `cursor_row`/`cursor_col`.
#[derive(Debug, Clone)]
pub struct CheckersInputState {
    pub stage: CheckersInputStage,
    pub origin: Option<Position>,
    /// Start square plus each landing square chosen so far. Empty in Idle.
    pub partial_path: Vec<Position>,
}

impl CheckersInputState {
    pub fn new() -> Self {
        Self {
            stage: CheckersInputStage::Idle,
            origin: None,
            partial_path: Vec::new(),
        }
    }

    pub fn reset(&mut self) {
        self.stage = CheckersInputStage::Idle;
        self.origin = None;
        self.partial_path.clear();
    }
}

impl Default for CheckersInputState {
    fn default() -> Self {
        Self::new()
    }
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

    // UI state
    pub show_help: bool,

    // Rematch state (online games only)
    /// We sent a rematch request; waiting for opponent's response.
    pub rematch_pending: bool,
    /// Opponent sent a rematch request; waiting for our Y/N.
    pub rematch_incoming: bool,

    // Solo mode turn order
    /// Whether the human player went first in the last solo game.
    /// None = no solo game played yet (first game will be random).
    /// Alternates on every rematch.
    pub solo_player_went_first: Option<bool>,

    // Checkers input state machine (only used when game is Checkers).
    pub checkers_input: CheckersInputState,

    // Chess input state (only used when game is Chess).
    pub chess_input: ChessInputState,

    /// Transient status/error line shown inside the game screen
    /// (e.g. illegal selection feedback for Checkers).
    pub status_message: Option<String>,

    // --- Snake state ---
    /// Pending direction queued by the player (sent online; passed to solo tick).
    pub snake_pending_direction: Option<Direction>,
    /// Last direction we actually sent over the wire (online debounce).
    pub snake_last_sent_direction: Option<Direction>,
    /// Bot player id for solo snake — cached so tick loop can compute bot input.
    pub snake_bot_player_id: Option<PlayerId>,
    /// Difficulty captured at solo-snake start so the tick thread can replay it.
    pub snake_bot_difficulty: Option<Difficulty>,
    /// Last time a solo snake tick was applied (inline scheduler).
    pub solo_snake_last_tick: Option<Instant>,
    /// Last delta tick we applied in online mode (for out-of-order detection).
    pub snake_last_applied_tick: Option<u64>,
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
            show_help: false,
            rematch_pending: false,
            rematch_incoming: false,
            solo_player_went_first: None,
            checkers_input: CheckersInputState::new(),
            chess_input: ChessInputState::new(),
            status_message: None,
            snake_pending_direction: None,
            snake_last_sent_direction: None,
            snake_bot_player_id: None,
            snake_bot_difficulty: None,
            solo_snake_last_tick: None,
            snake_last_applied_tick: None,
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
        self.rematch_pending = false;
        self.rematch_incoming = false;
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
            GameType::Checkers => gc_shared::protocol::codec::decode::<CheckersState>(state_data)
                .ok()
                .map(ClientGameState::Checkers),
            GameType::Chess => gc_shared::protocol::codec::decode::<ChessState>(state_data)
                .ok()
                .map(ClientGameState::Chess),
            GameType::Snake => gc_shared::protocol::codec::decode::<SnakeState>(state_data)
                .ok()
                .map(ClientGameState::Snake),
            _ => None,
        };
        if let Some(state) = decoded {
            // Snake: baseline snapshot — reset delta sequence tracker.
            if let ClientGameState::Snake(ref s) = state {
                self.snake_last_applied_tick = Some(s.tick);
            }
            self.game_state = Some(state);
            if self.screen == Screen::Room {
                self.screen = Screen::InGame;
                self.cursor_row = match self.selected_game_type {
                    GameType::Connect4 => 0, // cursor not used for row in Connect4
                    GameType::Checkers => 5,
                    _ => 1,
                };
                self.cursor_col = match self.selected_game_type {
                    GameType::Connect4 => 3, // center column
                    GameType::Checkers => 0,
                    _ => 1,
                };
                if self.selected_game_type == GameType::Checkers {
                    self.checkers_input.reset();
                }
                if self.selected_game_type == GameType::Chess {
                    self.chess_input.reset();
                }
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

        // First game in a session: player opens (no surprise bot move on a fresh board).
        // Each rematch: alternate who goes first.
        let player_goes_first = match self.solo_player_went_first {
            None => true,
            Some(went_first) => !went_first,
        };
        self.solo_player_went_first = Some(player_goes_first);

        let players = if player_goes_first {
            [player_id, bot_id]
        } else {
            [bot_id, player_id]
        };
        let settings = GameSettings::default();

        // Reset snake per-game state up front; overwritten below for Snake mode.
        self.snake_pending_direction = None;
        self.snake_last_sent_direction = None;
        self.snake_bot_player_id = None;
        self.snake_bot_difficulty = None;
        self.solo_snake_last_tick = None;
        self.snake_last_applied_tick = None;

        let state = match self.selected_game_type {
            GameType::TicTacToe => {
                ClientGameState::TicTacToe(TicTacToe::initial_state(&players, &settings))
            }
            GameType::Connect4 => {
                ClientGameState::Connect4(Connect4::initial_state(&players, &settings))
            }
            GameType::Checkers => {
                ClientGameState::Checkers(Checkers::initial_state(&players, &settings))
            }
            GameType::Chess => {
                ClientGameState::Chess(Chess::initial_state(&players, &settings))
            }
            GameType::Snake => {
                // Deterministic seed per game; not recorded for rematch.
                let snake_settings = GameSettings {
                    seed: Some(rand_seed()),
                    ..settings.clone()
                };
                self.snake_bot_player_id = Some(bot_id);
                self.snake_bot_difficulty = Some(difficulty);
                self.solo_snake_last_tick = Some(Instant::now());
                let s = SnakeEngine::initial_state(&players, &snake_settings);
                self.snake_last_applied_tick = Some(s.tick);
                ClientGameState::Snake(s)
            }
            _ => return,
        };

        self.game_mode = GameMode::Solo { difficulty };
        self.game_state = Some(state);
        self.game_over = None;
        self.cursor_row = match self.selected_game_type {
            GameType::Connect4 => 0,
            GameType::Checkers => 5,
            _ => 1,
        };
        self.cursor_col = match self.selected_game_type {
            GameType::Connect4 => 3,
            GameType::Checkers => 0,
            _ => 1,
        };
        self.checkers_input.reset();
        self.chess_input.reset();
        self.status_message = None;
        self.screen = Screen::InGame;

        // If the bot goes first, apply its opening move immediately.
        if !player_goes_first && let Some(state) = self.game_state.as_mut() {
            match state {
                ClientGameState::TicTacToe(s) => {
                    let bot_mv = tictactoe::bot_move(s, difficulty);
                    TicTacToe::apply_move(s, bot_id, &bot_mv);
                    if let Some(outcome) = TicTacToe::is_terminal(s) {
                        self.game_over = Some(outcome);
                    }
                }
                ClientGameState::Connect4(s) => {
                    let bot_mv = connect4::bot_move(s, difficulty);
                    Connect4::apply_move(s, bot_id, &bot_mv);
                    if let Some(outcome) = Connect4::is_terminal(s) {
                        self.game_over = Some(outcome);
                    }
                }
                ClientGameState::Checkers(s) => {
                    let bot_mv = checkers::bot_move(s, difficulty);
                    Checkers::apply_move(s, bot_id, &bot_mv);
                    if let Some(outcome) = Checkers::is_terminal(s) {
                        self.game_over = Some(outcome);
                    }
                }
                ClientGameState::Chess(s) => {
                    if let Some(bot_mv) = chess::bot_move(s, difficulty) {
                        Chess::apply_move(s, bot_id, &bot_mv);
                        if let Some(outcome) = Chess::is_terminal(s) {
                            self.game_over = Some(outcome);
                        }
                    }
                }
                // Snake is realtime — no discrete opening move. Tick loop drives it.
                ClientGameState::Snake(_) => {}
            }
        }
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
            ClientGameState::Checkers(_) => {
                // Checkers uses the path-based `submit_checkers_move` API;
                // this single-step (row, col) path is not expressive enough.
            }
            ClientGameState::Chess(_) => {
                // Chess uses `submit_chess_move(ChessMove)`; the (row, col)
                // path is not expressive enough for from/to/promotion.
            }
            ClientGameState::Snake(_) => {
                // Snake is realtime — driven by tick loop, not per-move calls.
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
        self.solo_player_went_first = None;
        self.checkers_input.reset();
        self.chess_input.reset();
        self.status_message = None;
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
            ClientGameState::Checkers(s) => Checkers::current_player(s),
            ClientGameState::Chess(s) => Chess::current_player(s),
            // Snake is realtime — every tick we may input a direction.
            ClientGameState::Snake(_) => {
                return self.game_over.is_none();
            }
        };
        Some(current) == self.my_player_id
    }

    /// Queue a direction change. In solo mode the tick loop picks it up; in
    /// online mode the caller should also emit a `GameAction` when the
    /// direction actually changes (debounce).
    #[allow(dead_code)] // Consumed by T5 snake key handler.
    pub fn snake_queue_direction(&mut self, dir: Direction) -> bool {
        if !matches!(self.game_state, Some(ClientGameState::Snake(_))) {
            return false;
        }
        let changed = self.snake_pending_direction != Some(dir);
        self.snake_pending_direction = Some(dir);
        changed
    }

    /// Drive a single solo snake tick if 100 ms has elapsed since the last one.
    /// Inline scheduling reuses the existing Tick (50 ms) path in `tui::mod.rs`;
    /// avoids a dedicated thread + channel and matches turn-based solo flow.
    pub fn solo_snake_tick(&mut self) {
        if !matches!(self.game_mode, GameMode::Solo { .. }) {
            return;
        }
        if self.game_over.is_some() {
            return;
        }
        let Some(ClientGameState::Snake(_)) = self.game_state.as_ref() else {
            return;
        };
        let now = Instant::now();
        let due = self
            .solo_snake_last_tick
            .map(|t| now.duration_since(t).as_millis() as u64 >= SNAKE_TICK_MS)
            .unwrap_or(true);
        if !due {
            return;
        }
        self.solo_snake_last_tick = Some(now);

        let (Some(player_id), Some(bot_id), Some(difficulty)) = (
            self.my_player_id,
            self.snake_bot_player_id,
            self.snake_bot_difficulty,
        ) else {
            return;
        };
        let Some(ClientGameState::Snake(state)) = self.game_state.as_mut() else {
            return;
        };

        // Solo uses a single arena (arenas[0])
        let mut inputs: HashMap<PlayerId, SnakeInput> = HashMap::new();
        if let Some(dir) = self.snake_pending_direction {
            inputs.insert(player_id, SnakeInput { direction: dir });
        }
        let bot_input = snake::bot_move(&state.arenas[0], bot_id, difficulty);
        inputs.insert(bot_id, bot_input);
        let delta = SnakeEngine::tick(state, &inputs);
        if let Some(outcome) = delta.game_over {
            self.game_over = Some(outcome);
        }
    }

    /// Apply an incoming online SnakeDelta to our local state. If the delta's
    /// tick doesn't follow the last applied tick we drop it and wait for the
    /// next authoritative `GameStateUpdate` snapshot to resync.
    pub fn on_snake_delta(&mut self, delta_data: &[u8]) {
        let Some(ClientGameState::Snake(state)) = self.game_state.as_mut() else {
            return;
        };
        let Ok(delta) = gc_shared::protocol::codec::decode::<SnakeDelta>(delta_data) else {
            return;
        };
        let expected = state.tick + 1;
        if delta.tick != expected {
            // Out of order — wait for the next snapshot.
            tracing::warn!(expected, got = delta.tick, "snake delta out of order, dropping");
            return;
        }

        // Apply deltas per arena. For multiplayer, delta.arenas contains one delta per arena.
        for arena_delta in &delta.arenas {
            let Some(arena) = state.arenas.iter_mut().find(|a| a.owner == arena_delta.owner) else {
                continue;
            };

            // 1) Heads / growth / tail.
            let grew: std::collections::HashSet<PlayerId> = arena_delta.grew.iter().copied().collect();
            for (pid, new_head) in &arena_delta.moves {
                if let Some(snake) = arena.snakes.iter_mut().find(|s| s.player_id == *pid) {
                    snake.body.push_front(*new_head);
                    if grew.contains(pid) {
                        snake.score += 1;
                    } else {
                        snake.body.pop_back();
                    }
                }
            }
            // 2) Deaths.
            for pid in &arena_delta.deaths {
                if let Some(snake) = arena.snakes.iter_mut().find(|s| s.player_id == *pid) {
                    snake.alive = false;
                }
            }
            // 3) Food churn.
            for eaten in &arena_delta.eaten_food {
                if let Some(idx) = arena.food.iter().position(|p| p == eaten) {
                    arena.food.remove(idx);
                }
            }
            for spawned in &arena_delta.new_food {
                arena.food.push(*spawned);
            }
        }

        state.tick = delta.tick;
        self.snake_last_applied_tick = Some(delta.tick);
        if let Some(outcome) = delta.game_over {
            state.game_over = Some(outcome.clone());
            self.game_over = Some(outcome);
        }
    }

    /// Apply a Checkers move locally in solo mode and let the bot respond.
    /// For online mode, caller sends the encoded move; server echoes state back
    /// and we reset our input so the next `on_game_state` cleanly applies.
    ///
    /// Returns `true` if the move was valid and applied (solo) or accepted for
    /// sending (online). On `false` the caller should surface an error.
    pub fn submit_checkers_move(&mut self, mv: CheckersMove) -> bool {
        let Some(state) = self.game_state.as_mut() else {
            return false;
        };
        let ClientGameState::Checkers(state) = state else {
            return false;
        };
        let Some(player_id) = self.my_player_id else {
            return false;
        };
        if Checkers::validate_move(state, player_id, &mv).is_err() {
            return false;
        }

        match self.game_mode {
            GameMode::Solo { difficulty } => {
                Checkers::apply_move(state, player_id, &mv);
                if let Some(outcome) = Checkers::is_terminal(state) {
                    self.game_over = Some(outcome);
                    self.checkers_input.reset();
                    return true;
                }
                // Bot response. Checkers Hard bot (depth=5 alpha-beta) typically
                // completes well under ~500 ms on mid-range hardware; call
                // synchronously on the TUI thread. TODO: if profiling shows it
                // slipping past that budget, dispatch via `spawn_blocking` and
                // post back via a new `NetEvent::SoloBotMove(CheckersMove)`.
                let bot_id = Checkers::current_player(state);
                let bot_mv = checkers::bot_move(state, difficulty);
                Checkers::apply_move(state, bot_id, &bot_mv);
                if let Some(outcome) = Checkers::is_terminal(state) {
                    self.game_over = Some(outcome);
                }
                self.checkers_input.reset();
                true
            }
            GameMode::Online => {
                // Server is authoritative. Caller encodes+sends; we just reset
                // the input so the authoritative state update applies cleanly.
                self.checkers_input.reset();
                true
            }
        }
    }

    // --- Checkers input state machine helpers ---

    /// Whether `(row, col)` is a dark (playable) checkers square.
    pub fn checkers_is_dark_square(row: u8, col: u8) -> bool {
        (row as usize + col as usize) % 2 == 1
    }

    /// Returns the side-to-move for the current Checkers state.
    fn checkers_current_side(state: &CheckersState) -> Side {
        // Mirrors `side_for_turn` in checkers.rs: turn 0 = Black, turn 1 = Red.
        if state.current_turn == 0 {
            Side::Black
        } else {
            Side::Red
        }
    }

    /// Handle an arrow-key direction in Checkers. Moves the cursor one square
    /// in the given direction, clamped to the board. All 64 squares are
    /// reachable; dark-square validation happens at Enter time.
    pub fn checkers_cursor_step(&mut self, drow: i32, dcol: i32) {
        let r = self.cursor_row as i32 + drow;
        let c = self.cursor_col as i32 + dcol;
        if (0..BOARD_SIZE as i32).contains(&r) && (0..BOARD_SIZE as i32).contains(&c) {
            self.cursor_row = r as u8;
            self.cursor_col = c as u8;
        }
    }

    /// Reset the Checkers input state and clear any transient error.
    pub fn checkers_cancel_selection(&mut self) {
        self.checkers_input.reset();
        self.status_message = None;
    }

    /// Handle `Enter` at the current cursor position for Checkers. Advances
    /// the input state machine, submitting the move when complete.
    /// Confirm the current input stage.
    ///
    /// Returns `Some(mv)` when the player completed a legal move — in solo
    /// mode the move is applied locally; in online mode the caller is
    /// responsible for sending it over the network. Returns `None` for
    /// partial progress (selected a piece, extended a chain) or on error.
    pub fn checkers_confirm(&mut self) -> Option<CheckersMove> {
        self.status_message = None;
        let Some(ClientGameState::Checkers(state)) = self.game_state.as_ref() else {
            return None;
        };
        let player_id = self.my_player_id?;
        if Checkers::current_player(state) != player_id {
            return None;
        }
        let side = Self::checkers_current_side(state);
        let cursor = Position {
            row: self.cursor_row,
            col: self.cursor_col,
        };

        match self.checkers_input.stage.clone() {
            CheckersInputStage::Idle => {
                if !Self::checkers_is_dark_square(cursor.row, cursor.col) {
                    return None;
                }
                let sq = state.board[cursor.row as usize][cursor.col as usize];
                let matches_side = matches!(
                    (sq, side),
                    (Square::Man(s) | Square::King(s), side2) if s == side2
                );
                if !matches_side {
                    self.status_message = Some(self.translator.get("checkers.error_own_piece").to_string());
                    return None;
                }
                self.checkers_input.stage = CheckersInputStage::TargetSelect;
                self.checkers_input.origin = Some(cursor);
                self.checkers_input.partial_path = vec![cursor];
                None
            }
            CheckersInputStage::TargetSelect | CheckersInputStage::Chaining => {
                if self.checkers_input.partial_path.last() == Some(&cursor) {
                    return None;
                }
                let mut tentative = self.checkers_input.partial_path.clone();
                tentative.push(cursor);

                let candidate = CheckersMove {
                    path: tentative.clone(),
                };
                if Checkers::validate_move(state, player_id, &candidate).is_ok() {
                    if matches!(self.game_mode, GameMode::Solo { .. }) {
                        self.submit_checkers_move(candidate.clone());
                    } else {
                        // Online: reset input; caller sends the move.
                        self.checkers_input.reset();
                    }
                    return Some(candidate);
                }

                let is_prefix = checkers::legal_moves(state).iter().any(|m| {
                    m.path.len() > tentative.len() && m.path[..tentative.len()] == tentative[..]
                });
                if is_prefix {
                    self.checkers_input.stage = CheckersInputStage::Chaining;
                    self.checkers_input.partial_path = tentative;
                } else {
                    self.status_message = Some(self.translator.get("checkers.error_illegal_target").to_string());
                    self.checkers_input.reset();
                }
                None
            }
        }
    }

    /// Apply a Chess move locally in solo mode and let the bot respond.
    /// Returns `true` if the move was valid and applied (solo) or accepted
    /// for sending (online).
    pub fn submit_chess_move(&mut self, mv: ChessMove) -> bool {
        let Some(state) = self.game_state.as_mut() else {
            return false;
        };
        let ClientGameState::Chess(state) = state else {
            return false;
        };
        let Some(player_id) = self.my_player_id else {
            return false;
        };
        match self.game_mode {
            GameMode::Solo { difficulty } => {
                if Chess::validate_move(state, player_id, &mv).is_err() {
                    return false;
                }
                Chess::apply_move(state, player_id, &mv);
                if let Some(outcome) = Chess::is_terminal(state) {
                    self.game_over = Some(outcome);
                    self.chess_input.reset();
                    return true;
                }
                let bot_id = Chess::current_player(state);
                if let Some(bot_mv) = chess::bot_move(state, difficulty) {
                    Chess::apply_move(state, bot_id, &bot_mv);
                    if let Some(outcome) = Chess::is_terminal(state) {
                        self.game_over = Some(outcome);
                    }
                }
                self.chess_input.reset();
                true
            }
            GameMode::Online => {
                self.chess_input.reset();
                true
            }
        }
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
        // If the opponent disconnects mid-rematch flow, clear rematch state.
        self.rematch_pending = false;
        self.rematch_incoming = false;
    }

    /// Called when a rematch is accepted: clear game-over state so the incoming
    /// GameStateUpdate starts the new game cleanly.
    pub fn on_rematch_accepted(&mut self) {
        self.game_over = None;
        self.rematch_pending = false;
        self.rematch_incoming = false;
        self.cursor_row = match self.current_game_type {
            GameType::Connect4 => 0,
            GameType::Checkers => 5,
            _ => 1,
        };
        self.cursor_col = match self.current_game_type {
            GameType::Connect4 => 3,
            GameType::Checkers => 0,
            _ => 1,
        };
        self.checkers_input.reset();
        self.chess_input.reset();
        self.status_message = None;
    }

    /// Called when a rematch is declined: clear flags and return to lobby.
    pub fn on_rematch_declined(&mut self) {
        self.on_room_left();
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
        // If player went first: player + bot = 2 moves.
        // If bot went first: bot (start) + player + bot = 3 moves.
        let player_went_first = app.solo_player_went_first.unwrap_or(true);
        let expected_moves = if player_went_first { 2 } else { 3 };
        match state {
            ClientGameState::TicTacToe(s) => assert_eq!(s.move_count, expected_moves),
            _ => panic!("expected TicTacToe state"),
        }
    }

    #[test]
    fn solo_rematch_resets_game() {
        let mut app = test_app();
        app.start_solo_game(Difficulty::Easy);
        // Play a move (may be rejected if bot went first and took that cell).
        // Either way, at least 1 move has been made (bot's opening or player's).
        app.play_solo_move(0, 0);
        match app.game_state.as_ref().unwrap() {
            ClientGameState::TicTacToe(s) => assert!(s.move_count >= 1),
            _ => panic!("expected TicTacToe state"),
        }

        // Simulate game over and rematch
        app.game_over = Some(GameOutcome::Draw);
        app.rematch_solo();

        assert!(app.game_over.is_none());
        // If player went first after rematch: move_count = 0.
        // If bot went first: bot already made its opening move, move_count = 1.
        let player_went_first = app.solo_player_went_first.unwrap_or(true);
        let expected_moves = if player_went_first { 0 } else { 1 };
        match app.game_state.as_ref().unwrap() {
            ClientGameState::TicTacToe(s) => assert_eq!(s.move_count, expected_moves),
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
        // If player went first: player + bot = 2 moves.
        // If bot went first: bot (start) + player + bot = 3 moves.
        let player_went_first = app.solo_player_went_first.unwrap_or(true);
        let expected_moves = if player_went_first { 2 } else { 3 };
        match app.game_state.as_ref().unwrap() {
            ClientGameState::Connect4(s) => assert_eq!(s.move_count, expected_moves),
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

    // --- Checkers tests ---

    /// Build a solo Checkers app where the player (Black) moves first.
    fn checkers_solo_app() -> App {
        let mut app = test_app();
        app.selected_game_type = GameType::Checkers;
        // Force player-goes-first on next start (alternates from Some(false)).
        app.solo_player_went_first = Some(false);
        app.start_solo_game(Difficulty::Easy);
        app
    }

    #[test]
    fn checkers_solo_start_sets_state_and_cursor() {
        let app = checkers_solo_app();
        assert_eq!(app.screen, Screen::InGame);
        assert!(matches!(app.game_state, Some(ClientGameState::Checkers(_))));
        // Initial cursor lands on a dark square (5, 0).
        assert_eq!(app.cursor_row, 5);
        assert_eq!(app.cursor_col, 0);
        assert!(App::checkers_is_dark_square(app.cursor_row, app.cursor_col));
        assert_eq!(app.checkers_input.stage, CheckersInputStage::Idle);
        assert!(app.checkers_input.partial_path.is_empty());
    }

    #[test]
    fn checkers_cursor_steps_one_square_and_clamps_to_board() {
        let mut app = checkers_solo_app();
        // Initial cursor is (5, 0). Step down -> (6, 0).
        app.checkers_cursor_step(1, 0);
        assert_eq!((app.cursor_row, app.cursor_col), (6, 0));
        // Left from column 0 is a no-op.
        app.checkers_cursor_step(0, -1);
        assert_eq!((app.cursor_row, app.cursor_col), (6, 0));
        // Step up + right lands on any square (light squares are reachable).
        app.checkers_cursor_step(-1, 1);
        assert_eq!((app.cursor_row, app.cursor_col), (5, 1));
    }

    #[test]
    fn checkers_input_idle_to_target_select_on_enter_over_own_piece() {
        let mut app = checkers_solo_app();
        // Cursor starts at (5,0) which in initial layout is a Black man.
        assert_eq!(app.checkers_input.stage, CheckersInputStage::Idle);
        app.checkers_confirm();
        assert_eq!(app.checkers_input.stage, CheckersInputStage::TargetSelect);
        assert_eq!(app.checkers_input.origin, Some(Position { row: 5, col: 0 }));
        assert_eq!(app.checkers_input.partial_path.len(), 1);
    }

    #[test]
    fn checkers_single_step_submits_and_alternates_turn() {
        let mut app = checkers_solo_app();
        // Select the Black man at (5, 0).
        app.checkers_confirm();
        assert_eq!(app.checkers_input.stage, CheckersInputStage::TargetSelect);

        // Move cursor to (4, 1) — diagonal forward for Black, legal step.
        app.cursor_row = 4;
        app.cursor_col = 1;
        let my_id = app.my_player_id.unwrap();
        app.checkers_confirm();

        // Move submitted, input reset.
        assert_eq!(app.checkers_input.stage, CheckersInputStage::Idle);
        assert!(app.checkers_input.partial_path.is_empty());

        // After our move the bot (or lack of moves) should have responded.
        // At minimum: our move applied. Verify board: (5,0) empty, (4,1) Black.
        let Some(ClientGameState::Checkers(ref state)) = app.game_state else {
            panic!("expected Checkers state");
        };
        assert_eq!(state.board[5][0], Square::Empty);
        assert_eq!(state.board[4][1], Square::Man(Side::Black));
        // Move count includes our move and the bot's response (at least 2).
        assert!(state.move_count >= 1);
        // It must be our turn again (bot moved) unless the game terminated.
        if app.game_over.is_none() {
            assert_eq!(Checkers::current_player(state), my_id);
        }
    }

    #[test]
    fn checkers_mandatory_capture_not_intercepted_by_non_capture() {
        let mut app = checkers_solo_app();
        // Force a state where Black must capture.
        let Some(ClientGameState::Checkers(ref mut state)) = app.game_state else {
            panic!("expected Checkers state");
        };
        state.board = [[Square::Empty; BOARD_SIZE]; BOARD_SIZE];
        state.board[5][0] = Square::Man(Side::Black);
        state.board[4][1] = Square::Man(Side::Red);
        // Also put a second Black man that could move non-capture: (5,2)->(4,3).
        state.board[5][2] = Square::Man(Side::Black);
        state.current_turn = 0; // Black to move.

        // Pick the non-capture origin (5,2).
        app.cursor_row = 5;
        app.cursor_col = 2;
        app.checkers_confirm();
        // Target (4,3) — would be a legal quiet step if no capture were pending.
        app.cursor_row = 4;
        app.cursor_col = 3;
        app.checkers_confirm();

        // Rejected: state reset, an error set, board unchanged for (5,2).
        assert_eq!(app.checkers_input.stage, CheckersInputStage::Idle);
        assert!(app.status_message.is_some());
        let Some(ClientGameState::Checkers(ref state)) = app.game_state else {
            panic!("expected Checkers state");
        };
        assert_eq!(state.board[5][2], Square::Man(Side::Black));
        assert_eq!(state.board[4][3], Square::Empty);
    }

    #[test]
    fn checkers_esc_cancels_selection() {
        let mut app = checkers_solo_app();
        app.checkers_confirm();
        assert_eq!(app.checkers_input.stage, CheckersInputStage::TargetSelect);
        app.checkers_cancel_selection();
        assert_eq!(app.checkers_input.stage, CheckersInputStage::Idle);
        assert!(app.checkers_input.origin.is_none());
        assert!(app.checkers_input.partial_path.is_empty());
    }

    #[test]
    fn g_key_cycle_includes_checkers_and_snake() {
        // Mirror the cycle in handle_lobby_key. Starts at TicTacToe.
        let mut g = GameType::TicTacToe;
        let cycle = |gt: GameType| -> GameType {
            match gt {
                GameType::TicTacToe => GameType::Connect4,
                GameType::Connect4 => GameType::Checkers,
                GameType::Checkers => GameType::Snake,
                GameType::Snake => GameType::TicTacToe,
                _ => GameType::TicTacToe,
            }
        };
        g = cycle(g);
        assert_eq!(g, GameType::Connect4);
        g = cycle(g);
        assert_eq!(g, GameType::Checkers);
        g = cycle(g);
        assert_eq!(g, GameType::Snake);
        g = cycle(g);
        assert_eq!(g, GameType::TicTacToe);
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

    #[test]
    fn snake_delta_out_of_order_drops_silently() {
        let mut app = test_app();
        let p0 = PlayerId::new();
        let p1 = PlayerId::new();
        app.my_player_id = Some(p0);
        app.game_mode = GameMode::Online;

        // Create initial snake state at tick 5
        let mut state = SnakeEngine::initial_state(&[p0, p1], &GameSettings::default());
        state.tick = 5;
        app.game_state = Some(ClientGameState::Snake(state));
        app.snake_last_applied_tick = Some(5);

        // Feed a delta with tick=5 (should be tick=6), which is out-of-order
        let delta = SnakeDelta {
            tick: 5,
            arenas: vec![],
            game_over: None,
        };
        let delta_data = gc_shared::protocol::codec::encode(&delta).unwrap();

        // Call on_snake_delta — it should drop the delta
        app.on_snake_delta(&delta_data);

        // Verify the state wasn't advanced (tick should still be 5)
        let Some(ClientGameState::Snake(ref state)) = app.game_state else {
            panic!("expected Snake state");
        };
        assert_eq!(state.tick, 5);
        assert_eq!(app.snake_last_applied_tick, Some(5));
    }
}
