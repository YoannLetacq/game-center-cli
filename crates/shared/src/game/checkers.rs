use serde::{Deserialize, Serialize};

use crate::types::{Difficulty, GameOutcome, GameSettings, PlayerId};

use super::traits::GameEngine;

pub const BOARD_SIZE: usize = 8;
const DRAW_PLY_LIMIT: u32 = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Position {
    pub row: u8,
    pub col: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Black,
    Red,
}

impl Side {
    fn opponent(self) -> Side {
        match self {
            Side::Black => Side::Red,
            Side::Red => Side::Black,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Square {
    Empty,
    Man(Side),
    King(Side),
}

impl Square {
    fn side(self) -> Option<Side> {
        match self {
            Square::Man(s) | Square::King(s) => Some(s),
            Square::Empty => None,
        }
    }

    fn is_king(self) -> bool {
        matches!(self, Square::King(_))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckersMove {
    /// Ordered squares: [0] = start, [last] = final landing. len >= 2.
    pub path: Vec<Position>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckersState {
    pub board: [[Square; BOARD_SIZE]; BOARD_SIZE],
    pub players: [PlayerId; 2],
    pub current_turn: usize,
    pub move_count: u32,
    pub plies_since_progress: u32,
}

pub struct Checkers;

impl GameEngine for Checkers {
    type Move = CheckersMove;
    type State = CheckersState;

    #[allow(clippy::needless_range_loop)]
    fn initial_state(players: &[PlayerId], _settings: &GameSettings) -> Self::State {
        assert!(players.len() >= 2, "Checkers requires exactly 2 players");
        let mut board = [[Square::Empty; BOARD_SIZE]; BOARD_SIZE];
        for row in 0..3 {
            for col in 0..BOARD_SIZE {
                if is_dark_square(row, col) {
                    board[row][col] = Square::Man(Side::Red);
                }
            }
        }
        for row in 5..BOARD_SIZE {
            for col in 0..BOARD_SIZE {
                if is_dark_square(row, col) {
                    board[row][col] = Square::Man(Side::Black);
                }
            }
        }
        CheckersState {
            board,
            players: [players[0], players[1]],
            current_turn: 0,
            move_count: 0,
            plies_since_progress: 0,
        }
    }

    fn validate_move(state: &Self::State, player: PlayerId, mv: &Self::Move) -> Result<(), String> {
        if state.players[state.current_turn] != player {
            return Err("not your turn".to_string());
        }
        let side = side_for_turn(state.current_turn);
        validate_move_inner(state, side, mv)
    }

    fn apply_move(state: &mut Self::State, _player: PlayerId, mv: &Self::Move) {
        let side = side_for_turn(state.current_turn);
        let (captured, promoted) = execute_move(&mut state.board, side, mv);
        state.move_count += 1;
        if captured > 0 || promoted {
            state.plies_since_progress = 0;
        } else {
            state.plies_since_progress += 1;
        }
        state.current_turn = 1 - state.current_turn;
    }

    fn is_terminal(state: &Self::State) -> Option<GameOutcome> {
        if state.plies_since_progress >= DRAW_PLY_LIMIT {
            return Some(GameOutcome::Draw);
        }
        if legal_moves(state).is_empty() {
            let winner = state.players[1 - state.current_turn];
            return Some(GameOutcome::Win(winner));
        }
        None
    }

    fn current_player(state: &Self::State) -> PlayerId {
        state.players[state.current_turn]
    }
}

fn side_for_turn(turn: usize) -> Side {
    if turn == 0 { Side::Black } else { Side::Red }
}

fn is_dark_square(row: usize, col: usize) -> bool {
    (row + col) % 2 == 1
}

fn in_bounds(r: i32, c: i32) -> bool {
    (0..BOARD_SIZE as i32).contains(&r) && (0..BOARD_SIZE as i32).contains(&c)
}

fn king_row(side: Side) -> u8 {
    match side {
        Side::Black => 0,
        Side::Red => (BOARD_SIZE - 1) as u8,
    }
}

fn forward_dys(side: Side) -> &'static [i32] {
    match side {
        // Black moves up (row decreasing), Red moves down.
        Side::Black => &[-1],
        Side::Red => &[1],
    }
}

const ALL_DYS: [i32; 2] = [-1, 1];

fn execute_move(
    board: &mut [[Square; BOARD_SIZE]; BOARD_SIZE],
    side: Side,
    mv: &CheckersMove,
) -> (u32, bool) {
    let start = mv.path[0];
    let mut piece = board[start.row as usize][start.col as usize];
    board[start.row as usize][start.col as usize] = Square::Empty;

    let mut captured = 0u32;
    let mut promoted = false;

    for window in mv.path.windows(2) {
        let from = window[0];
        let to = window[1];
        let dr = to.row as i32 - from.row as i32;
        let dc = to.col as i32 - from.col as i32;
        if dr.abs() == 2 && dc.abs() == 2 {
            let mr = (from.row as i32 + dr / 2) as usize;
            let mc = (from.col as i32 + dc / 2) as usize;
            board[mr][mc] = Square::Empty;
            captured += 1;
        }
        if !piece.is_king() && to.row == king_row(side) {
            piece = Square::King(side);
            promoted = true;
        }
    }

    let end = *mv.path.last().expect("move path non-empty");
    board[end.row as usize][end.col as usize] = piece;
    (captured, promoted)
}

fn validate_move_inner(state: &CheckersState, side: Side, mv: &CheckersMove) -> Result<(), String> {
    if mv.path.len() < 2 {
        return Err("move path too short".to_string());
    }
    for p in &mv.path {
        if (p.row as usize) >= BOARD_SIZE || (p.col as usize) >= BOARD_SIZE {
            return Err("position out of bounds".to_string());
        }
        if !is_dark_square(p.row as usize, p.col as usize) {
            return Err("position not on a dark square".to_string());
        }
    }

    let start = mv.path[0];
    let piece0 = state.board[start.row as usize][start.col as usize];
    match piece0.side() {
        Some(s) if s == side => {}
        _ => return Err("no piece of yours on starting square".to_string()),
    }

    let captures_available = any_capture_available(&state.board, side);
    let first_dr = mv.path[1].row as i32 - mv.path[0].row as i32;
    let first_dc = mv.path[1].col as i32 - mv.path[0].col as i32;
    let is_jump_move = first_dr.abs() == 2 && first_dc.abs() == 2;

    if captures_available && !is_jump_move {
        return Err("a capture is available and must be taken".to_string());
    }

    let mut board = state.board;
    board[start.row as usize][start.col as usize] = Square::Empty;
    let mut piece = piece0;
    let mut cur = start;
    let mut any_jump = false;
    let mut promoted_this_move = false;
    let mut captured_squares: Vec<(u8, u8)> = Vec::new();

    for i in 1..mv.path.len() {
        if promoted_this_move {
            return Err("chain must end after promotion".to_string());
        }
        let to = mv.path[i];
        let dr = to.row as i32 - cur.row as i32;
        let dc = to.col as i32 - cur.col as i32;
        if dr.abs() != dc.abs() || (dr.abs() != 1 && dr.abs() != 2) {
            return Err("hop must be a diagonal 1 or 2 square step".to_string());
        }
        if board[to.row as usize][to.col as usize] != Square::Empty {
            return Err("destination square not empty".to_string());
        }
        let is_king = piece.is_king();
        let legal_dys: &[i32] = if is_king { &ALL_DYS } else { forward_dys(side) };
        if !legal_dys.contains(&dr.signum()) {
            return Err("illegal direction for this piece".to_string());
        }

        if dr.abs() == 2 {
            if i >= 2 && !any_jump {
                return Err("cannot mix steps and jumps".to_string());
            }
            let mr = (cur.row as i32 + dr / 2) as u8;
            let mc = (cur.col as i32 + dc / 2) as u8;
            let mid = board[mr as usize][mc as usize];
            match mid.side() {
                Some(s) if s == side.opponent() => {}
                _ => return Err("jump must be over an enemy piece".to_string()),
            }
            if captured_squares.contains(&(mr, mc)) {
                return Err("cannot jump the same piece twice".to_string());
            }
            board[mr as usize][mc as usize] = Square::Empty;
            captured_squares.push((mr, mc));
            any_jump = true;
        } else {
            if i != 1 || mv.path.len() != 2 {
                return Err("cannot mix steps and jumps".to_string());
            }
            if captures_available {
                return Err("a capture is available and must be taken".to_string());
            }
        }

        cur = to;
        if !piece.is_king() && to.row == king_row(side) {
            piece = Square::King(side);
            promoted_this_move = true;
        }
    }

    board[cur.row as usize][cur.col as usize] = piece;

    if any_jump && !promoted_this_move && piece_has_jump_from(&board, piece, cur) {
        return Err("must continue jumping".to_string());
    }

    Ok(())
}

fn piece_has_jump_from(
    board: &[[Square; BOARD_SIZE]; BOARD_SIZE],
    piece: Square,
    pos: Position,
) -> bool {
    let side = match piece.side() {
        Some(s) => s,
        None => return false,
    };
    let dys: &[i32] = if piece.is_king() {
        &ALL_DYS
    } else {
        forward_dys(side)
    };
    for &dy in dys {
        for &dx in &ALL_DYS {
            let mr = pos.row as i32 + dy;
            let mc = pos.col as i32 + dx;
            let lr = pos.row as i32 + 2 * dy;
            let lc = pos.col as i32 + 2 * dx;
            if !in_bounds(lr, lc) {
                continue;
            }
            let mid = board[mr as usize][mc as usize];
            let land = board[lr as usize][lc as usize];
            if land == Square::Empty && mid.side() == Some(side.opponent()) {
                return true;
            }
        }
    }
    false
}

fn any_capture_available(board: &[[Square; BOARD_SIZE]; BOARD_SIZE], side: Side) -> bool {
    for r in 0..BOARD_SIZE {
        for c in 0..BOARD_SIZE {
            let piece = board[r][c];
            if piece.side() == Some(side)
                && piece_has_jump_from(
                    board,
                    piece,
                    Position {
                        row: r as u8,
                        col: c as u8,
                    },
                )
            {
                return true;
            }
        }
    }
    false
}

/// Public wrapper around [`legal_moves`] so that client code can enumerate
/// legal moves (e.g. for input validation in the TUI).
pub fn legal_moves_public(state: &CheckersState) -> Vec<CheckersMove> {
    legal_moves(state)
}

pub(crate) fn legal_moves(state: &CheckersState) -> Vec<CheckersMove> {
    let side = side_for_turn(state.current_turn);
    let mut moves = Vec::new();
    let captures = any_capture_available(&state.board, side);

    for r in 0..BOARD_SIZE {
        for c in 0..BOARD_SIZE {
            let piece = state.board[r][c];
            if piece.side() != Some(side) {
                continue;
            }
            let pos = Position {
                row: r as u8,
                col: c as u8,
            };
            if captures {
                let mut board = state.board;
                board[r][c] = Square::Empty;
                let mut path = vec![pos];
                extend_jumps(&mut board, piece, side, &mut path, &mut moves);
            } else {
                collect_steps(&state.board, piece, side, pos, &mut moves);
            }
        }
    }
    moves
}

fn collect_steps(
    board: &[[Square; BOARD_SIZE]; BOARD_SIZE],
    piece: Square,
    side: Side,
    pos: Position,
    out: &mut Vec<CheckersMove>,
) {
    let dys: &[i32] = if piece.is_king() {
        &ALL_DYS
    } else {
        forward_dys(side)
    };
    for &dy in dys {
        for &dx in &ALL_DYS {
            let nr = pos.row as i32 + dy;
            let nc = pos.col as i32 + dx;
            if !in_bounds(nr, nc) {
                continue;
            }
            if board[nr as usize][nc as usize] == Square::Empty {
                out.push(CheckersMove {
                    path: vec![
                        pos,
                        Position {
                            row: nr as u8,
                            col: nc as u8,
                        },
                    ],
                });
            }
        }
    }
}

fn extend_jumps(
    board: &mut [[Square; BOARD_SIZE]; BOARD_SIZE],
    piece: Square,
    side: Side,
    path: &mut Vec<Position>,
    out: &mut Vec<CheckersMove>,
) {
    let pos = *path.last().unwrap();
    let dys: &[i32] = if piece.is_king() {
        &ALL_DYS
    } else {
        forward_dys(side)
    };
    for &dy in dys {
        for &dx in &ALL_DYS {
            let mr = pos.row as i32 + dy;
            let mc = pos.col as i32 + dx;
            let lr = pos.row as i32 + 2 * dy;
            let lc = pos.col as i32 + 2 * dx;
            if !in_bounds(lr, lc) {
                continue;
            }
            let mid_piece = board[mr as usize][mc as usize];
            if mid_piece.side() != Some(side.opponent()) {
                continue;
            }
            if board[lr as usize][lc as usize] != Square::Empty {
                continue;
            }
            let landing = Position {
                row: lr as u8,
                col: lc as u8,
            };
            let promoted = !piece.is_king() && landing.row == king_row(side);
            let new_piece = if promoted { Square::King(side) } else { piece };

            // Apply hop.
            board[mr as usize][mc as usize] = Square::Empty;
            path.push(landing);

            if promoted {
                out.push(CheckersMove { path: path.clone() });
            } else {
                board[lr as usize][lc as usize] = new_piece;
                let before = out.len();
                extend_jumps(board, new_piece, side, path, out);
                let no_further = out.len() == before;
                board[lr as usize][lc as usize] = Square::Empty;
                if no_further {
                    out.push(CheckersMove { path: path.clone() });
                }
            }

            // Undo hop.
            path.pop();
            board[mr as usize][mc as usize] = mid_piece;
        }
    }
}

// --- Bot AI ---

/// Returns a legal move for the current player.
/// Precondition: `is_terminal(state)` must return `None` — panics on a finished game.
pub fn bot_move(state: &CheckersState, difficulty: Difficulty) -> CheckersMove {
    match difficulty {
        Difficulty::Easy => random_move(state),
        Difficulty::Hard => best_move(state),
    }
}

fn random_move(state: &CheckersState) -> CheckersMove {
    use rand::prelude::IndexedRandom;
    let mut rng = rand::rng();
    let moves = legal_moves(state);
    assert!(!moves.is_empty(), "no legal moves for bot");
    moves.choose(&mut rng).expect("non-empty").clone()
}

fn best_move(state: &CheckersState) -> CheckersMove {
    let moves = legal_moves(state);
    assert!(!moves.is_empty(), "no legal moves for bot");
    // Black (players[0]) is maximizer.
    let maximizing = state.current_turn == 0;
    let mut best_score = if maximizing { i32::MIN } else { i32::MAX };
    let mut best = moves[0].clone();

    for mv in &moves {
        let mut next = state.clone();
        Checkers::apply_move(&mut next, state.players[state.current_turn], mv);
        let score = alpha_beta(&next, 6, i32::MIN, i32::MAX, !maximizing);
        let better = if maximizing {
            score > best_score
        } else {
            score < best_score
        };
        if better {
            best_score = score;
            best = mv.clone();
        }
    }

    best
}

fn alpha_beta(
    state: &CheckersState,
    depth: u8,
    mut alpha: i32,
    mut beta: i32,
    maximizing: bool,
) -> i32 {
    if let Some(outcome) = Checkers::is_terminal(state) {
        return match outcome {
            GameOutcome::Win(pid) if pid == state.players[0] => 100_000 + depth as i32,
            GameOutcome::Win(_) => -(100_000 + depth as i32),
            GameOutcome::Draw => 0,
        };
    }
    if depth == 0 {
        return evaluate(state);
    }
    let moves = legal_moves(state);
    if moves.is_empty() {
        // Treated as loss for side to move.
        return if state.current_turn == 0 {
            -(100_000 + depth as i32)
        } else {
            100_000 + depth as i32
        };
    }

    if maximizing {
        let mut value = i32::MIN;
        for mv in &moves {
            let mut next = state.clone();
            Checkers::apply_move(&mut next, state.players[state.current_turn], mv);
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
        for mv in &moves {
            let mut next = state.clone();
            Checkers::apply_move(&mut next, state.players[state.current_turn], mv);
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

fn evaluate(state: &CheckersState) -> i32 {
    let mut black_men = 0i32;
    let mut red_men = 0i32;
    let mut black_kings = 0i32;
    let mut red_kings = 0i32;
    let mut back_row = 0i32;
    let mut edge = 0i32;
    for r in 0..BOARD_SIZE {
        for c in 0..BOARD_SIZE {
            match state.board[r][c] {
                Square::Man(Side::Black) => {
                    black_men += 1;
                    if r == BOARD_SIZE - 1 {
                        back_row += 1;
                    }
                    if c == 0 || c == BOARD_SIZE - 1 {
                        edge += 1;
                    }
                }
                Square::Man(Side::Red) => {
                    red_men += 1;
                    if r == 0 {
                        back_row -= 1;
                    }
                    if c == 0 || c == BOARD_SIZE - 1 {
                        edge -= 1;
                    }
                }
                Square::King(Side::Black) => {
                    black_kings += 1;
                    if c == 0 || c == BOARD_SIZE - 1 {
                        edge += 1;
                    }
                }
                Square::King(Side::Red) => {
                    red_kings += 1;
                    if c == 0 || c == BOARD_SIZE - 1 {
                        edge -= 1;
                    }
                }
                Square::Empty => {}
            }
        }
    }
    100 * (black_men - red_men) + 150 * (black_kings - red_kings) + 10 * back_row + 5 * edge
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_players() -> [PlayerId; 2] {
        [PlayerId::new(), PlayerId::new()]
    }

    fn new_game() -> CheckersState {
        Checkers::initial_state(&two_players(), &GameSettings::default())
    }

    fn pos(r: u8, c: u8) -> Position {
        Position { row: r, col: c }
    }

    fn empty_state() -> CheckersState {
        let mut s = new_game();
        s.board = [[Square::Empty; BOARD_SIZE]; BOARD_SIZE];
        s
    }

    #[test]
    fn initial_state_layout_correct() {
        let s = new_game();
        for r in 0..3 {
            for c in 0..BOARD_SIZE {
                if is_dark_square(r, c) {
                    assert_eq!(s.board[r][c], Square::Man(Side::Red));
                } else {
                    assert_eq!(s.board[r][c], Square::Empty);
                }
            }
        }
        for r in 3..5 {
            for c in 0..BOARD_SIZE {
                assert_eq!(s.board[r][c], Square::Empty);
            }
        }
        for r in 5..BOARD_SIZE {
            for c in 0..BOARD_SIZE {
                if is_dark_square(r, c) {
                    assert_eq!(s.board[r][c], Square::Man(Side::Black));
                } else {
                    assert_eq!(s.board[r][c], Square::Empty);
                }
            }
        }
        assert_eq!(s.current_turn, 0);
        assert_eq!(s.move_count, 0);
        assert_eq!(s.plies_since_progress, 0);
    }

    #[test]
    fn black_moves_first() {
        let s = new_game();
        assert_eq!(Checkers::current_player(&s), s.players[0]);
    }

    #[test]
    fn man_diagonal_forward_step() {
        let mut s = new_game();
        let p0 = s.players[0];
        let mv = CheckersMove {
            path: vec![pos(5, 0), pos(4, 1)],
        };
        assert!(Checkers::validate_move(&s, p0, &mv).is_ok());
        Checkers::apply_move(&mut s, p0, &mv);
        assert_eq!(s.board[4][1], Square::Man(Side::Black));
        assert_eq!(s.board[5][0], Square::Empty);
        assert_eq!(s.current_turn, 1);
    }

    #[test]
    fn man_backward_step_rejected() {
        let mut s = empty_state();
        s.board[4][1] = Square::Man(Side::Black);
        s.current_turn = 0;
        let mv = CheckersMove {
            path: vec![pos(4, 1), pos(5, 2)],
        };
        assert!(Checkers::validate_move(&s, s.players[0], &mv).is_err());
    }

    #[test]
    fn man_capture_mandatory() {
        let mut s = empty_state();
        s.board[5][0] = Square::Man(Side::Black);
        s.board[4][1] = Square::Man(Side::Red);
        // (3,2) is dark (3+2=5 odd) empty — jump available.
        s.current_turn = 0;
        // Non-capture step rejected.
        let step = CheckersMove {
            path: vec![pos(5, 0), pos(4, 1)],
        };
        assert!(Checkers::validate_move(&s, s.players[0], &step).is_err());
        // A different non-capture (on a distinct piece) also rejected.
        s.board[5][2] = Square::Man(Side::Black);
        let step2 = CheckersMove {
            path: vec![pos(5, 2), pos(4, 3)],
        };
        assert!(Checkers::validate_move(&s, s.players[0], &step2).is_err());
        // Capture accepted.
        let jump = CheckersMove {
            path: vec![pos(5, 0), pos(3, 2)],
        };
        assert!(Checkers::validate_move(&s, s.players[0], &jump).is_ok());
    }

    #[test]
    fn multi_jump_chain_accepted() {
        let mut s = empty_state();
        s.board[5][0] = Square::Man(Side::Black);
        s.board[4][1] = Square::Man(Side::Red);
        s.board[2][3] = Square::Man(Side::Red);
        s.current_turn = 0;
        let mv = CheckersMove {
            path: vec![pos(5, 0), pos(3, 2), pos(1, 4)],
        };
        let p0 = s.players[0];
        assert!(Checkers::validate_move(&s, p0, &mv).is_ok());
        Checkers::apply_move(&mut s, p0, &mv);
        // Landed on row 1 (not yet row 0). Piece still a man.
        assert_eq!(s.board[1][4], Square::Man(Side::Black));
        assert_eq!(s.board[4][1], Square::Empty);
        assert_eq!(s.board[2][3], Square::Empty);
        assert_eq!(s.plies_since_progress, 0);
    }

    #[test]
    fn stopping_mid_chain_rejected() {
        let mut s = empty_state();
        s.board[5][0] = Square::Man(Side::Black);
        s.board[4][1] = Square::Man(Side::Red);
        s.board[2][3] = Square::Man(Side::Red);
        s.current_turn = 0;
        let short = CheckersMove {
            path: vec![pos(5, 0), pos(3, 2)],
        };
        assert!(Checkers::validate_move(&s, s.players[0], &short).is_err());
    }

    #[test]
    fn king_backward_capture_legal() {
        let mut s = empty_state();
        // Black king at (3,2); Red man at (4,3); landing (5,4). All dark squares.
        s.board[3][2] = Square::King(Side::Black);
        s.board[4][3] = Square::Man(Side::Red);
        s.current_turn = 0;
        let mv = CheckersMove {
            path: vec![pos(3, 2), pos(5, 4)],
        };
        let p0 = s.players[0];
        assert!(Checkers::validate_move(&s, p0, &mv).is_ok());
        Checkers::apply_move(&mut s, p0, &mv);
        assert_eq!(s.board[5][4], Square::King(Side::Black));
        assert_eq!(s.board[4][3], Square::Empty);
    }

    #[test]
    fn king_promotion_on_reaching_back_row() {
        let mut s = empty_state();
        s.board[1][0] = Square::Man(Side::Black);
        s.current_turn = 0;
        let mv = CheckersMove {
            path: vec![pos(1, 0), pos(0, 1)],
        };
        let p0 = s.players[0];
        assert!(Checkers::validate_move(&s, p0, &mv).is_ok());
        Checkers::apply_move(&mut s, p0, &mv);
        assert_eq!(s.board[0][1], Square::King(Side::Black));
        assert_eq!(s.plies_since_progress, 0);
    }

    #[test]
    fn promotion_ends_jump_chain() {
        let mut s = empty_state();
        // Black man at (2,1). Red at (1,2). After jump lands at (0,3) = promote.
        // Place another Red at (1,4) that would be jumpable as a king from (0,3)?
        // From (0,3) king could jump to (2,5) over (1,4). But promotion ends chain.
        s.board[2][1] = Square::Man(Side::Black);
        s.board[1][2] = Square::Man(Side::Red);
        s.board[1][4] = Square::Man(Side::Red);
        s.current_turn = 0;
        // Stopping on promotion must be OK.
        let stop = CheckersMove {
            path: vec![pos(2, 1), pos(0, 3)],
        };
        assert!(Checkers::validate_move(&s, s.players[0], &stop).is_ok());
        // Attempting to continue after promotion must fail.
        let cont = CheckersMove {
            path: vec![pos(2, 1), pos(0, 3), pos(2, 5)],
        };
        assert!(Checkers::validate_move(&s, s.players[0], &cont).is_err());
    }

    #[test]
    fn forty_ply_rule_triggers_draw() {
        let mut s = empty_state();
        s.board[3][0] = Square::King(Side::Black);
        s.board[3][6] = Square::King(Side::Red);
        s.current_turn = 0;
        s.plies_since_progress = DRAW_PLY_LIMIT;
        match Checkers::is_terminal(&s) {
            Some(GameOutcome::Draw) => {}
            other => panic!("expected draw, got {other:?}"),
        }
    }

    #[test]
    fn no_legal_moves_means_loss() {
        let mut s = empty_state();
        // Single Black Man on its own king row (row 0) — no forward squares exist.
        s.board[0][1] = Square::Man(Side::Black);
        s.current_turn = 0;
        assert!(legal_moves(&s).is_empty());
        match Checkers::is_terminal(&s) {
            Some(GameOutcome::Win(w)) => assert_eq!(w, s.players[1]),
            other => panic!("expected red win, got {other:?}"),
        }
    }

    #[test]
    fn current_player_alternates() {
        let mut s = new_game();
        let p0 = s.players[0];
        let p1 = s.players[1];
        assert_eq!(Checkers::current_player(&s), p0);
        let mv = CheckersMove {
            path: vec![pos(5, 0), pos(4, 1)],
        };
        Checkers::apply_move(&mut s, p0, &mv);
        assert_eq!(Checkers::current_player(&s), p1);
        let mv2 = CheckersMove {
            path: vec![pos(2, 1), pos(3, 0)],
        };
        Checkers::apply_move(&mut s, p1, &mv2);
        assert_eq!(Checkers::current_player(&s), p0);
    }

    #[test]
    fn bot_easy_returns_legal_move() {
        let s = new_game();
        let mv = bot_move(&s, Difficulty::Easy);
        assert!(Checkers::validate_move(&s, s.players[0], &mv).is_ok());
    }

    #[test]
    fn bot_hard_returns_legal_move() {
        let s = new_game();
        let mv = bot_move(&s, Difficulty::Hard);
        assert!(Checkers::validate_move(&s, s.players[0], &mv).is_ok());
    }

    #[test]
    fn bot_plays_full_game_without_panic() {
        let mut s = new_game();
        let mut plies = 0;
        while Checkers::is_terminal(&s).is_none() && plies < 500 {
            let side_player = s.players[s.current_turn];
            let mv = bot_move(&s, Difficulty::Easy);
            assert!(Checkers::validate_move(&s, side_player, &mv).is_ok());
            Checkers::apply_move(&mut s, side_player, &mv);
            plies += 1;
        }
    }
}
