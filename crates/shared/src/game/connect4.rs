use serde::{Deserialize, Serialize};

use crate::types::{Difficulty, GameOutcome, GameSettings, PlayerId};

use super::traits::GameEngine;

pub const ROWS: usize = 6;
pub const COLS: usize = 7;

/// A cell on the Connect 4 board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cell {
    Empty,
    Red,
    Yellow,
}

/// A move: which column to drop a piece into (0-6).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Connect4Move {
    pub col: u8,
}

/// The game state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connect4State {
    /// board[row][col], row 0 = bottom.
    pub board: [[Cell; COLS]; ROWS],
    pub players: [PlayerId; 2],
    /// Index into players: 0 = Red, 1 = Yellow.
    pub current_turn: usize,
    pub move_count: u8,
}

pub struct Connect4;

impl GameEngine for Connect4 {
    type Move = Connect4Move;
    type State = Connect4State;

    fn initial_state(players: &[PlayerId], _settings: &GameSettings) -> Self::State {
        assert!(players.len() >= 2, "Connect 4 requires exactly 2 players");
        Connect4State {
            board: [[Cell::Empty; COLS]; ROWS],
            players: [players[0], players[1]],
            current_turn: 0,
            move_count: 0,
        }
    }

    fn validate_move(state: &Self::State, player: PlayerId, mv: &Self::Move) -> Result<(), String> {
        if state.players[state.current_turn] != player {
            return Err("not your turn".to_string());
        }
        if mv.col as usize >= COLS {
            return Err("column out of bounds".to_string());
        }
        // Check if column is full (top row occupied)
        if state.board[ROWS - 1][mv.col as usize] != Cell::Empty {
            return Err("column is full".to_string());
        }
        Ok(())
    }

    fn apply_move(state: &mut Self::State, _player: PlayerId, mv: &Self::Move) {
        let cell = if state.current_turn == 0 {
            Cell::Red
        } else {
            Cell::Yellow
        };
        // Find the lowest empty row in this column
        for row in 0..ROWS {
            if state.board[row][mv.col as usize] == Cell::Empty {
                state.board[row][mv.col as usize] = cell;
                break;
            }
        }
        state.move_count += 1;
        state.current_turn = 1 - state.current_turn;
    }

    fn is_terminal(state: &Self::State) -> Option<GameOutcome> {
        // Check all four-in-a-row directions
        for row in 0..ROWS {
            for col in 0..COLS {
                let cell = state.board[row][col];
                if cell == Cell::Empty {
                    continue;
                }
                // Horizontal
                if col + 3 < COLS
                    && cell == state.board[row][col + 1]
                    && cell == state.board[row][col + 2]
                    && cell == state.board[row][col + 3]
                {
                    return Some(winner_from_cell(state, cell));
                }
                // Vertical
                if row + 3 < ROWS
                    && cell == state.board[row + 1][col]
                    && cell == state.board[row + 2][col]
                    && cell == state.board[row + 3][col]
                {
                    return Some(winner_from_cell(state, cell));
                }
                // Diagonal up-right
                if row + 3 < ROWS
                    && col + 3 < COLS
                    && cell == state.board[row + 1][col + 1]
                    && cell == state.board[row + 2][col + 2]
                    && cell == state.board[row + 3][col + 3]
                {
                    return Some(winner_from_cell(state, cell));
                }
                // Diagonal up-left
                if row + 3 < ROWS
                    && col >= 3
                    && cell == state.board[row + 1][col - 1]
                    && cell == state.board[row + 2][col - 2]
                    && cell == state.board[row + 3][col - 3]
                {
                    return Some(winner_from_cell(state, cell));
                }
            }
        }
        // Draw: board full
        if state.move_count as usize == ROWS * COLS {
            return Some(GameOutcome::Draw);
        }
        None
    }

    fn current_player(state: &Self::State) -> PlayerId {
        state.players[state.current_turn]
    }
}

fn winner_from_cell(state: &Connect4State, cell: Cell) -> GameOutcome {
    match cell {
        Cell::Red => GameOutcome::Win(state.players[0]),
        Cell::Yellow => GameOutcome::Win(state.players[1]),
        Cell::Empty => unreachable!(),
    }
}

// --- Bot AI ---

/// Generate a bot move using the given difficulty.
pub fn bot_move(state: &Connect4State, difficulty: Difficulty) -> Connect4Move {
    match difficulty {
        Difficulty::Easy => random_move(state),
        Difficulty::Hard => best_move(state),
    }
}

