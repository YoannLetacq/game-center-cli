use gc_shared::game::tictactoe::{TicTacToe, TicTacToeMove, TicTacToeState};
use gc_shared::game::traits::GameEngine;
use gc_shared::protocol::codec;
use gc_shared::protocol::messages::ServerMsg;
use gc_shared::types::{GameSettings, GameType, PlayerId};
use tracing::info;

/// Manages the state of a turn-based game in progress.
pub struct TurnBasedGame {
    pub game_type: GameType,
    pub state: GameState,
    pub players: Vec<PlayerId>,
    pub finished: bool,
}

/// Type-erased game state wrapper.
pub enum GameState {
    TicTacToe(TicTacToeState),
}

impl TurnBasedGame {
    /// Create a new game for the given type and players.
    pub fn new(game_type: GameType, players: &[PlayerId], settings: &GameSettings) -> Option<Self> {
        let state = match game_type {
            GameType::TicTacToe => {
                GameState::TicTacToe(TicTacToe::initial_state(players, settings))
            }
            // Other turn-based games will be added in Phase 5
            _ => return None,
        };

        Some(Self {
            game_type,
            state,
            players: players.to_vec(),
            finished: false,
        })
    }

    /// Process a game action (move) from a player.
    /// Returns: (response for the acting player, broadcasts for other players).
    pub fn apply_action(
        &mut self,
        player: PlayerId,
        action_data: &[u8],
    ) -> (ServerMsg, Vec<(PlayerId, ServerMsg)>) {
        let GameState::TicTacToe(ref mut state) = self.state;

        // Decode the move
        let mv: TicTacToeMove = match codec::decode(action_data) {
            Ok(m) => m,
            Err(e) => {
                return (
                    ServerMsg::Error {
                        code: 400,
                        message: format!("invalid move data: {e}"),
                    },
                    Vec::new(),
                );
            }
        };

        // Validate
        if let Err(reason) = TicTacToe::validate_move(state, player, &mv) {
            return (
                ServerMsg::Error {
                    code: 400,
                    message: reason,
                },
                Vec::new(),
            );
        }

        // Apply move in-place (no clone)
        TicTacToe::apply_move(state, player, &mv);

        // Encode state for broadcast
        let state_bytes = codec::encode(&state).unwrap_or_default();
        let tick = state.move_count as u64;

        // Build the state update message (same for response and broadcasts)
        let state_msg = ServerMsg::GameStateUpdate {
            tick,
            state_data: state_bytes,
        };

        // Broadcasts go to OTHER players only (acting player gets the direct response)
        let others: Vec<PlayerId> = self
            .players
            .iter()
            .filter(|&&pid| pid != player)
            .copied()
            .collect();

        let mut broadcasts: Vec<(PlayerId, ServerMsg)> =
            others.iter().map(|&pid| (pid, state_msg.clone())).collect();

        // Check for game over
        if let Some(outcome) = TicTacToe::is_terminal(state) {
            self.finished = true;
            info!(game_type = ?self.game_type, ?outcome, "game finished");

            // Send GameOver to ALL players (acting player via broadcast too)
            for &pid in &self.players {
                broadcasts.push((
                    pid,
                    ServerMsg::GameOver {
                        outcome: outcome.clone(),
                    },
                ));
            }
        }

        (state_msg, broadcasts)
    }

    /// Get the current player whose turn it is.
    #[allow(dead_code)]
    pub fn current_player(&self) -> Option<PlayerId> {
        match &self.state {
            GameState::TicTacToe(state) => Some(TicTacToe::current_player(state)),
        }
    }

    /// Get the serialized game state.
    pub fn encode_state(&self) -> Vec<u8> {
        match &self.state {
            GameState::TicTacToe(state) => codec::encode(state).unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gc_shared::game::tictactoe::TicTacToeMove;

    fn make_game() -> (TurnBasedGame, PlayerId, PlayerId) {
        let p0 = PlayerId::new();
        let p1 = PlayerId::new();
        let game =
            TurnBasedGame::new(GameType::TicTacToe, &[p0, p1], &GameSettings::default()).unwrap();
        (game, p0, p1)
    }

    fn encode_move(row: u8, col: u8) -> Vec<u8> {
        codec::encode(&TicTacToeMove { row, col }).unwrap()
    }

    #[test]
    fn create_game() {
        let (game, p0, _) = make_game();
        assert!(!game.finished);
        assert_eq!(game.current_player(), Some(p0));
    }

    #[test]
    fn valid_move_broadcasts_state() {
        let (mut game, p0, _) = make_game();
        let action = encode_move(1, 1);
        let (resp, broadcasts) = game.apply_action(p0, &action);

        match resp {
            ServerMsg::GameStateUpdate { tick, .. } => assert_eq!(tick, 1),
            other => panic!("expected GameStateUpdate, got {other:?}"),
        }

        // Only the OTHER player gets the broadcast (acting player gets the direct response)
        assert_eq!(broadcasts.len(), 1);
    }

    #[test]
    fn invalid_move_returns_error() {
        let (mut game, _, p1) = make_game();
        let action = encode_move(0, 0);
        let (resp, broadcasts) = game.apply_action(p1, &action); // wrong turn

        match resp {
            ServerMsg::Error { code: 400, .. } => {}
            other => panic!("expected error, got {other:?}"),
        }
        assert!(broadcasts.is_empty());
    }

    #[test]
    fn game_over_sends_outcome() {
        let (mut game, p0, p1) = make_game();

        // Play to X wins: row 0
        game.apply_action(p0, &encode_move(0, 0));
        game.apply_action(p1, &encode_move(1, 0));
        game.apply_action(p0, &encode_move(0, 1));
        game.apply_action(p1, &encode_move(1, 1));
        let (_, broadcasts) = game.apply_action(p0, &encode_move(0, 2)); // X wins

        assert!(game.finished);

        // 1 state update (to other player) + 2 game overs (to both) = 3 broadcasts
        let game_overs: Vec<_> = broadcasts
            .iter()
            .filter(|(_, msg)| matches!(msg, ServerMsg::GameOver { .. }))
            .collect();
        assert_eq!(game_overs.len(), 2);

        let state_updates: Vec<_> = broadcasts
            .iter()
            .filter(|(_, msg)| matches!(msg, ServerMsg::GameStateUpdate { .. }))
            .collect();
        assert_eq!(state_updates.len(), 1); // only to other player
        assert_eq!(broadcasts.len(), 3);
    }
}
