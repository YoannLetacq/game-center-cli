use serde::{Serialize, de::DeserializeOwned};
use std::collections::HashMap;
use std::fmt::Debug;

use crate::types::{GameOutcome, GameSettings, PlayerId};

/// Trait for turn-based game engines.
///
/// The server and client both use this to validate and apply moves.
/// The server is authoritative — it validates every move before broadcasting.
pub trait GameEngine: Send + Sync + 'static {
    type Move: Serialize + DeserializeOwned + Clone + Debug;
    type State: Serialize + DeserializeOwned + Clone + Debug;

    /// Create the initial game state for the given players and settings.
    fn initial_state(players: &[PlayerId], settings: &GameSettings) -> Self::State;

    /// Validate whether a move is legal in the current state for the given player.
    fn validate_move(state: &Self::State, player: PlayerId, mv: &Self::Move) -> Result<(), String>;

    /// Apply a validated move to the state. Caller must validate first.
    fn apply_move(state: &mut Self::State, player: PlayerId, mv: &Self::Move);

    /// Check if the game has ended. Returns the outcome if terminal.
    fn is_terminal(state: &Self::State) -> Option<GameOutcome>;

    /// Get the player whose turn it is.
    fn current_player(state: &Self::State) -> PlayerId;
}

/// Trait for real-time game engines (Snake, Pacman, Block Breaker).
///
/// These run on a server-driven tick loop rather than alternating turns.
pub trait RealtimeGameEngine: Send + Sync + 'static {
    type Input: Serialize + DeserializeOwned + Clone + Debug;
    type State: Serialize + DeserializeOwned + Clone + Debug;
    type Delta: Serialize + DeserializeOwned + Clone + Debug;

    /// Create the initial game state.
    fn initial_state(players: &[PlayerId], settings: &GameSettings) -> Self::State;

    /// Advance the game by one tick, applying buffered player inputs.
    /// Returns a delta describing what changed.
    fn tick(state: &mut Self::State, inputs: &HashMap<PlayerId, Self::Input>) -> Self::Delta;

    /// Check if the game has ended.
    fn is_terminal(state: &Self::State) -> Option<GameOutcome>;

    /// Take a full snapshot of the state (for periodic sync / reconnection).
    fn snapshot(state: &Self::State) -> Self::State;
}
