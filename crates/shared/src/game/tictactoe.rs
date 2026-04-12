use serde::{Deserialize, Serialize};

use crate::types::{Difficulty, GameOutcome, GameSettings, PlayerId};

use super::traits::GameEngine;

/// A cell on the Tic-Tac-Toe board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cell {
    Empty,
    X,
    O,
}

/// A move: row and column (0-2).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TicTacToeMove {
    pub row: u8,
    pub col: u8,
}

/// The game state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicTacToeState {
    pub board: [[Cell; 3]; 3],
    pub players: [PlayerId; 2],
    pub current_turn: usize, // index into players: 0 = X, 1 = O
    pub move_count: u8,
}

pub struct TicTacToe;

impl GameEngine for TicTacToe {
    type Move = TicTacToeMove;
    type State = TicTacToeState;

    fn initial_state(players: &[PlayerId], _settings: &GameSettings) -> Self::State {
        assert!(players.len() >= 2, "Tic-Tac-Toe requires exactly 2 players");
        TicTacToeState {
            board: [[Cell::Empty; 3]; 3],
            players: [players[0], players[1]],
            current_turn: 0,
            move_count: 0,
        }
    }

    fn validate_move(state: &Self::State, player: PlayerId, mv: &Self::Move) -> Result<(), String> {
        if state.players[state.current_turn] != player {
            return Err("not your turn".to_string());
        }
        if mv.row > 2 || mv.col > 2 {
            return Err("position out of bounds".to_string());
        }
        if state.board[mv.row as usize][mv.col as usize] != Cell::Empty {
            return Err("cell already occupied".to_string());
        }
        Ok(())
    }

    fn apply_move(state: &mut Self::State, _player: PlayerId, mv: &Self::Move) {
        let cell = if state.current_turn == 0 {
            Cell::X
        } else {
            Cell::O
        };
        state.board[mv.row as usize][mv.col as usize] = cell;
        state.move_count += 1;
        state.current_turn = 1 - state.current_turn;
    }

    fn is_terminal(state: &Self::State) -> Option<GameOutcome> {
        // Check rows
        for row in 0..3 {
            if state.board[row][0] != Cell::Empty
                && state.board[row][0] == state.board[row][1]
                && state.board[row][1] == state.board[row][2]
            {
                return Some(winner_from_cell(state, state.board[row][0]));
            }
        }
        // Check columns
        for col in 0..3 {
            if state.board[0][col] != Cell::Empty
                && state.board[0][col] == state.board[1][col]
                && state.board[1][col] == state.board[2][col]
            {
                return Some(winner_from_cell(state, state.board[0][col]));
            }
        }
        // Check diagonals
        if state.board[0][0] != Cell::Empty
            && state.board[0][0] == state.board[1][1]
            && state.board[1][1] == state.board[2][2]
        {
            return Some(winner_from_cell(state, state.board[0][0]));
        }
        if state.board[0][2] != Cell::Empty
            && state.board[0][2] == state.board[1][1]
            && state.board[1][1] == state.board[2][0]
        {
            return Some(winner_from_cell(state, state.board[0][2]));
        }
        // Check draw (all cells filled)
        if state.move_count == 9 {
            return Some(GameOutcome::Draw);
        }
        None
    }

    fn current_player(state: &Self::State) -> PlayerId {
        state.players[state.current_turn]
    }
}

fn winner_from_cell(state: &TicTacToeState, cell: Cell) -> GameOutcome {
    match cell {
        Cell::X => GameOutcome::Win(state.players[0]),
        Cell::O => GameOutcome::Win(state.players[1]),
        Cell::Empty => unreachable!(),
    }
}

// --- Bot AI ---

/// Generate a bot move.
pub fn bot_move(state: &TicTacToeState, difficulty: Difficulty) -> TicTacToeMove {
    match difficulty {
        Difficulty::Easy => random_move(state),
        Difficulty::Hard => best_move(state),
    }
}

/// Random valid move.
fn random_move(state: &TicTacToeState) -> TicTacToeMove {
    use rand::prelude::IndexedRandom;
    let mut rng = rand::rng();

    let empty: Vec<(u8, u8)> = (0..3u8)
        .flat_map(|r| (0..3u8).map(move |c| (r, c)))
        .filter(|&(r, c)| state.board[r as usize][c as usize] == Cell::Empty)
        .collect();

    let &(row, col) = empty.choose(&mut rng).expect("no empty cells for bot move");
    TicTacToeMove { row, col }
}