/// Random valid move.
fn random_move(state: &Connect4State) -> Connect4Move {
    use rand::prelude::IndexedRandom;
    let mut rng = rand::rng();

    let valid: Vec<u8> = (0..COLS as u8)
        .filter(|&c| state.board[ROWS - 1][c as usize] == Cell::Empty)
        .collect();

    let &col = valid
        .choose(&mut rng)
        .expect("no valid columns for bot move");
    Connect4Move { col }
}

/// Alpha-beta pruning at depth 6 (1 ply in this outer loop + 5 remaining in recursion).
fn best_move(state: &Connect4State) -> Connect4Move {
    let is_maximizing = state.current_turn == 0;
    let mut best_score = if is_maximizing { i32::MIN } else { i32::MAX };
    let mut best_col = 0u8;

    // Evaluate columns from center outward for better pruning
    let col_order: [usize; 7] = [3, 2, 4, 1, 5, 0, 6];

    for &col in &col_order {
        if state.board[ROWS - 1][col] != Cell::Empty {
            continue;
        }
        let mut next = state.clone();
        let mv = Connect4Move { col: col as u8 };
        Connect4::apply_move(&mut next, state.players[state.current_turn], &mv);

        let score = alpha_beta(&next, 5, i32::MIN, i32::MAX, !is_maximizing);

        let dominated = if is_maximizing {
            score > best_score
        } else {
            score < best_score
        };
        if dominated {
            best_score = score;
            best_col = col as u8;
        }
    }

    Connect4Move { col: best_col }
}

fn alpha_beta(
    state: &Connect4State,
    depth: u8,
    mut alpha: i32,
    mut beta: i32,
    is_maximizing: bool,
) -> i32 {
    if let Some(outcome) = Connect4::is_terminal(state) {
        return match outcome {
            GameOutcome::Win(pid) if pid == state.players[0] => 1000 + depth as i32,
            GameOutcome::Win(_) => -(1000 + depth as i32),
            GameOutcome::Draw => 0,
        };
    }
    if depth == 0 {
        return evaluate(state);
    }

    let col_order: [usize; 7] = [3, 2, 4, 1, 5, 0, 6];

    if is_maximizing {
        let mut value = i32::MIN;
        for &col in &col_order {
            if state.board[ROWS - 1][col] != Cell::Empty {
                continue;
            }
            let mut next = state.clone();
            Connect4::apply_move(
                &mut next,
                state.players[state.current_turn],
                &Connect4Move { col: col as u8 },
            );
            let score = alpha_beta(&next, depth - 1, alpha, beta, false);
            value = value.max(score);
            alpha = alpha.max(score);
            if alpha >= beta {
                break;
            }
        }
        value
    } else {
        let mut value = i32::MAX;
        for &col in &col_order {
            if state.board[ROWS - 1][col] != Cell::Empty {
                continue;
            }
            let mut next = state.clone();
            Connect4::apply_move(
                &mut next,
                state.players[state.current_turn],
                &Connect4Move { col: col as u8 },
            );
            let score = alpha_beta(&next, depth - 1, alpha, beta, true);
            value = value.min(score);
            beta = beta.min(score);
            if alpha >= beta {
                break;
            }
        }
        value
    }
}

/// Static evaluation: score windows of 4 cells.
fn evaluate(state: &Connect4State) -> i32 {
    let mut score = 0i32;

    // Score center column (positional advantage)
    for row in 0..ROWS {
        if state.board[row][3] == Cell::Red {
            score += 3;
        } else if state.board[row][3] == Cell::Yellow {
            score -= 3;
        }
    }

    // Score all windows of 4
    for row in 0..ROWS {
        for col in 0..COLS {
            // Horizontal
            if col + 3 < COLS {
                score += score_window(
                    state.board[row][col],
                    state.board[row][col + 1],
                    state.board[row][col + 2],
                    state.board[row][col + 3],
                );
            }
            // Vertical
            if row + 3 < ROWS {
                score += score_window(
                    state.board[row][col],
                    state.board[row + 1][col],
                    state.board[row + 2][col],
                    state.board[row + 3][col],
                );
            }
            // Diagonal up-right
            if row + 3 < ROWS && col + 3 < COLS {
                score += score_window(
                    state.board[row][col],
                    state.board[row + 1][col + 1],
                    state.board[row + 2][col + 2],
                    state.board[row + 3][col + 3],
                );
            }
            // Diagonal up-left
            if row + 3 < ROWS && col >= 3 {
                score += score_window(
                    state.board[row][col],
                    state.board[row + 1][col - 1],
                    state.board[row + 2][col - 2],
                    state.board[row + 3][col - 3],
                );
            }
        }
    }

    score
}

