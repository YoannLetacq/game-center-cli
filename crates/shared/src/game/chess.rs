use serde::{Deserialize, Serialize};

use crate::types::{Difficulty, GameOutcome, GameSettings, PlayerId};

use super::traits::GameEngine;

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Side {
    White,
    Black,
}

impl Side {
    pub fn opponent(self) -> Side {
        match self {
            Side::White => Side::Black,
            Side::Black => Side::White,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PieceKind {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Piece {
    pub kind: PieceKind,
    pub side: Side,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Position {
    pub row: u8, // 0 = rank 1 (white back rank), 7 = rank 8 (black back rank)
    pub col: u8, // 0 = a-file, 7 = h-file
}

impl Position {
    pub fn new(row: u8, col: u8) -> Self {
        Self { row, col }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChessMove {
    pub from: Position,
    pub to: Position,
    pub promotion: Option<PieceKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CastleRights {
    pub white_kingside: bool,
    pub white_queenside: bool,
    pub black_kingside: bool,
    pub black_queenside: bool,
}

impl CastleRights {
    fn all() -> Self {
        Self {
            white_kingside: true,
            white_queenside: true,
            black_kingside: true,
            black_queenside: true,
        }
    }

    fn kingside_for(&self, side: Side) -> bool {
        match side {
            Side::White => self.white_kingside,
            Side::Black => self.black_kingside,
        }
    }

    fn queenside_for(&self, side: Side) -> bool {
        match side {
            Side::White => self.white_queenside,
            Side::Black => self.black_queenside,
        }
    }

    fn clear_side(&mut self, side: Side) {
        match side {
            Side::White => {
                self.white_kingside = false;
                self.white_queenside = false;
            }
            Side::Black => {
                self.black_kingside = false;
                self.black_queenside = false;
            }
        }
    }

    fn clear_kingside(&mut self, side: Side) {
        match side {
            Side::White => self.white_kingside = false,
            Side::Black => self.black_kingside = false,
        }
    }

    fn clear_queenside(&mut self, side: Side) {
        match side {
            Side::White => self.white_queenside = false,
            Side::Black => self.black_queenside = false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChessState {
    pub board: [[Option<Piece>; 8]; 8],
    pub players: [PlayerId; 2], // 0 = White, 1 = Black
    pub current_turn: usize,    // 0 or 1
    pub move_count: u32,
    pub halfmove_clock: u32,
    pub castle_rights: CastleRights,
    pub en_passant_target: Option<Position>,
    pub position_history: Vec<u64>,
}

// ---------------------------------------------------------------------------
// Engine struct
// ---------------------------------------------------------------------------

pub struct Chess;

impl GameEngine for Chess {
    type Move = ChessMove;
    type State = ChessState;

    fn initial_state(players: &[PlayerId], _settings: &GameSettings) -> Self::State {
        assert!(players.len() >= 2, "Chess requires exactly 2 players");
        let mut board = [[None; 8]; 8];

        // White back rank (row 0)
        board[0][0] = Some(Piece { kind: PieceKind::Rook,   side: Side::White });
        board[0][1] = Some(Piece { kind: PieceKind::Knight, side: Side::White });
        board[0][2] = Some(Piece { kind: PieceKind::Bishop, side: Side::White });
        board[0][3] = Some(Piece { kind: PieceKind::Queen,  side: Side::White });
        board[0][4] = Some(Piece { kind: PieceKind::King,   side: Side::White });
        board[0][5] = Some(Piece { kind: PieceKind::Bishop, side: Side::White });
        board[0][6] = Some(Piece { kind: PieceKind::Knight, side: Side::White });
        board[0][7] = Some(Piece { kind: PieceKind::Rook,   side: Side::White });
        // White pawns (row 1)
        for sq in &mut board[1] {
            *sq = Some(Piece { kind: PieceKind::Pawn, side: Side::White });
        }
        // Black pawns (row 6)
        for sq in &mut board[6] {
            *sq = Some(Piece { kind: PieceKind::Pawn, side: Side::Black });
        }
        // Black back rank (row 7)
        board[7][0] = Some(Piece { kind: PieceKind::Rook,   side: Side::Black });
        board[7][1] = Some(Piece { kind: PieceKind::Knight, side: Side::Black });
        board[7][2] = Some(Piece { kind: PieceKind::Bishop, side: Side::Black });
        board[7][3] = Some(Piece { kind: PieceKind::Queen,  side: Side::Black });
        board[7][4] = Some(Piece { kind: PieceKind::King,   side: Side::Black });
        board[7][5] = Some(Piece { kind: PieceKind::Bishop, side: Side::Black });
        board[7][6] = Some(Piece { kind: PieceKind::Knight, side: Side::Black });
        board[7][7] = Some(Piece { kind: PieceKind::Rook,   side: Side::Black });

        let hash = position_hash(&board, 0, &CastleRights::all(), None);
        ChessState {
            board,
            players: [players[0], players[1]],
            current_turn: 0,
            move_count: 0,
            halfmove_clock: 0,
            castle_rights: CastleRights::all(),
            en_passant_target: None,
            position_history: vec![hash],
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
        apply_move_inner(state, side, mv);
    }

    fn is_terminal(state: &Self::State) -> Option<GameOutcome> {
        // Checkmate / stalemate — must be evaluated before 50-move and repetition (FIDE)
        let moves = legal_moves(state);
        if moves.is_empty() {
            let side = side_for_turn(state.current_turn);
            if in_check(state, side) {
                let winner = state.players[1 - state.current_turn];
                return Some(GameOutcome::Win(winner));
            } else {
                return Some(GameOutcome::Draw);
            }
        }

        // 50-move rule (100 half-moves)
        if state.halfmove_clock >= 100 {
            return Some(GameOutcome::Draw);
        }

        // Threefold repetition — any position repeated >= 3 times
        let mut counts: std::collections::HashMap<u64, u32> = std::collections::HashMap::new();
        for &h in &state.position_history {
            let c = counts.entry(h).or_insert(0);
            *c += 1;
            if *c >= 3 {
                return Some(GameOutcome::Draw);
            }
        }

        // Insufficient material
        if is_insufficient_material(&state.board) {
            return Some(GameOutcome::Draw);
        }

        None
    }

    fn current_player(state: &Self::State) -> PlayerId {
        state.players[state.current_turn]
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn side_for_turn(turn: usize) -> Side {
    if turn == 0 { Side::White } else { Side::Black }
}

fn in_bounds(r: i32, c: i32) -> bool {
    (0..8i32).contains(&r) && (0..8i32).contains(&c)
}

/// Returns true if `pos` is attacked by any piece of `by_side`.
pub fn is_square_attacked(board: &[[Option<Piece>; 8]; 8], pos: Position, by_side: Side) -> bool {
    let r = pos.row as i32;
    let c = pos.col as i32;

    // Pawn attacks:
    // White pawns move toward row 7 (+1 dir). A white pawn at (r-1, c±1) attacks (r,c).
    // Black pawns move toward row 0 (-1 dir). A black pawn at (r+1, c±1) attacks (r,c).
    let pawn_step: i32 = match by_side {
        Side::White => 1,
        Side::Black => -1,
    };
    for dc in [-1i32, 1] {
        let pr = r - pawn_step;
        let pc = c + dc;
        if in_bounds(pr, pc)
            && let Some(p) = board[pr as usize][pc as usize]
            && p.side == by_side
            && p.kind == PieceKind::Pawn
        {
            return true;
        }
    }

    // Knight attacks
    for (dr, dc) in [
        (-2i32, -1i32), (-2, 1), (-1, -2), (-1, 2),
        (1, -2), (1, 2), (2, -1), (2, 1),
    ] {
        let nr = r + dr;
        let nc = c + dc;
        if in_bounds(nr, nc)
            && let Some(p) = board[nr as usize][nc as usize]
            && p.side == by_side
            && p.kind == PieceKind::Knight
        {
            return true;
        }
    }

    // Bishop / Queen (diagonals)
    for (dr, dc) in [(-1i32, -1i32), (-1, 1), (1, -1), (1, 1)] {
        let mut nr = r + dr;
        let mut nc = c + dc;
        while in_bounds(nr, nc) {
            match board[nr as usize][nc as usize] {
                Some(p) if p.side == by_side && matches!(p.kind, PieceKind::Bishop | PieceKind::Queen) => {
                    return true;
                }
                Some(_) => break,
                None => {}
            }
            nr += dr;
            nc += dc;
        }
    }

    // Rook / Queen (ranks and files)
    for (dr, dc) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
        let mut nr = r + dr;
        let mut nc = c + dc;
        while in_bounds(nr, nc) {
            match board[nr as usize][nc as usize] {
                Some(p) if p.side == by_side && matches!(p.kind, PieceKind::Rook | PieceKind::Queen) => {
                    return true;
                }
                Some(_) => break,
                None => {}
            }
            nr += dr;
            nc += dc;
        }
    }

    // King attacks (adjacent squares)
    for dr in -1i32..=1 {
        for dc in -1i32..=1 {
            if dr == 0 && dc == 0 {
                continue;
            }
            let nr = r + dr;
            let nc = c + dc;
            if in_bounds(nr, nc)
                && let Some(p) = board[nr as usize][nc as usize]
                && p.side == by_side
                && p.kind == PieceKind::King
            {
                return true;
            }
        }
    }

    false
}

/// Find the king position for the given side.
pub fn king_position(state: &ChessState, side: Side) -> Option<Position> {
    for (r, row) in state.board.iter().enumerate() {
        for (c, sq) in row.iter().enumerate() {
            if let Some(p) = sq
                && p.kind == PieceKind::King
                && p.side == side
            {
                return Some(Position::new(r as u8, c as u8));
            }
        }
    }
    None
}

/// Returns true if `side` is currently in check.
pub fn in_check(state: &ChessState, side: Side) -> bool {
    match king_position(state, side) {
        Some(kpos) => is_square_attacked(&state.board, kpos, side.opponent()),
        None => false,
    }
}

/// Clone state and apply a pseudo-legal move (no legality check — just mutate).
/// Used to test whether a move leaves own king in check.
fn clone_and_apply_pseudo(state: &ChessState, mv: &ChessMove) -> ChessState {
    let mut next = state.clone();
    let side = side_for_turn(state.current_turn);
    apply_move_inner(&mut next, side, mv);
    next
}

/// Generate all pseudo-legal moves for a piece at `from`, appending to `out`.
fn pseudo_moves_for_piece(
    state: &ChessState,
    from: Position,
    piece: Piece,
    out: &mut Vec<ChessMove>,
) {
    let r = from.row as i32;
    let c = from.col as i32;
    let side = piece.side;

    match piece.kind {
        PieceKind::Pawn => {
            let dir: i32 = match side {
                Side::White => 1,
                Side::Black => -1,
            };
            let start_row: i32 = match side {
                Side::White => 1,
                Side::Black => 6,
            };
            let promo_row: i32 = match side {
                Side::White => 7,
                Side::Black => 0,
            };

            // Single step forward
            let nr = r + dir;
            let nc = c;
            if in_bounds(nr, nc) && state.board[nr as usize][nc as usize].is_none() {
                if nr == promo_row {
                    push_promotions(from, Position::new(nr as u8, nc as u8), out);
                } else {
                    out.push(ChessMove { from, to: Position::new(nr as u8, nc as u8), promotion: None });
                    // Double step from starting rank
                    if r == start_row {
                        let nr2 = r + 2 * dir;
                        if state.board[nr2 as usize][nc as usize].is_none() {
                            out.push(ChessMove {
                                from,
                                to: Position::new(nr2 as u8, nc as u8),
                                promotion: None,
                            });
                        }
                    }
                }
            }

            // Diagonal captures
            for dc in [-1i32, 1] {
                let tr = r + dir;
                let tc = c + dc;
                if !in_bounds(tr, tc) {
                    continue;
                }
                let target_sq = state.board[tr as usize][tc as usize];
                let is_ep = state.en_passant_target == Some(Position::new(tr as u8, tc as u8));
                let is_capture = target_sq.is_some_and(|p| p.side != side);
                if is_capture || is_ep {
                    if tr == promo_row {
                        push_promotions(from, Position::new(tr as u8, tc as u8), out);
                    } else {
                        out.push(ChessMove {
                            from,
                            to: Position::new(tr as u8, tc as u8),
                            promotion: None,
                        });
                    }
                }
            }
        }

        PieceKind::Knight => {
            for (dr, dc) in [
                (-2i32, -1i32), (-2, 1), (-1, -2), (-1, 2),
                (1, -2), (1, 2), (2, -1), (2, 1),
            ] {
                let nr = r + dr;
                let nc = c + dc;
                if in_bounds(nr, nc) {
                    let target = state.board[nr as usize][nc as usize];
                    if target.is_none_or(|p| p.side != side) {
                        out.push(ChessMove {
                            from,
                            to: Position::new(nr as u8, nc as u8),
                            promotion: None,
                        });
                    }
                }
            }
        }

        PieceKind::Bishop => {
            slide_moves(state, from, side, &[(-1i32, -1i32), (-1, 1), (1, -1), (1, 1)], out);
        }

        PieceKind::Rook => {
            slide_moves(state, from, side, &[(-1i32, 0i32), (1, 0), (0, -1), (0, 1)], out);
        }

        PieceKind::Queen => {
            slide_moves(
                state,
                from,
                side,
                &[(-1i32, -1i32), (-1, 1), (1, -1), (1, 1), (-1, 0), (1, 0), (0, -1), (0, 1)],
                out,
            );
        }

        PieceKind::King => {
            // Normal king moves
            for dr in -1i32..=1 {
                for dc in -1i32..=1 {
                    if dr == 0 && dc == 0 {
                        continue;
                    }
                    let nr = r + dr;
                    let nc = c + dc;
                    if in_bounds(nr, nc) {
                        let target = state.board[nr as usize][nc as usize];
                        if target.is_none_or(|p| p.side != side) {
                            out.push(ChessMove {
                                from,
                                to: Position::new(nr as u8, nc as u8),
                                promotion: None,
                            });
                        }
                    }
                }
            }

            // Castling
            let back_row: i32 = match side {
                Side::White => 0,
                Side::Black => 7,
            };
            if r == back_row && c == 4 && !in_check_board(&state.board, state.current_turn) {
                let rook_at = |col: usize| -> bool {
                    matches!(
                        state.board[back_row as usize][col],
                        Some(Piece { kind: PieceKind::Rook, side: s }) if s == side
                    )
                };
                // Kingside: king col 4 → 6, rook col 7 → 5
                if state.castle_rights.kingside_for(side)
                    && rook_at(7)
                    && state.board[back_row as usize][5].is_none()
                    && state.board[back_row as usize][6].is_none()
                    && !is_square_attacked(&state.board, Position::new(back_row as u8, 5), side.opponent())
                    && !is_square_attacked(&state.board, Position::new(back_row as u8, 6), side.opponent())
                {
                    out.push(ChessMove {
                        from,
                        to: Position::new(back_row as u8, 6),
                        promotion: None,
                    });
                }
                // Queenside: king col 4 → 2, rook col 0 → 3
                if state.castle_rights.queenside_for(side)
                    && rook_at(0)
                    && state.board[back_row as usize][3].is_none()
                    && state.board[back_row as usize][2].is_none()
                    && state.board[back_row as usize][1].is_none()
                    && !is_square_attacked(&state.board, Position::new(back_row as u8, 3), side.opponent())
                    && !is_square_attacked(&state.board, Position::new(back_row as u8, 2), side.opponent())
                {
                    out.push(ChessMove {
                        from,
                        to: Position::new(back_row as u8, 2),
                        promotion: None,
                    });
                }
            }
        }
    }
}

fn push_promotions(from: Position, to: Position, out: &mut Vec<ChessMove>) {
    for kind in [PieceKind::Queen, PieceKind::Rook, PieceKind::Bishop, PieceKind::Knight] {
        out.push(ChessMove { from, to, promotion: Some(kind) });
    }
}

fn slide_moves(
    state: &ChessState,
    from: Position,
    side: Side,
    dirs: &[(i32, i32)],
    out: &mut Vec<ChessMove>,
) {
    let r = from.row as i32;
    let c = from.col as i32;
    for &(dr, dc) in dirs {
        let mut nr = r + dr;
        let mut nc = c + dc;
        while in_bounds(nr, nc) {
            match state.board[nr as usize][nc as usize] {
                None => {
                    out.push(ChessMove {
                        from,
                        to: Position::new(nr as u8, nc as u8),
                        promotion: None,
                    });
                }
                Some(p) if p.side != side => {
                    out.push(ChessMove {
                        from,
                        to: Position::new(nr as u8, nc as u8),
                        promotion: None,
                    });
                    break;
                }
                Some(_) => break,
            }
            nr += dr;
            nc += dc;
        }
    }
}

fn in_check_board(board: &[[Option<Piece>; 8]; 8], current_turn: usize) -> bool {
    let side = side_for_turn(current_turn);
    for (r, row) in board.iter().enumerate() {
        for (c, sq) in row.iter().enumerate() {
            if let Some(p) = sq
                && p.kind == PieceKind::King
                && p.side == side
            {
                return is_square_attacked(board, Position::new(r as u8, c as u8), side.opponent());
            }
        }
    }
    false
}

/// Generate all legal moves for the current player.
pub fn legal_moves(state: &ChessState) -> Vec<ChessMove> {
    let side = side_for_turn(state.current_turn);
    let mut pseudo = Vec::new();

    for (r, row) in state.board.iter().enumerate() {
        for (c, sq) in row.iter().enumerate() {
            if let Some(piece) = sq
                && piece.side == side
            {
                pseudo_moves_for_piece(state, Position::new(r as u8, c as u8), *piece, &mut pseudo);
            }
        }
    }

    // Filter: keep only moves that don't leave own king in check, and reject
    // king-capture moves outright (defensive: in normal play `is_terminal`
    // fires checkmate first, but malformed state must not let a king be eaten).
    pseudo
        .into_iter()
        .filter(|mv| {
            if let Some(target) = state.board[mv.to.row as usize][mv.to.col as usize]
                && target.kind == PieceKind::King
            {
                return false;
            }
            let next = clone_and_apply_pseudo(state, mv);
            !in_check(&next, side)
        })
        .collect()
}

/// Validate a move (pseudo-legal + leaves king safe).
fn validate_move_inner(state: &ChessState, side: Side, mv: &ChessMove) -> Result<(), String> {
    // Source square
    if mv.from.row >= 8 || mv.from.col >= 8 || mv.to.row >= 8 || mv.to.col >= 8 {
        return Err("position out of bounds".to_string());
    }
    let piece = match state.board[mv.from.row as usize][mv.from.col as usize] {
        Some(p) if p.side == side => p,
        Some(_) => return Err("that piece belongs to your opponent".to_string()),
        None => return Err("no piece on the source square".to_string()),
    };

    // Reject king captures outright: a king is never a legal capture target
    // (mate ends the game first). Defends against malformed state.
    if let Some(target) = state.board[mv.to.row as usize][mv.to.col as usize]
        && target.kind == PieceKind::King
    {
        return Err("a king cannot be captured".to_string());
    }

    // Promotion field must be present iff pawn reaches last rank
    let promo_row: u8 = match side {
        Side::White => 7,
        Side::Black => 0,
    };
    if piece.kind == PieceKind::Pawn && mv.to.row == promo_row {
        match mv.promotion {
            None => return Err("must specify promotion piece".to_string()),
            Some(PieceKind::King) | Some(PieceKind::Pawn) => {
                return Err("invalid promotion piece".to_string())
            }
            Some(_) => {}
        }
    } else if mv.promotion.is_some() {
        return Err("promotion field set on non-promoting move".to_string());
    }

    // Check the move is in pseudo-legal list
    let mut pseudo = Vec::new();
    pseudo_moves_for_piece(state, mv.from, piece, &mut pseudo);
    if !pseudo.iter().any(|m| m.from == mv.from && m.to == mv.to && m.promotion == mv.promotion) {
        return Err("illegal move for this piece".to_string());
    }

    // Check move doesn't leave own king in check
    let next = clone_and_apply_pseudo(state, mv);
    if in_check(&next, side) {
        return Err("move leaves king in check".to_string());
    }

    Ok(())
}

/// Apply a move, updating all state fields.
fn apply_move_inner(state: &mut ChessState, side: Side, mv: &ChessMove) {
    let piece = state.board[mv.from.row as usize][mv.from.col as usize]
        .expect("apply_move_inner: no piece at source");

    let is_pawn_move = piece.kind == PieceKind::Pawn;
    let is_capture = state.board[mv.to.row as usize][mv.to.col as usize].is_some();

    // En passant capture: detect before clearing source
    let ep_capture_pos: Option<Position> =
        if is_pawn_move && let Some(ep) = state.en_passant_target && mv.to == ep {
            // The captured pawn is on the same row as the moving pawn, en passant column
            Some(Position::new(mv.from.row, mv.to.col))
        } else {
            None
        };

    // Halfmove clock
    if is_pawn_move || is_capture || ep_capture_pos.is_some() {
        state.halfmove_clock = 0;
    } else {
        state.halfmove_clock += 1;
    }

    // Detect castling (king moves 2 squares horizontally)
    let is_castling = piece.kind == PieceKind::King && (mv.to.col as i32 - mv.from.col as i32).abs() == 2;

    // Move piece
    state.board[mv.from.row as usize][mv.from.col as usize] = None;

    // Remove en passant captured pawn
    if let Some(ep_pos) = ep_capture_pos {
        state.board[ep_pos.row as usize][ep_pos.col as usize] = None;
    }

    // Castling: move rook
    if is_castling {
        let back_row = mv.from.row as usize;
        if mv.to.col == 6 {
            // Kingside
            state.board[back_row][5] = state.board[back_row][7].take();
        } else {
            // Queenside
            state.board[back_row][3] = state.board[back_row][0].take();
        }
    }

    // Place piece (or promoted piece) on destination
    let placed_piece = if let Some(promo_kind) = mv.promotion {
        Piece { kind: promo_kind, side }
    } else {
        piece
    };
    state.board[mv.to.row as usize][mv.to.col as usize] = Some(placed_piece);

    // Update castle rights
    if piece.kind == PieceKind::King {
        state.castle_rights.clear_side(side);
    }
    // Rook moves from corners
    if piece.kind == PieceKind::Rook {
        let back_row: u8 = match side {
            Side::White => 0,
            Side::Black => 7,
        };
        if mv.from.row == back_row {
            if mv.from.col == 0 {
                state.castle_rights.clear_queenside(side);
            } else if mv.from.col == 7 {
                state.castle_rights.clear_kingside(side);
            }
        }
    }
    // If a rook on the opponent's corner is captured, clear their rights
    {
        let opp = side.opponent();
        let opp_back: u8 = match opp {
            Side::White => 0,
            Side::Black => 7,
        };
        if mv.to.row == opp_back && mv.to.col == 0 {
            state.castle_rights.clear_queenside(opp);
        }
        if mv.to.row == opp_back && mv.to.col == 7 {
            state.castle_rights.clear_kingside(opp);
        }
    }

    // Update en passant target — only if an enemy pawn can actually capture
    state.en_passant_target = if is_pawn_move {
        let dr = mv.to.row as i32 - mv.from.row as i32;
        if dr.abs() == 2 {
            let ep_row = (mv.from.row as i32 + dr / 2) as u8;
            let to_r = mv.to.row as usize;
            let to_c = mv.to.col as i32;
            let enemy = side.opponent();
            let has_enemy_pawn = |c: i32| -> bool {
                if !(0..8).contains(&c) {
                    return false;
                }
                matches!(
                    state.board[to_r][c as usize],
                    Some(Piece { kind: PieceKind::Pawn, side: s }) if s == enemy
                )
            };
            if has_enemy_pawn(to_c - 1) || has_enemy_pawn(to_c + 1) {
                Some(Position::new(ep_row, mv.from.col))
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    state.move_count += 1;
    state.current_turn = 1 - state.current_turn;

    // Record position hash
    let hash = position_hash(
        &state.board,
        state.current_turn,
        &state.castle_rights,
        state.en_passant_target,
    );
    state.position_history.push(hash);
}

// ---------------------------------------------------------------------------
// Insufficient material detection
// ---------------------------------------------------------------------------

fn is_insufficient_material(board: &[[Option<Piece>; 8]; 8]) -> bool {
    let mut white_pieces: Vec<(PieceKind, u8, u8)> = Vec::new();
    let mut black_pieces: Vec<(PieceKind, u8, u8)> = Vec::new();

    for (r, row) in board.iter().enumerate() {
        for (c, sq) in row.iter().enumerate() {
            if let Some(p) = sq {
                match p.side {
                    Side::White => white_pieces.push((p.kind, r as u8, c as u8)),
                    Side::Black => black_pieces.push((p.kind, r as u8, c as u8)),
                }
            }
        }
    }

    // K vs K
    if white_pieces.len() == 1 && black_pieces.len() == 1 {
        return true;
    }

    let is_lone_king = |pieces: &Vec<(PieceKind, u8, u8)>| {
        pieces.len() == 1 && pieces[0].0 == PieceKind::King
    };
    let is_kb_or_kn = |pieces: &Vec<(PieceKind, u8, u8)>| {
        if pieces.len() != 2 {
            return false;
        }
        let kinds: Vec<PieceKind> = pieces.iter().map(|p| p.0).collect();
        kinds.contains(&PieceKind::King)
            && (kinds.contains(&PieceKind::Bishop) || kinds.contains(&PieceKind::Knight))
    };

    if is_lone_king(&white_pieces) && is_kb_or_kn(&black_pieces) {
        return true;
    }
    if is_lone_king(&black_pieces) && is_kb_or_kn(&white_pieces) {
        return true;
    }

    // K+B vs K+B same-colored bishops
    let is_kb = |pieces: &Vec<(PieceKind, u8, u8)>| -> Option<(u8, u8)> {
        if pieces.len() != 2 {
            return None;
        }
        let mut bishop_sq = None;
        let mut has_king = false;
        for &(kind, r, c) in pieces {
            match kind {
                PieceKind::King => has_king = true,
                PieceKind::Bishop => bishop_sq = Some((r, c)),
                _ => return None,
            }
        }
        if has_king { bishop_sq } else { None }
    };

    if let (Some((wr, wc)), Some((br, bc))) = (is_kb(&white_pieces), is_kb(&black_pieces))
        && (wr + wc) % 2 == (br + bc) % 2
    {
        return true;
    }

    false
}

// ---------------------------------------------------------------------------
// Simple Zobrist-like hash for position history
// ---------------------------------------------------------------------------

fn position_hash(
    board: &[[Option<Piece>; 8]; 8],
    current_turn: usize,
    castle_rights: &CastleRights,
    en_passant: Option<Position>,
) -> u64 {
    let mut h: u64 = 0;
    for (r, row) in board.iter().enumerate() {
        for (c, sq) in row.iter().enumerate() {
            if let Some(p) = sq {
                let piece_idx = piece_index(*p);
                h ^= splitmix64((r as u64 * 8 + c as u64) * 16 + piece_idx as u64 + 1);
            }
        }
    }
    h ^= current_turn as u64 * 0x9e37_79b9_7f4a_7c15;
    if castle_rights.white_kingside  { h ^= 0xbf58_476d_1ce4_e5b9; }
    if castle_rights.white_queenside { h ^= 0x94d0_49bb_1331_11eb; }
    if castle_rights.black_kingside  { h ^= 0x0123_4567_89ab_cdef; }
    if castle_rights.black_queenside { h ^= 0xfedc_ba98_7654_3210; }
    if let Some(ep) = en_passant {
        h ^= splitmix64(ep.col as u64 + 100);
    }
    h
}

fn piece_index(p: Piece) -> u8 {
    let side_offset: u8 = match p.side { Side::White => 0, Side::Black => 6 };
    let kind_offset: u8 = match p.kind {
        PieceKind::Pawn   => 0,
        PieceKind::Knight => 1,
        PieceKind::Bishop => 2,
        PieceKind::Rook   => 3,
        PieceKind::Queen  => 4,
        PieceKind::King   => 5,
    };
    side_offset + kind_offset
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

// ---------------------------------------------------------------------------
// Bot
// ---------------------------------------------------------------------------

/// Returns a legal move for the current player, or None if no legal moves.
pub fn bot_move(state: &ChessState, difficulty: Difficulty) -> Option<ChessMove> {
    match difficulty {
        Difficulty::Easy => bot_random(state),
        Difficulty::Hard => bot_minimax(state),
    }
}

fn bot_random(state: &ChessState) -> Option<ChessMove> {
    use rand::prelude::IndexedRandom;
    let moves = legal_moves(state);
    if moves.is_empty() {
        return None;
    }
    let mut rng = rand::rng();
    moves.choose(&mut rng).copied()
}

fn bot_minimax(state: &ChessState) -> Option<ChessMove> {
    use rand::prelude::IndexedRandom;
    let moves = legal_moves(state);
    if moves.is_empty() {
        return None;
    }

    // White (turn 0) is maximizer, Black (turn 1) is minimizer
    let maximizing = state.current_turn == 0;
    let mut scored: Vec<(ChessMove, i32)> = Vec::with_capacity(moves.len());
    for mv in &moves {
        let next = clone_and_apply_pseudo(state, mv);
        let score = chess_alpha_beta(&next, 3, i32::MIN, i32::MAX, !maximizing);
        scored.push((*mv, score));
    }

    let best_score = if maximizing {
        scored.iter().map(|(_, s)| *s).max().unwrap()
    } else {
        scored.iter().map(|(_, s)| *s).min().unwrap()
    };

    let best_moves: Vec<ChessMove> = scored
        .into_iter()
        .filter(|(_, s)| *s == best_score)
        .map(|(m, _)| m)
        .collect();

    if best_moves.len() == 1 {
        return Some(best_moves[0]);
    }
    let mut rng = rand::rng();
    best_moves.choose(&mut rng).copied()
}

fn chess_alpha_beta(
    state: &ChessState,
    depth: u8,
    mut alpha: i32,
    mut beta: i32,
    maximizing: bool,
) -> i32 {
    if let Some(outcome) = Chess::is_terminal(state) {
        return match outcome {
            GameOutcome::Win(pid) if pid == state.players[0] => 100_000 + depth as i32,
            GameOutcome::Win(_) => -(100_000 + depth as i32),
            GameOutcome::Draw => 0,
        };
    }
    if depth == 0 {
        return evaluate_chess(state);
    }

    let moves = legal_moves(state);
    if moves.is_empty() {
        let side = side_for_turn(state.current_turn);
        return if in_check(state, side) {
            if state.current_turn == 0 { -(100_000 + depth as i32) } else { 100_000 + depth as i32 }
        } else {
            0 // stalemate
        };
    }

    if maximizing {
        let mut value = i32::MIN;
        for mv in &moves {
            let next = clone_and_apply_pseudo(state, mv);
            let score = chess_alpha_beta(&next, depth - 1, alpha, beta, false);
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
            let next = clone_and_apply_pseudo(state, mv);
            let score = chess_alpha_beta(&next, depth - 1, alpha, beta, true);
            value = value.min(score);
            beta = beta.min(score);
            if alpha >= beta {
                break;
            }
        }
        value
    }
}

/// Material evaluation from White's perspective.
fn evaluate_chess(state: &ChessState) -> i32 {
    let mut score = 0i32;
    for p in state.board.iter().flatten().flatten() {
        let val = piece_value(p.kind);
        match p.side {
            Side::White => score += val,
            Side::Black => score -= val,
        }
    }
    score
}

fn piece_value(kind: PieceKind) -> i32 {
    match kind {
        PieceKind::Pawn   => 100,
        PieceKind::Knight => 300,
        PieceKind::Bishop => 300,
        PieceKind::Rook   => 500,
        PieceKind::Queen  => 900,
        PieceKind::King   => 100_000,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn two_players() -> [PlayerId; 2] {
        [PlayerId::new(), PlayerId::new()]
    }

    fn new_game() -> ChessState {
        let players = two_players();
        Chess::initial_state(&players, &GameSettings::default())
    }

    fn empty_state() -> ChessState {
        let players = two_players();
        let mut s = Chess::initial_state(&players, &GameSettings::default());
        s.board = [[None; 8]; 8];
        s.castle_rights = CastleRights {
            white_kingside: false,
            white_queenside: false,
            black_kingside: false,
            black_queenside: false,
        };
        s
    }

    fn mv(fr: u8, fc: u8, tr: u8, tc: u8) -> ChessMove {
        ChessMove {
            from: Position::new(fr, fc),
            to: Position::new(tr, tc),
            promotion: None,
        }
    }

    fn mv_promo(fr: u8, fc: u8, tr: u8, tc: u8, promo: PieceKind) -> ChessMove {
        ChessMove {
            from: Position::new(fr, fc),
            to: Position::new(tr, tc),
            promotion: Some(promo),
        }
    }

    #[test]
    fn initial_state_has_32_pieces() {
        let s = new_game();
        let count: usize = s.board.iter().flatten().filter(|sq| sq.is_some()).count();
        assert_eq!(count, 32);
    }

    #[test]
    fn white_moves_first() {
        let s = new_game();
        assert_eq!(Chess::current_player(&s), s.players[0]);
    }

    #[test]
    fn pawn_single_and_double_step() {
        let s = new_game();
        let p0 = s.players[0];
        // Single step: e2-e3 (row 1 col 4 → row 2 col 4)
        let single = mv(1, 4, 2, 4);
        assert!(Chess::validate_move(&s, p0, &single).is_ok(), "single step failed");
        // Double step: e2-e4
        let double = mv(1, 4, 3, 4);
        assert!(Chess::validate_move(&s, p0, &double).is_ok(), "double step failed");
    }

    #[test]
    fn pawn_blocked_cannot_move() {
        let mut s = new_game();
        // Place a black pawn in front of white e-pawn
        s.board[2][4] = Some(Piece { kind: PieceKind::Pawn, side: Side::Black });
        let p0 = s.players[0];
        assert!(Chess::validate_move(&s, p0, &mv(1, 4, 2, 4)).is_err());
        assert!(Chess::validate_move(&s, p0, &mv(1, 4, 3, 4)).is_err());
    }

    #[test]
    fn pawn_diagonal_capture() {
        let mut s = new_game();
        // Move white e-pawn to e4 (3,4), put black piece on d5 (4,3)
        s.board[3][4] = s.board[1][4].take();
        s.board[4][3] = Some(Piece { kind: PieceKind::Pawn, side: Side::Black });
        let p0 = s.players[0];
        // White captures diagonally: (3,4) → (4,3)
        assert!(Chess::validate_move(&s, p0, &mv(3, 4, 4, 3)).is_ok());
        // Cannot capture own piece
        s.board[4][5] = Some(Piece { kind: PieceKind::Pawn, side: Side::White });
        assert!(Chess::validate_move(&s, p0, &mv(3, 4, 4, 5)).is_err());
    }

    #[test]
    fn knight_moves_from_corner() {
        let mut s = empty_state();
        s.board[0][0] = Some(Piece { kind: PieceKind::Knight, side: Side::White });
        s.board[0][4] = Some(Piece { kind: PieceKind::King, side: Side::White });
        s.board[7][4] = Some(Piece { kind: PieceKind::King, side: Side::Black });
        let p0 = s.players[0];
        // From a1 (0,0), knight can go to (1,2) or (2,1)
        assert!(Chess::validate_move(&s, p0, &mv(0, 0, 1, 2)).is_ok());
        assert!(Chess::validate_move(&s, p0, &mv(0, 0, 2, 1)).is_ok());
        // Cannot go to adjacent square
        assert!(Chess::validate_move(&s, p0, &mv(0, 0, 0, 1)).is_err());
    }

    #[test]
    fn bishop_blocked_by_own_piece() {
        let s = new_game();
        let p0 = s.players[0];
        // White bishop at (0,2) is blocked by pawns at row 1
        assert!(Chess::validate_move(&s, p0, &mv(0, 2, 2, 4)).is_err());
    }

    #[test]
    fn cannot_move_into_check() {
        let mut s = empty_state();
        // White king at e1 (0,4). Black queen at (1,3) attacks (0,3) and (0,4).
        s.board[0][4] = Some(Piece { kind: PieceKind::King, side: Side::White });
        s.board[7][4] = Some(Piece { kind: PieceKind::King, side: Side::Black });
        s.board[1][3] = Some(Piece { kind: PieceKind::Queen, side: Side::Black });
        let p0 = s.players[0];
        // King moving to (0,3) would step into black queen's attack
        assert!(Chess::validate_move(&s, p0, &mv(0, 4, 0, 3)).is_err());
    }

    #[test]
    fn castling_kingside_white() {
        let mut s = new_game();
        // Clear squares between king and kingside rook
        s.board[0][5] = None;
        s.board[0][6] = None;
        let p0 = s.players[0];
        // King moves from (0,4) to (0,6) = kingside castle
        let castle = mv(0, 4, 0, 6);
        assert!(Chess::validate_move(&s, p0, &castle).is_ok(), "kingside castle should be legal");
        Chess::apply_move(&mut s, p0, &castle);
        assert_eq!(s.board[0][6], Some(Piece { kind: PieceKind::King, side: Side::White }));
        assert_eq!(s.board[0][5], Some(Piece { kind: PieceKind::Rook, side: Side::White }));
        assert!(s.board[0][7].is_none());
    }

    #[test]
    fn castling_blocked_when_king_in_check() {
        let mut s = new_game();
        // Clear kingside knight/bishop and white's e-pawn so the black rook actually pins the king
        s.board[0][5] = None;
        s.board[0][6] = None;
        s.board[1][4] = None;
        // Put black rook on e-file attacking white king
        s.board[3][4] = Some(Piece { kind: PieceKind::Rook, side: Side::Black });
        let p0 = s.players[0];
        let castle = mv(0, 4, 0, 6);
        assert!(Chess::validate_move(&s, p0, &castle).is_err(), "cannot castle while in check");
    }

    #[test]
    fn en_passant_capture() {
        let mut s = empty_state();
        // White pawn at (4,4), black pawn about to double-push from (6,3)
        s.board[4][4] = Some(Piece { kind: PieceKind::Pawn, side: Side::White });
        s.board[6][3] = Some(Piece { kind: PieceKind::Pawn, side: Side::Black });
        s.board[0][4] = Some(Piece { kind: PieceKind::King, side: Side::White });
        s.board[7][4] = Some(Piece { kind: PieceKind::King, side: Side::Black });
        s.current_turn = 1; // Black's turn

        let p1 = s.players[1];
        // Black double-pushes d7-d5 (row 6 → row 4)
        Chess::apply_move(&mut s, p1, &mv(6, 3, 4, 3));
        // En passant target should be (5,3)
        assert_eq!(s.en_passant_target, Some(Position::new(5, 3)));

        let p0 = s.players[0];
        // White captures en passant: (4,4) → (5,3)
        let ep_mv = mv(4, 4, 5, 3);
        assert!(Chess::validate_move(&s, p0, &ep_mv).is_ok());
        Chess::apply_move(&mut s, p0, &ep_mv);
        // White pawn on (5,3), black pawn at (4,3) removed
        assert_eq!(s.board[5][3], Some(Piece { kind: PieceKind::Pawn, side: Side::White }));
        assert!(s.board[4][3].is_none());
    }

    #[test]
    fn pawn_promotion_to_queen() {
        let mut s = empty_state();
        s.board[6][0] = Some(Piece { kind: PieceKind::Pawn, side: Side::White });
        s.board[0][4] = Some(Piece { kind: PieceKind::King, side: Side::White });
        s.board[7][4] = Some(Piece { kind: PieceKind::King, side: Side::Black });
        let p0 = s.players[0];
        let promo = mv_promo(6, 0, 7, 0, PieceKind::Queen);
        assert!(Chess::validate_move(&s, p0, &promo).is_ok());
        Chess::apply_move(&mut s, p0, &promo);
        assert_eq!(s.board[7][0], Some(Piece { kind: PieceKind::Queen, side: Side::White }));
    }

    #[test]
    fn fools_mate_is_checkmate() {
        // 1.f3 e5 2.g4 Qh4#
        let mut s = new_game();
        let [p0, p1] = [s.players[0], s.players[1]];

        // 1. f3 (white f-pawn: row1 col5 → row2 col5)
        Chess::apply_move(&mut s, p0, &mv(1, 5, 2, 5));
        // 1... e5 (black e-pawn: row6 col4 → row4 col4)
        Chess::apply_move(&mut s, p1, &mv(6, 4, 4, 4));
        // 2. g4 (white g-pawn: row1 col6 → row3 col6)
        Chess::apply_move(&mut s, p0, &mv(1, 6, 3, 6));
        // 2... Qh4# (black queen: row7 col3 → row3 col7 — h4 is row 3 since row 0 = rank 1)
        Chess::apply_move(&mut s, p1, &mv(7, 3, 3, 7));

        match Chess::is_terminal(&s) {
            Some(GameOutcome::Win(winner)) => assert_eq!(winner, p1, "black should win fool's mate"),
            other => panic!("expected checkmate (black wins), got {other:?}"),
        }
    }

    #[test]
    fn stalemate_is_draw() {
        // White king on a1 (0,0), black king on c2 (1,2), black queen on b3 (2,1)
        // White's turn; white king has no legal moves but is not in check.
        let mut s = empty_state();
        s.board[0][0] = Some(Piece { kind: PieceKind::King, side: Side::White });
        s.board[1][2] = Some(Piece { kind: PieceKind::King, side: Side::Black });
        s.board[2][1] = Some(Piece { kind: PieceKind::Queen, side: Side::Black });
        s.current_turn = 0;

        assert!(!in_check(&s, Side::White), "white should not be in check in stalemate");
        let moves = legal_moves(&s);
        assert!(moves.is_empty(), "white should have no legal moves: {moves:?}");

        match Chess::is_terminal(&s) {
            Some(GameOutcome::Draw) => {}
            other => panic!("expected stalemate draw, got {other:?}"),
        }
    }

    #[test]
    fn bot_easy_returns_legal_move() {
        let s = new_game();
        let mv_opt = bot_move(&s, Difficulty::Easy);
        assert!(mv_opt.is_some());
        let bot_mv = mv_opt.unwrap();
        assert!(Chess::validate_move(&s, s.players[0], &bot_mv).is_ok());
    }

    #[test]
    fn bot_hard_returns_legal_move() {
        let s = new_game();
        let mv_opt = bot_move(&s, Difficulty::Hard);
        assert!(mv_opt.is_some());
        let bot_mv = mv_opt.unwrap();
        assert!(Chess::validate_move(&s, s.players[0], &bot_mv).is_ok());
    }

    #[test]
    fn king_capture_is_never_legal() {
        // White rook on (0,0); black king sitting on (0,7) with no defender.
        // Even though the rook can pseudo-capture the king on the rank,
        // legal_moves and validate_move must reject it.
        let mut s = empty_state();
        s.board[0][0] = Some(Piece { kind: PieceKind::Rook, side: Side::White });
        s.board[0][4] = Some(Piece { kind: PieceKind::King, side: Side::White });
        s.board[0][7] = Some(Piece { kind: PieceKind::King, side: Side::Black });
        s.current_turn = 0;

        let p0 = s.players[0];
        let capture_king = mv(0, 0, 0, 7);
        assert!(
            Chess::validate_move(&s, p0, &capture_king).is_err(),
            "king-capture must be rejected by validate_move"
        );
        let legals = legal_moves(&s);
        assert!(
            !legals.iter().any(|m| m.to == Position::new(0, 7)),
            "legal_moves must never include a king capture"
        );
    }

    #[test]
    fn fifty_move_rule_draw() {
        let mut s = new_game();
        s.halfmove_clock = 100;
        match Chess::is_terminal(&s) {
            Some(GameOutcome::Draw) => {}
            other => panic!("expected draw from 50-move rule, got {other:?}"),
        }
    }
}