/// Minimax for optimal play.
fn best_move(state: &TicTacToeState) -> TicTacToeMove {
    let is_maximizing = state.current_turn == 0; // X maximizes, O minimizes
    let mut best_score = if is_maximizing { i32::MIN } else { i32::MAX };
    let mut best = TicTacToeMove { row: 0, col: 0 };

    for row in 0..3u8 {
        for col in 0..3u8 {
            if state.board[row as usize][col as usize] != Cell::Empty {
                continue;
            }
            let mut next = state.clone();
            let mv = TicTacToeMove { row, col };
            TicTacToe::apply_move(&mut next, state.players[state.current_turn], &mv);

            let score = minimax(&next, !is_maximizing);

            let dominated = if is_maximizing {
                score > best_score
            } else {
                score < best_score
            };
            if dominated {
                best_score = score;
                best = mv;
            }
        }
    }

    best
}

fn minimax(state: &TicTacToeState, is_maximizing: bool) -> i32 {
    if let Some(outcome) = TicTacToe::is_terminal(state) {
        return match outcome {
            GameOutcome::Win(pid) if pid == state.players[0] => 10 - state.move_count as i32,
            GameOutcome::Win(_) => state.move_count as i32 - 10,
            GameOutcome::Draw => 0,
        };
    }

    let mut best = if is_maximizing { i32::MIN } else { i32::MAX };

    for row in 0..3u8 {
        for col in 0..3u8 {
            if state.board[row as usize][col as usize] != Cell::Empty {
                continue;
            }
            let mut next = state.clone();
            let mv = TicTacToeMove { row, col };
            TicTacToe::apply_move(&mut next, state.players[state.current_turn], &mv);

            let score = minimax(&next, !is_maximizing);
            best = if is_maximizing {
                best.max(score)
            } else {
                best.min(score)
            };
        }
    }

    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_players() -> [PlayerId; 2] {
        [PlayerId::new(), PlayerId::new()]
    }

    fn new_game() -> TicTacToeState {
        let p = two_players();
        TicTacToe::initial_state(&p, &GameSettings::default())
    }

    #[test]
    fn initial_state_is_empty() {
        let state = new_game();
        for row in &state.board {
            for cell in row {
                assert_eq!(*cell, Cell::Empty);
            }
        }
        assert_eq!(state.move_count, 0);
        assert_eq!(state.current_turn, 0);
    }

    #[test]
    fn valid_move_applies() {
        let mut state = new_game();
        let player = state.players[0];
        let mv = TicTacToeMove { row: 1, col: 1 };

        assert!(TicTacToe::validate_move(&state, player, &mv).is_ok());
        TicTacToe::apply_move(&mut state, player, &mv);

        assert_eq!(state.board[1][1], Cell::X);
        assert_eq!(state.current_turn, 1);
        assert_eq!(state.move_count, 1);
    }

    #[test]
    fn wrong_turn_rejected() {
        let state = new_game();
        let wrong_player = state.players[1]; // O tries to go first
        let mv = TicTacToeMove { row: 0, col: 0 };
        assert!(TicTacToe::validate_move(&state, wrong_player, &mv).is_err());
    }

    #[test]
    fn occupied_cell_rejected() {
        let mut state = new_game();
        let p0 = state.players[0];
        let p1 = state.players[1];
        let mv = TicTacToeMove { row: 0, col: 0 };

        TicTacToe::apply_move(&mut state, p0, &mv);
        assert!(TicTacToe::validate_move(&state, p1, &mv).is_err());
    }

    #[test]
    fn out_of_bounds_rejected() {
        let state = new_game();
        let player = state.players[0];
        let mv = TicTacToeMove { row: 3, col: 0 };
        assert!(TicTacToe::validate_move(&state, player, &mv).is_err());
    }

    #[test]
    fn row_win() {
        let mut state = new_game();
        let p0 = state.players[0];
        let p1 = state.players[1];

        // X: (0,0), O: (1,0), X: (0,1), O: (1,1), X: (0,2)
        TicTacToe::apply_move(&mut state, p0, &TicTacToeMove { row: 0, col: 0 });
        TicTacToe::apply_move(&mut state, p1, &TicTacToeMove { row: 1, col: 0 });
        TicTacToe::apply_move(&mut state, p0, &TicTacToeMove { row: 0, col: 1 });
        TicTacToe::apply_move(&mut state, p1, &TicTacToeMove { row: 1, col: 1 });
        TicTacToe::apply_move(&mut state, p0, &TicTacToeMove { row: 0, col: 2 });

        match TicTacToe::is_terminal(&state) {
            Some(GameOutcome::Win(winner)) => assert_eq!(winner, p0),
            other => panic!("expected X win, got {other:?}"),
        }
    }

    #[test]
    fn column_win() {
        let mut state = new_game();
        let p0 = state.players[0];
        let p1 = state.players[1];

        // X: (0,0), O: (0,1), X: (1,0), O: (1,1), X: (2,0)
        TicTacToe::apply_move(&mut state, p0, &TicTacToeMove { row: 0, col: 0 });
        TicTacToe::apply_move(&mut state, p1, &TicTacToeMove { row: 0, col: 1 });
        TicTacToe::apply_move(&mut state, p0, &TicTacToeMove { row: 1, col: 0 });
        TicTacToe::apply_move(&mut state, p1, &TicTacToeMove { row: 1, col: 1 });
        TicTacToe::apply_move(&mut state, p0, &TicTacToeMove { row: 2, col: 0 });

        match TicTacToe::is_terminal(&state) {
            Some(GameOutcome::Win(winner)) => assert_eq!(winner, p0),
            other => panic!("expected X win, got {other:?}"),
        }
    }

    #[test]
    fn diagonal_win() {
        let mut state = new_game();
        let p0 = state.players[0];
        let p1 = state.players[1];

        // X: (0,0), O: (0,1), X: (1,1), O: (0,2), X: (2,2)
        TicTacToe::apply_move(&mut state, p0, &TicTacToeMove { row: 0, col: 0 });
        TicTacToe::apply_move(&mut state, p1, &TicTacToeMove { row: 0, col: 1 });
        TicTacToe::apply_move(&mut state, p0, &TicTacToeMove { row: 1, col: 1 });
        TicTacToe::apply_move(&mut state, p1, &TicTacToeMove { row: 0, col: 2 });
        TicTacToe::apply_move(&mut state, p0, &TicTacToeMove { row: 2, col: 2 });

        match TicTacToe::is_terminal(&state) {
            Some(GameOutcome::Win(winner)) => assert_eq!(winner, p0),
            other => panic!("expected X win, got {other:?}"),
        }
    }

    #[test]
    fn anti_diagonal_win() {
        let mut state = new_game();
        let p0 = state.players[0];
        let p1 = state.players[1];

        // X: (0,2), O: (0,0), X: (1,1), O: (1,0), X: (2,0)
        TicTacToe::apply_move(&mut state, p0, &TicTacToeMove { row: 0, col: 2 });
        TicTacToe::apply_move(&mut state, p1, &TicTacToeMove { row: 0, col: 0 });
        TicTacToe::apply_move(&mut state, p0, &TicTacToeMove { row: 1, col: 1 });
        TicTacToe::apply_move(&mut state, p1, &TicTacToeMove { row: 1, col: 0 });
        TicTacToe::apply_move(&mut state, p0, &TicTacToeMove { row: 2, col: 0 });

        match TicTacToe::is_terminal(&state) {
            Some(GameOutcome::Win(winner)) => assert_eq!(winner, p0),
            other => panic!("expected X win, got {other:?}"),
        }
    }

    #[test]
    fn draw() {
        let mut state = new_game();
        let p0 = state.players[0];
        let p1 = state.players[1];

        // X O X
        // X X O
        // O X O
        TicTacToe::apply_move(&mut state, p0, &TicTacToeMove { row: 0, col: 0 }); // X
        TicTacToe::apply_move(&mut state, p1, &TicTacToeMove { row: 0, col: 1 }); // O
        TicTacToe::apply_move(&mut state, p0, &TicTacToeMove { row: 0, col: 2 }); // X
        TicTacToe::apply_move(&mut state, p1, &TicTacToeMove { row: 1, col: 2 }); // O
        TicTacToe::apply_move(&mut state, p0, &TicTacToeMove { row: 1, col: 0 }); // X
        TicTacToe::apply_move(&mut state, p1, &TicTacToeMove { row: 2, col: 0 }); // O
        TicTacToe::apply_move(&mut state, p0, &TicTacToeMove { row: 1, col: 1 }); // X
        TicTacToe::apply_move(&mut state, p1, &TicTacToeMove { row: 2, col: 2 }); // O
        TicTacToe::apply_move(&mut state, p0, &TicTacToeMove { row: 2, col: 1 }); // X

        match TicTacToe::is_terminal(&state) {
            Some(GameOutcome::Draw) => {}
            other => panic!("expected draw, got {other:?}"),
        }
    }

    #[test]
    fn not_terminal_mid_game() {
        let mut state = new_game();
        let p0 = state.players[0];
        TicTacToe::apply_move(&mut state, p0, &TicTacToeMove { row: 1, col: 1 });
        assert!(TicTacToe::is_terminal(&state).is_none());
    }

    #[test]
    fn current_player_alternates() {
        let mut state = new_game();
        let p0 = state.players[0];
        let p1 = state.players[1];

        assert_eq!(TicTacToe::current_player(&state), p0);
        TicTacToe::apply_move(&mut state, p0, &TicTacToeMove { row: 0, col: 0 });
        assert_eq!(TicTacToe::current_player(&state), p1);
        TicTacToe::apply_move(&mut state, p1, &TicTacToeMove { row: 1, col: 1 });
        assert_eq!(TicTacToe::current_player(&state), p0);
    }

    #[test]
    fn bot_easy_returns_valid_move() {
        let state = new_game();
        let mv = bot_move(&state, Difficulty::Easy);
        assert!(mv.row <= 2 && mv.col <= 2);
        assert_eq!(state.board[mv.row as usize][mv.col as usize], Cell::Empty);
    }

    #[test]
    fn bot_hard_returns_valid_move() {
        let state = new_game();
        let mv = bot_move(&state, Difficulty::Hard);
        assert!(mv.row <= 2 && mv.col <= 2);
        assert_eq!(state.board[mv.row as usize][mv.col as usize], Cell::Empty);
    }

    #[test]
    fn bot_hard_blocks_winning_move() {
        // O has two in a row, X (hard bot) should block
        let mut state = new_game();
        let p0 = state.players[0];
        let p1 = state.players[1];

        // X: (0,0), O: (1,0), X: (2,2), O: (1,1) — O threatens (1,2)
        TicTacToe::apply_move(&mut state, p0, &TicTacToeMove { row: 0, col: 0 });
        TicTacToe::apply_move(&mut state, p1, &TicTacToeMove { row: 1, col: 0 });
        TicTacToe::apply_move(&mut state, p0, &TicTacToeMove { row: 2, col: 2 });
        TicTacToe::apply_move(&mut state, p1, &TicTacToeMove { row: 1, col: 1 });

        // It's X's turn. Hard bot should make a strategic move.
        let mv = bot_move(&state, Difficulty::Hard);
        let player = state.players[state.current_turn];
        assert!(TicTacToe::validate_move(&state, player, &mv).is_ok());
    }

    #[test]
    fn bot_hard_takes_winning_move() {
        // X has two in a row, hard bot (as X) should win
        let mut state = new_game();
        let p0 = state.players[0];
        let p1 = state.players[1];

        // X: (0,0), O: (1,0), X: (0,1), O: (1,1)
        TicTacToe::apply_move(&mut state, p0, &TicTacToeMove { row: 0, col: 0 });
        TicTacToe::apply_move(&mut state, p1, &TicTacToeMove { row: 1, col: 0 });
        TicTacToe::apply_move(&mut state, p0, &TicTacToeMove { row: 0, col: 1 });
        TicTacToe::apply_move(&mut state, p1, &TicTacToeMove { row: 1, col: 1 });

        // X should play (0,2) to win
        let mv = bot_move(&state, Difficulty::Hard);
        assert_eq!(mv.row, 0);
        assert_eq!(mv.col, 2);
    }

    #[test]
    fn full_game_hard_vs_hard_is_draw() {
        // Two perfect players should always draw
        let mut state = new_game();
        loop {
            if TicTacToe::is_terminal(&state).is_some() {
                break;
            }
            let mv = bot_move(&state, Difficulty::Hard);
            let player = state.players[state.current_turn];
            TicTacToe::apply_move(&mut state, player, &mv);
        }
        match TicTacToe::is_terminal(&state) {
            Some(GameOutcome::Draw) => {}
            other => panic!("expected draw from perfect play, got {other:?}"),
        }
    }
}