/// Score a window of 4 cells. Positive = Red advantage, negative = Yellow advantage.
fn score_window(a: Cell, b: Cell, c: Cell, d: Cell) -> i32 {
    let cells = [a, b, c, d];
    let red = cells.iter().filter(|&&c| c == Cell::Red).count();
    let yellow = cells.iter().filter(|&&c| c == Cell::Yellow).count();

    if red > 0 && yellow > 0 {
        return 0; // Mixed window, no value
    }

    (match red {
        4 => 100,
        3 => 5,
        2 => 2,
        _ => 0,
    }) - match yellow {
        4 => 100,
        3 => 5,
        2 => 2,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_players() -> [PlayerId; 2] {
        [PlayerId::new(), PlayerId::new()]
    }

    fn new_game() -> Connect4State {
        let p = two_players();
        Connect4::initial_state(&p, &GameSettings::default())
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
    fn piece_falls_to_bottom() {
        let mut state = new_game();
        let p0 = state.players[0];
        Connect4::apply_move(&mut state, p0, &Connect4Move { col: 3 });
        // Should land on row 0
        assert_eq!(state.board[0][3], Cell::Red);
        assert_eq!(state.board[1][3], Cell::Empty);
    }

    #[test]
    fn pieces_stack() {
        let mut state = new_game();
        let p0 = state.players[0];
        let p1 = state.players[1];
        Connect4::apply_move(&mut state, p0, &Connect4Move { col: 3 });
        Connect4::apply_move(&mut state, p1, &Connect4Move { col: 3 });
        assert_eq!(state.board[0][3], Cell::Red);
        assert_eq!(state.board[1][3], Cell::Yellow);
    }

    #[test]
    fn full_column_rejected() {
        let mut state = new_game();
        let p0 = state.players[0];
        let p1 = state.players[1];
        // Fill column 0
        for _ in 0..3 {
            Connect4::apply_move(&mut state, p0, &Connect4Move { col: 0 });
            Connect4::apply_move(&mut state, p1, &Connect4Move { col: 0 });
        }
        // Column 0 is now full (6 pieces)
        assert!(Connect4::validate_move(&state, p0, &Connect4Move { col: 0 }).is_err());
    }

    #[test]
    fn column_out_of_bounds_rejected() {
        let state = new_game();
        let p0 = state.players[0];
        assert!(Connect4::validate_move(&state, p0, &Connect4Move { col: 7 }).is_err());
    }

    #[test]
    fn wrong_turn_rejected() {
        let state = new_game();
        let p1 = state.players[1];
        assert!(Connect4::validate_move(&state, p1, &Connect4Move { col: 0 }).is_err());
    }

    #[test]
    fn horizontal_win() {
        let mut state = new_game();
        let p0 = state.players[0];
        let p1 = state.players[1];
        // Red: 0,1,2,3 on row 0; Yellow: 0,1,2 on row 1
        Connect4::apply_move(&mut state, p0, &Connect4Move { col: 0 });
        Connect4::apply_move(&mut state, p1, &Connect4Move { col: 0 });
        Connect4::apply_move(&mut state, p0, &Connect4Move { col: 1 });
        Connect4::apply_move(&mut state, p1, &Connect4Move { col: 1 });
        Connect4::apply_move(&mut state, p0, &Connect4Move { col: 2 });
        Connect4::apply_move(&mut state, p1, &Connect4Move { col: 2 });
        Connect4::apply_move(&mut state, p0, &Connect4Move { col: 3 });

        match Connect4::is_terminal(&state) {
            Some(GameOutcome::Win(winner)) => assert_eq!(winner, p0),
            other => panic!("expected Red win, got {other:?}"),
        }
    }

    #[test]
    fn vertical_win() {
        let mut state = new_game();
        let p0 = state.players[0];
        let p1 = state.players[1];
        // Red plays col 0 four times, Yellow plays col 1
        Connect4::apply_move(&mut state, p0, &Connect4Move { col: 0 });
        Connect4::apply_move(&mut state, p1, &Connect4Move { col: 1 });
        Connect4::apply_move(&mut state, p0, &Connect4Move { col: 0 });
        Connect4::apply_move(&mut state, p1, &Connect4Move { col: 1 });
        Connect4::apply_move(&mut state, p0, &Connect4Move { col: 0 });
        Connect4::apply_move(&mut state, p1, &Connect4Move { col: 1 });
        Connect4::apply_move(&mut state, p0, &Connect4Move { col: 0 });

        match Connect4::is_terminal(&state) {
            Some(GameOutcome::Win(winner)) => assert_eq!(winner, p0),
            other => panic!("expected Red win, got {other:?}"),
        }
    }

    #[test]
    fn diagonal_win() {
        let mut state = new_game();
        let p0 = state.players[0];
        let p1 = state.players[1];
        // Build a diagonal: Red at (0,0),(1,1),(2,2),(3,3)
        // Col 0: R
        Connect4::apply_move(&mut state, p0, &Connect4Move { col: 0 }); // (0,0) R
        // Col 1: Y, R
        Connect4::apply_move(&mut state, p1, &Connect4Move { col: 1 }); // (0,1) Y
        Connect4::apply_move(&mut state, p0, &Connect4Move { col: 1 }); // (1,1) R
        // Col 2: Y, Y, R
        Connect4::apply_move(&mut state, p1, &Connect4Move { col: 2 }); // (0,2) Y
        Connect4::apply_move(&mut state, p0, &Connect4Move { col: 3 }); // (0,3) R — placeholder
        Connect4::apply_move(&mut state, p1, &Connect4Move { col: 2 }); // (1,2) Y
        Connect4::apply_move(&mut state, p0, &Connect4Move { col: 2 }); // (2,2) R
        // Col 3: Y, Y, Y, R
        Connect4::apply_move(&mut state, p1, &Connect4Move { col: 3 }); // (1,3) Y
        Connect4::apply_move(&mut state, p0, &Connect4Move { col: 4 }); // placeholder
        Connect4::apply_move(&mut state, p1, &Connect4Move { col: 3 }); // (2,3) Y
        Connect4::apply_move(&mut state, p0, &Connect4Move { col: 3 }); // (3,3) R — diagonal complete

        match Connect4::is_terminal(&state) {
            Some(GameOutcome::Win(winner)) => assert_eq!(winner, p0),
            other => panic!("expected Red diagonal win, got {other:?}"),
        }
    }

    #[test]
    fn no_win_mid_game() {
        let mut state = new_game();
        let p0 = state.players[0];
        Connect4::apply_move(&mut state, p0, &Connect4Move { col: 3 });
        assert!(Connect4::is_terminal(&state).is_none());
    }

    #[test]
    fn current_player_alternates() {
        let mut state = new_game();
        let p0 = state.players[0];
        let p1 = state.players[1];

        assert_eq!(Connect4::current_player(&state), p0);
        Connect4::apply_move(&mut state, p0, &Connect4Move { col: 0 });
        assert_eq!(Connect4::current_player(&state), p1);
        Connect4::apply_move(&mut state, p1, &Connect4Move { col: 1 });
        assert_eq!(Connect4::current_player(&state), p0);
    }

    #[test]
    fn bot_easy_returns_valid_move() {
        let state = new_game();
        let mv = bot_move(&state, Difficulty::Easy);
        assert!((mv.col as usize) < COLS);
        assert_eq!(state.board[0][mv.col as usize], Cell::Empty);
    }

    #[test]
    fn bot_hard_returns_valid_move() {
        let state = new_game();
        let mv = bot_move(&state, Difficulty::Hard);
        assert!((mv.col as usize) < COLS);
        assert_eq!(state.board[0][mv.col as usize], Cell::Empty);
    }

    #[test]
    fn bot_hard_takes_winning_move() {
        let mut state = new_game();
        let p0 = state.players[0];
        let p1 = state.players[1];
        // Red has 3 in a row on bottom: cols 0,1,2
        Connect4::apply_move(&mut state, p0, &Connect4Move { col: 0 });
        Connect4::apply_move(&mut state, p1, &Connect4Move { col: 0 });
        Connect4::apply_move(&mut state, p0, &Connect4Move { col: 1 });
        Connect4::apply_move(&mut state, p1, &Connect4Move { col: 1 });
        Connect4::apply_move(&mut state, p0, &Connect4Move { col: 2 });
        Connect4::apply_move(&mut state, p1, &Connect4Move { col: 2 });
        // Red's turn, should play col 3 to win
        let mv = bot_move(&state, Difficulty::Hard);
        assert_eq!(mv.col, 3);
    }

    #[test]
    fn bot_hard_blocks_opponent_win() {
        let mut state = new_game();
        let p0 = state.players[0];
        let p1 = state.players[1];
        // Red: col 6 (dump move), Yellow: cols 0,1,2 — Yellow threatens col 3
        Connect4::apply_move(&mut state, p0, &Connect4Move { col: 6 });
        Connect4::apply_move(&mut state, p1, &Connect4Move { col: 0 });
        Connect4::apply_move(&mut state, p0, &Connect4Move { col: 6 });
        Connect4::apply_move(&mut state, p1, &Connect4Move { col: 1 });
        Connect4::apply_move(&mut state, p0, &Connect4Move { col: 5 });
        Connect4::apply_move(&mut state, p1, &Connect4Move { col: 2 });
        // Red's turn — must block col 3
        let mv = bot_move(&state, Difficulty::Hard);
        assert_eq!(mv.col, 3);
    }
}
