//! Regression / retrocompatibility test suite.
//!
//! Run before every phase merge: `cargo test -p gc-shared --test regression`
//!
//! Covers: codec round-trips for all game states, engine invariants,
//! bot validity, protocol exhaustiveness, i18n parity, GameType consistency.
//!
//! When adding a new game, add corresponding tests to each section.

// ============================================================
// 1. CODEC: Every game state and move type must round-trip
// ============================================================
mod codec_regression {
    use gc_shared::game::checkers::{Checkers, CheckersMove, CheckersState, Position};
    use gc_shared::game::chess::{
        Chess, ChessMove, ChessState, PieceKind, Position as ChessPosition,
    };
    use gc_shared::game::connect4::{COLS, Connect4, Connect4Move, Connect4State, ROWS};
    use gc_shared::game::snake::{Direction, Snake, SnakeDelta, SnakeInput, SnakeState};
    use gc_shared::game::tictactoe::{TicTacToe, TicTacToeMove, TicTacToeState};
    use gc_shared::game::traits::GameEngine;
    use gc_shared::protocol::codec::{decode, encode};
    use gc_shared::types::{GameOutcome, GameSettings, PlayerId};
    use std::collections::VecDeque;

    fn two_players() -> [PlayerId; 2] {
        [PlayerId::new(), PlayerId::new()]
    }

    #[test]
    fn tictactoe_state_roundtrip() {
        let players = two_players();
        let mut state = TicTacToe::initial_state(&players, &GameSettings::default());
        TicTacToe::apply_move(&mut state, players[0], &TicTacToeMove { row: 1, col: 1 });

        let bytes = encode(&state).expect("encode TicTacToeState");
        let decoded: TicTacToeState = decode(&bytes).expect("decode TicTacToeState");

        assert_eq!(decoded.move_count, state.move_count);
        assert_eq!(decoded.current_turn, state.current_turn);
        assert_eq!(decoded.board, state.board);
    }

    #[test]
    fn tictactoe_move_roundtrip() {
        let mv = TicTacToeMove { row: 2, col: 0 };
        let bytes = encode(&mv).expect("encode");
        let decoded: TicTacToeMove = decode(&bytes).expect("decode");
        assert_eq!(decoded.row, mv.row);
        assert_eq!(decoded.col, mv.col);
    }

    #[test]
    fn connect4_state_roundtrip() {
        let players = two_players();
        let mut state = Connect4::initial_state(&players, &GameSettings::default());
        Connect4::apply_move(&mut state, players[0], &Connect4Move { col: 3 });
        Connect4::apply_move(&mut state, players[1], &Connect4Move { col: 4 });

        let bytes = encode(&state).expect("encode Connect4State");
        let decoded: Connect4State = decode(&bytes).expect("decode Connect4State");

        assert_eq!(decoded.move_count, state.move_count);
        assert_eq!(decoded.current_turn, state.current_turn);
        for row in 0..ROWS {
            for col in 0..COLS {
                assert_eq!(decoded.board[row][col], state.board[row][col]);
            }
        }
    }

    #[test]
    fn connect4_move_roundtrip() {
        let mv = Connect4Move { col: 5 };
        let bytes = encode(&mv).expect("encode");
        let decoded: Connect4Move = decode(&bytes).expect("decode");
        assert_eq!(decoded.col, mv.col);
    }

    #[test]
    fn checkers_state_roundtrip() {
        let players = two_players();
        let mut state = Checkers::initial_state(&players, &GameSettings::default());
        Checkers::apply_move(
            &mut state,
            players[0],
            &CheckersMove {
                path: vec![Position { row: 5, col: 0 }, Position { row: 4, col: 1 }],
            },
        );
        let bytes = encode(&state).expect("encode CheckersState");
        let decoded: CheckersState = decode(&bytes).expect("decode CheckersState");
        assert_eq!(decoded.move_count, state.move_count);
        assert_eq!(decoded.current_turn, state.current_turn);
        assert_eq!(decoded.plies_since_progress, state.plies_since_progress);
        for row in 0..8 {
            for col in 0..8 {
                assert_eq!(decoded.board[row][col], state.board[row][col]);
            }
        }
    }

    #[test]
    fn checkers_move_roundtrip() {
        let mv = CheckersMove {
            path: vec![
                Position { row: 5, col: 0 },
                Position { row: 3, col: 2 },
                Position { row: 1, col: 4 },
            ],
        };
        let bytes = encode(&mv).expect("encode");
        let decoded: CheckersMove = decode(&bytes).expect("decode");
        assert_eq!(decoded.path.len(), mv.path.len());
        for (a, b) in decoded.path.iter().zip(mv.path.iter()) {
            assert_eq!(a.row, b.row);
            assert_eq!(a.col, b.col);
        }
    }

    #[test]
    fn chess_state_roundtrip() {
        let players = two_players();
        let mut state = Chess::initial_state(&players, &GameSettings::default());
        Chess::apply_move(
            &mut state,
            players[0],
            &ChessMove {
                from: ChessPosition::new(1, 4),
                to: ChessPosition::new(3, 4),
                promotion: None,
            },
        );
        let bytes = encode(&state).expect("encode ChessState");
        let decoded: ChessState = decode(&bytes).expect("decode ChessState");
        assert_eq!(decoded.move_count, state.move_count);
        assert_eq!(decoded.current_turn, state.current_turn);
        assert_eq!(decoded.halfmove_clock, state.halfmove_clock);
        assert_eq!(decoded.en_passant_target, state.en_passant_target);
        for row in 0..8 {
            for col in 0..8 {
                assert_eq!(decoded.board[row][col], state.board[row][col]);
            }
        }
    }

    #[test]
    fn chess_move_roundtrip() {
        let mv = ChessMove {
            from: ChessPosition::new(6, 0),
            to: ChessPosition::new(7, 0),
            promotion: Some(PieceKind::Queen),
        };
        let bytes = encode(&mv).expect("encode");
        let decoded: ChessMove = decode(&bytes).expect("decode");
        assert_eq!(decoded.from, mv.from);
        assert_eq!(decoded.to, mv.to);
        assert_eq!(decoded.promotion, mv.promotion);
    }

    #[test]
    fn snake_input_roundtrip() {
        let input = SnakeInput {
            direction: Direction::Up,
        };
        let bytes = encode(&input).expect("encode SnakeInput");
        let decoded: SnakeInput = decode(&bytes).expect("decode SnakeInput");
        assert_eq!(decoded.direction, input.direction);
    }

    #[test]
    fn snake_state_roundtrip() {
        let pid_a = PlayerId::new();
        let pid_b = PlayerId::new();
        let body_a: VecDeque<_> = vec![
            gc_shared::game::snake::Position { x: 4, y: 9 },
            gc_shared::game::snake::Position { x: 3, y: 9 },
            gc_shared::game::snake::Position { x: 2, y: 9 },
        ]
        .into_iter()
        .collect();
        let body_b: VecDeque<_> = vec![
            gc_shared::game::snake::Position { x: 27, y: 9 },
            gc_shared::game::snake::Position { x: 28, y: 9 },
        ]
        .into_iter()
        .collect();
        let state = SnakeState {
            arena_w: 32,
            arena_h: 18,
            snakes: vec![
                Snake {
                    player_id: pid_a,
                    body: body_a,
                    direction: Direction::Right,
                    pending_direction: None,
                    alive: true,
                    score: 2,
                },
                Snake {
                    player_id: pid_b,
                    body: body_b,
                    direction: Direction::Left,
                    pending_direction: Some(Direction::Down),
                    alive: false,
                    score: 0,
                },
            ],
            food: vec![gc_shared::game::snake::Position { x: 10, y: 5 }],
            tick: 42,
            rng_seed: 99,
            rng_state: 12345,
            game_over: Some(GameOutcome::Win(pid_a)),
        };
        let bytes = encode(&state).expect("encode SnakeState");
        let decoded: SnakeState = decode(&bytes).expect("decode SnakeState");
        assert_eq!(decoded.tick, state.tick);
        assert_eq!(decoded.arena_w, state.arena_w);
        assert_eq!(decoded.arena_h, state.arena_h);
        assert_eq!(decoded.snakes.len(), 2);
        assert_eq!(decoded.snakes[0].score, 2);
        assert!(decoded.snakes[0].alive);
        assert!(!decoded.snakes[1].alive);
        assert_eq!(decoded.food.len(), 1);
        assert!(matches!(decoded.game_over, Some(GameOutcome::Win(_))));
    }

    #[test]
    fn snake_delta_roundtrip() {
        let pid = PlayerId::new();
        let delta = SnakeDelta {
            tick: 7,
            moves: vec![(pid, gc_shared::game::snake::Position { x: 5, y: 5 })],
            grew: vec![pid],
            deaths: vec![],
            new_food: vec![gc_shared::game::snake::Position { x: 15, y: 10 }],
            eaten_food: vec![gc_shared::game::snake::Position { x: 5, y: 5 }],
            game_over: None,
        };
        let bytes = encode(&delta).expect("encode SnakeDelta");
        let decoded: SnakeDelta = decode(&bytes).expect("decode SnakeDelta");
        assert_eq!(decoded.tick, delta.tick);
        assert_eq!(decoded.moves.len(), 1);
        assert_eq!(decoded.grew.len(), 1);
        assert!(decoded.deaths.is_empty());
        assert_eq!(decoded.new_food.len(), 1);
        assert_eq!(decoded.eaten_food.len(), 1);
        assert!(decoded.game_over.is_none());
    }

    #[test]
    fn game_settings_seed_roundtrip() {
        // With seed.
        let with_seed = GameSettings {
            seed: Some(42),
            ..GameSettings::default()
        };
        let bytes = encode(&with_seed).expect("encode GameSettings with seed");
        let decoded: GameSettings = decode(&bytes).expect("decode GameSettings with seed");
        assert_eq!(decoded.seed, Some(42));

        // Without seed — backward-compat: must deserialize with seed = None.
        let without_seed = GameSettings::default();
        let bytes2 = encode(&without_seed).expect("encode GameSettings default");
        let decoded2: GameSettings = decode(&bytes2).expect("decode GameSettings default");
        assert_eq!(decoded2.seed, None);
    }

    #[test]
    fn cross_decode_fails_gracefully() {
        let players = two_players();
        let c4_state = Connect4::initial_state(&players, &GameSettings::default());
        let bytes = encode(&c4_state).expect("encode");
        let result: Result<TicTacToeState, _> = decode(&bytes);
        assert!(
            result.is_err(),
            "decoding Connect4State as TicTacToeState must fail"
        );
    }
}

// ============================================================
// 2. GAME ENGINE: Invariants that must hold for ALL engines
// ============================================================
mod engine_invariants {
    use gc_shared::game::checkers::{Checkers, CheckersMove, Position};
    use gc_shared::game::chess::{Chess, ChessMove, Position as ChessPosition};
    use gc_shared::game::connect4::{Connect4, Connect4Move};
    use gc_shared::game::snake::{
        ARENA_H, ARENA_W, Direction, Snake, SnakeEngine, SnakeInput, SnakeState,
    };
    use gc_shared::game::tictactoe::{TicTacToe, TicTacToeMove};
    use gc_shared::game::traits::{GameEngine, RealtimeGameEngine};
    use gc_shared::types::{GameOutcome, GameSettings, PlayerId};
    use std::collections::{HashMap, VecDeque};

    fn dir_opposite(d: Direction) -> Direction {
        match d {
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
        }
    }

    fn two_players() -> [PlayerId; 2] {
        [PlayerId::new(), PlayerId::new()]
    }

    #[test]
    fn tictactoe_initial_invariants() {
        let players = two_players();
        let state = TicTacToe::initial_state(&players, &GameSettings::default());
        assert!(TicTacToe::is_terminal(&state).is_none());
        assert_eq!(TicTacToe::current_player(&state), players[0]);
    }

    #[test]
    fn connect4_initial_invariants() {
        let players = two_players();
        let state = Connect4::initial_state(&players, &GameSettings::default());
        assert!(Connect4::is_terminal(&state).is_none());
        assert_eq!(Connect4::current_player(&state), players[0]);
    }

    #[test]
    fn tictactoe_wrong_player_rejected() {
        let players = two_players();
        let state = TicTacToe::initial_state(&players, &GameSettings::default());
        assert!(
            TicTacToe::validate_move(&state, players[1], &TicTacToeMove { row: 0, col: 0 })
                .is_err()
        );
    }

    #[test]
    fn connect4_wrong_player_rejected() {
        let players = two_players();
        let state = Connect4::initial_state(&players, &GameSettings::default());
        assert!(Connect4::validate_move(&state, players[1], &Connect4Move { col: 0 }).is_err());
    }

    #[test]
    fn tictactoe_turn_alternates() {
        let players = two_players();
        let mut state = TicTacToe::initial_state(&players, &GameSettings::default());
        assert_eq!(TicTacToe::current_player(&state), players[0]);
        TicTacToe::apply_move(&mut state, players[0], &TicTacToeMove { row: 0, col: 0 });
        assert_eq!(TicTacToe::current_player(&state), players[1]);
    }

    #[test]
    fn connect4_turn_alternates() {
        let players = two_players();
        let mut state = Connect4::initial_state(&players, &GameSettings::default());
        assert_eq!(Connect4::current_player(&state), players[0]);
        Connect4::apply_move(&mut state, players[0], &Connect4Move { col: 3 });
        assert_eq!(Connect4::current_player(&state), players[1]);
    }

    #[test]
    fn tictactoe_terminal_has_valid_outcome() {
        let players = two_players();
        let mut state = TicTacToe::initial_state(&players, &GameSettings::default());
        TicTacToe::apply_move(&mut state, players[0], &TicTacToeMove { row: 0, col: 0 });
        TicTacToe::apply_move(&mut state, players[1], &TicTacToeMove { row: 1, col: 0 });
        TicTacToe::apply_move(&mut state, players[0], &TicTacToeMove { row: 0, col: 1 });
        TicTacToe::apply_move(&mut state, players[1], &TicTacToeMove { row: 1, col: 1 });
        TicTacToe::apply_move(&mut state, players[0], &TicTacToeMove { row: 0, col: 2 });

        match TicTacToe::is_terminal(&state) {
            Some(GameOutcome::Win(pid)) => assert!(pid == players[0] || pid == players[1]),
            Some(GameOutcome::Draw) => {}
            None => panic!("expected terminal state"),
        }
    }

    #[test]
    fn connect4_terminal_has_valid_outcome() {
        let players = two_players();
        let mut state = Connect4::initial_state(&players, &GameSettings::default());
        for _ in 0..3 {
            Connect4::apply_move(&mut state, players[0], &Connect4Move { col: 0 });
            Connect4::apply_move(&mut state, players[1], &Connect4Move { col: 1 });
        }
        Connect4::apply_move(&mut state, players[0], &Connect4Move { col: 0 });

        match Connect4::is_terminal(&state) {
            Some(GameOutcome::Win(pid)) => assert!(pid == players[0] || pid == players[1]),
            Some(GameOutcome::Draw) => {}
            None => panic!("expected terminal state"),
        }
    }

    #[test]
    fn checkers_initial_invariants() {
        let players = two_players();
        let state = Checkers::initial_state(&players, &GameSettings::default());
        assert!(Checkers::is_terminal(&state).is_none());
        assert_eq!(Checkers::current_player(&state), players[0]);
    }

    #[test]
    fn checkers_wrong_player_rejected() {
        let players = two_players();
        let state = Checkers::initial_state(&players, &GameSettings::default());
        let mv = CheckersMove {
            path: vec![Position { row: 5, col: 0 }, Position { row: 4, col: 1 }],
        };
        assert!(Checkers::validate_move(&state, players[1], &mv).is_err());
    }

    #[test]
    fn checkers_turn_alternates() {
        let players = two_players();
        let mut state = Checkers::initial_state(&players, &GameSettings::default());
        assert_eq!(Checkers::current_player(&state), players[0]);
        Checkers::apply_move(
            &mut state,
            players[0],
            &CheckersMove {
                path: vec![Position { row: 5, col: 0 }, Position { row: 4, col: 1 }],
            },
        );
        assert_eq!(Checkers::current_player(&state), players[1]);
    }

    #[test]
    fn chess_initial_invariants() {
        let players = two_players();
        let state = Chess::initial_state(&players, &GameSettings::default());
        assert!(Chess::is_terminal(&state).is_none());
        assert_eq!(Chess::current_player(&state), players[0]);
    }

    #[test]
    fn chess_wrong_player_rejected() {
        let players = two_players();
        let state = Chess::initial_state(&players, &GameSettings::default());
        let mv = ChessMove {
            from: ChessPosition::new(1, 4),
            to: ChessPosition::new(3, 4),
            promotion: None,
        };
        assert!(Chess::validate_move(&state, players[1], &mv).is_err());
    }

    #[test]
    fn chess_turn_alternates() {
        let players = two_players();
        let mut state = Chess::initial_state(&players, &GameSettings::default());
        assert_eq!(Chess::current_player(&state), players[0]);
        Chess::apply_move(
            &mut state,
            players[0],
            &ChessMove {
                from: ChessPosition::new(1, 4),
                to: ChessPosition::new(3, 4),
                promotion: None,
            },
        );
        assert_eq!(Chess::current_player(&state), players[1]);
    }

    // Helper: play fool's mate (1. f3 e5 2. g4 Qh4#) and return the final state.
    fn play_fools_mate(players: &[gc_shared::types::PlayerId; 2]) -> gc_shared::game::chess::ChessState {
        use gc_shared::game::chess::Chess;
        use gc_shared::game::traits::GameEngine;
        let mut state = Chess::initial_state(players, &gc_shared::types::GameSettings::default());
        let [p0, p1] = [players[0], players[1]];
        // 1. f3
        Chess::apply_move(&mut state, p0, &ChessMove { from: ChessPosition::new(1, 5), to: ChessPosition::new(2, 5), promotion: None });
        // 1... e5
        Chess::apply_move(&mut state, p1, &ChessMove { from: ChessPosition::new(6, 4), to: ChessPosition::new(4, 4), promotion: None });
        // 2. g4
        Chess::apply_move(&mut state, p0, &ChessMove { from: ChessPosition::new(1, 6), to: ChessPosition::new(3, 6), promotion: None });
        // 2... Qh4#
        Chess::apply_move(&mut state, p1, &ChessMove { from: ChessPosition::new(7, 3), to: ChessPosition::new(3, 7), promotion: None });
        state
    }

    #[test]
    fn chess_terminal_has_valid_outcome() {
        let players = two_players();
        // Position before the mating move must not be terminal.
        let mut state = Chess::initial_state(&players, &GameSettings::default());
        let [p0, p1] = [players[0], players[1]];
        Chess::apply_move(&mut state, p0, &ChessMove { from: ChessPosition::new(1, 5), to: ChessPosition::new(2, 5), promotion: None });
        Chess::apply_move(&mut state, p1, &ChessMove { from: ChessPosition::new(6, 4), to: ChessPosition::new(4, 4), promotion: None });
        Chess::apply_move(&mut state, p0, &ChessMove { from: ChessPosition::new(1, 6), to: ChessPosition::new(3, 6), promotion: None });
        assert!(Chess::is_terminal(&state).is_none(), "position before mating move must not be terminal");

        // Now deliver the mating move.
        Chess::apply_move(&mut state, p1, &ChessMove { from: ChessPosition::new(7, 3), to: ChessPosition::new(3, 7), promotion: None });
        match Chess::is_terminal(&state) {
            Some(GameOutcome::Win(pid)) => assert!(pid == players[0] || pid == players[1], "win must be one of the two players"),
            Some(GameOutcome::Draw) => panic!("fool's mate must not be a draw"),
            None => panic!("expected terminal state after fool's mate"),
        }
    }

    #[test]
    fn chess_checkmate_overrides_50_move_rule() {
        let players = two_players();
        let mut state = play_fools_mate(&players);
        // Manually set halfmove_clock to 100 (50-move rule threshold) after checkmate.
        state.halfmove_clock = 100;
        // Checkmate must take precedence over the 50-move draw claim (FIDE rule).
        match Chess::is_terminal(&state) {
            Some(GameOutcome::Win(_)) => {}
            other => panic!("checkmate must override 50-move rule, got {other:?}"),
        }
    }

    #[test]
    fn chess_en_passant_target_not_set_when_no_capturer() {
        let players = two_players();

        // Case 1: white e4 with no black pawn adjacent at row 3 → en_passant_target is None.
        let mut state = Chess::initial_state(&players, &GameSettings::default());
        Chess::apply_move(
            &mut state,
            players[0],
            &ChessMove { from: ChessPosition::new(1, 4), to: ChessPosition::new(3, 4), promotion: None },
        );
        assert_eq!(state.en_passant_target, None, "no adjacent enemy pawn means no EP target");

        // Case 2: place a black pawn at d4 (row 3, col 3) adjacent to e4 landing square.
        // White plays e4 from initial; the black pawn must be at row 3 col 3 before the push.
        let mut state2 = Chess::initial_state(&players, &GameSettings::default());
        // Clear black d-pawn from starting position and plant it at d4 so it sits adjacent to e4.
        state2.board[6][3] = None;
        state2.board[3][3] = Some(gc_shared::game::chess::Piece {
            kind: gc_shared::game::chess::PieceKind::Pawn,
            side: gc_shared::game::chess::Side::Black,
        });
        Chess::apply_move(
            &mut state2,
            players[0],
            &ChessMove { from: ChessPosition::new(1, 4), to: ChessPosition::new(3, 4), promotion: None },
        );
        // e3 = row 2 col 4; engine stores ep target as (ep_row, from_col) = (2, 4).
        assert_eq!(
            state2.en_passant_target,
            Some(ChessPosition::new(2, 4)),
            "black pawn at d4 adjacent to e4 must set EP target to e3"
        );
    }

    #[test]
    fn snake_cannot_reverse_180_via_tick() {
        let players = two_players();
        let mut state = SnakeEngine::initial_state(
            &players,
            &GameSettings {
                seed: Some(1),
                ..GameSettings::default()
            },
        );
        // Snake 0 starts facing Right; attempt reversal to Left.
        let original_dir = state.snakes[0].direction;
        let mut inputs = HashMap::new();
        inputs.insert(
            players[0],
            SnakeInput {
                direction: dir_opposite(original_dir),
            },
        );
        SnakeEngine::tick(&mut state, &inputs);
        assert_eq!(state.snakes[0].direction, original_dir);
    }

    #[test]
    fn snake_wall_collision_kills_via_tick() {
        let pid = PlayerId::new();
        // Snake at top row (y=0) facing Up — next tick hits wall.
        let body: VecDeque<_> = vec![
            gc_shared::game::snake::Position { x: 5, y: 0 },
            gc_shared::game::snake::Position { x: 5, y: 1 },
        ]
        .into_iter()
        .collect();
        let mut state = SnakeState {
            arena_w: ARENA_W,
            arena_h: ARENA_H,
            snakes: vec![Snake {
                player_id: pid,
                body,
                direction: Direction::Up,
                pending_direction: None,
                alive: true,
                score: 0,
            }],
            food: Vec::new(),
            tick: 0,
            rng_seed: 1,
            rng_state: 1,
            game_over: None,
        };
        SnakeEngine::tick(&mut state, &HashMap::new());
        assert!(!state.snakes[0].alive);
    }

    #[test]
    fn snake_head_on_collision_kills_both_via_tick() {
        let pid_a = PlayerId::new();
        let pid_b = PlayerId::new();
        // Two snakes one cell apart, facing each other.
        let body_a: VecDeque<_> = vec![
            gc_shared::game::snake::Position { x: 10, y: 10 },
            gc_shared::game::snake::Position { x: 9, y: 10 },
        ]
        .into_iter()
        .collect();
        let body_b: VecDeque<_> = vec![
            gc_shared::game::snake::Position { x: 12, y: 10 },
            gc_shared::game::snake::Position { x: 13, y: 10 },
        ]
        .into_iter()
        .collect();
        let mut state = SnakeState {
            arena_w: ARENA_W,
            arena_h: ARENA_H,
            snakes: vec![
                Snake {
                    player_id: pid_a,
                    body: body_a,
                    direction: Direction::Right,
                    pending_direction: None,
                    alive: true,
                    score: 0,
                },
                Snake {
                    player_id: pid_b,
                    body: body_b,
                    direction: Direction::Left,
                    pending_direction: None,
                    alive: true,
                    score: 0,
                },
            ],
            food: Vec::new(),
            tick: 0,
            rng_seed: 1,
            rng_state: 1,
            game_over: None,
        };
        SnakeEngine::tick(&mut state, &HashMap::new());
        assert!(!state.snakes[0].alive, "snake A should be dead");
        assert!(!state.snakes[1].alive, "snake B should be dead");
        assert!(
            matches!(state.game_over, Some(GameOutcome::Draw)),
            "head-on collision must produce Draw"
        );
    }

    #[test]
    fn snake_food_growth_via_tick() {
        let pid = PlayerId::new();
        let food_pos = gc_shared::game::snake::Position { x: 6, y: 5 };
        let body: VecDeque<_> = vec![
            gc_shared::game::snake::Position { x: 5, y: 5 },
            gc_shared::game::snake::Position { x: 4, y: 5 },
        ]
        .into_iter()
        .collect();
        let initial_len = body.len();
        let mut state = SnakeState {
            arena_w: ARENA_W,
            arena_h: ARENA_H,
            snakes: vec![Snake {
                player_id: pid,
                body,
                direction: Direction::Right,
                pending_direction: None,
                alive: true,
                score: 0,
            }],
            food: vec![food_pos],
            tick: 0,
            rng_seed: 123,
            rng_state: 123,
            game_over: None,
        };
        let delta = SnakeEngine::tick(&mut state, &HashMap::new());
        assert_eq!(
            state.snakes[0].body.len(),
            initial_len + 1,
            "body should grow by 1"
        );
        assert!(delta.grew.contains(&pid));
        assert_eq!(delta.eaten_food, vec![food_pos]);
        // A new food should have been spawned.
        assert_eq!(delta.new_food.len(), 1);
        assert_eq!(state.food.len(), 1);
    }

    #[test]
    fn snake_deterministic_seed() {
        let players = vec![PlayerId::new(), PlayerId::new()];
        let settings = GameSettings {
            seed: Some(777),
            ..GameSettings::default()
        };
        let s1 = SnakeEngine::initial_state(&players, &settings);
        let s2 = SnakeEngine::initial_state(&players, &settings);
        assert_eq!(s1.rng_seed, s2.rng_seed);
        assert_eq!(s1.rng_state, s2.rng_state);
        assert_eq!(s1.food, s2.food);
        assert_eq!(s1.tick, s2.tick);

        // Apply identical tick sequences and compare deltas.
        let mut state1 = s1;
        let mut state2 = s2;
        for _ in 0..5 {
            let d1 = SnakeEngine::tick(&mut state1, &HashMap::new());
            let d2 = SnakeEngine::tick(&mut state2, &HashMap::new());
            assert_eq!(d1.tick, d2.tick);
            assert_eq!(d1.new_food, d2.new_food);
            assert_eq!(d1.moves.len(), d2.moves.len());
        }
    }

    #[test]
    fn snake_terminal_has_valid_outcome() {
        let players = vec![PlayerId::new(), PlayerId::new()];
        let settings = GameSettings {
            seed: Some(42),
            ..GameSettings::default()
        };
        let mut state = SnakeEngine::initial_state(&players, &settings);
        // Drive snake[0] into the wall to force a death.
        let pid0 = state.snakes[0].player_id;
        let mut inputs = HashMap::new();
        inputs.insert(
            pid0,
            SnakeInput {
                direction: Direction::Up,
            },
        );
        for _ in 0..ARENA_H + 2 {
            if SnakeEngine::is_terminal(&state).is_some() {
                break;
            }
            SnakeEngine::tick(&mut state, &inputs);
        }
        let outcome = SnakeEngine::is_terminal(&state);
        assert!(
            outcome.is_some(),
            "game must be terminal after a snake dies"
        );
        match outcome.unwrap() {
            GameOutcome::Win(pid) => {
                assert!(players.contains(&pid), "winner must be one of the players")
            }
            GameOutcome::Draw => {}
        }
        // All dead snakes cannot be the sole alive snake.
        let alive_count = state.snakes.iter().filter(|s| s.alive).count();
        assert!(
            alive_count <= 1,
            "at most one snake can be alive at terminal"
        );
    }
}

// ============================================================
// 3. BOT: Every game bot must return valid moves
// ============================================================
mod bot_regression {
    use gc_shared::game::checkers::{self, Checkers};
    use gc_shared::game::chess::{self, Chess};
    use gc_shared::game::connect4::{self, Connect4};
    use gc_shared::game::snake::{self, Direction, SnakeEngine};
    use gc_shared::game::tictactoe::{self, TicTacToe};
    use gc_shared::game::traits::{GameEngine, RealtimeGameEngine};
    use gc_shared::types::{Difficulty, GameSettings, PlayerId};
    use std::collections::HashMap;

    fn dir_opposite(d: Direction) -> Direction {
        match d {
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
        }
    }

    fn two_players() -> [PlayerId; 2] {
        [PlayerId::new(), PlayerId::new()]
    }

    #[test]
    fn tictactoe_bot_moves_are_valid() {
        let players = two_players();
        let state = TicTacToe::initial_state(&players, &GameSettings::default());
        for difficulty in [Difficulty::Easy, Difficulty::Hard] {
            let mv = tictactoe::bot_move(&state, difficulty);
            assert!(
                TicTacToe::validate_move(&state, players[0], &mv).is_ok(),
                "TicTacToe bot ({difficulty:?}) returned invalid move"
            );
        }
    }

    #[test]
    fn connect4_bot_moves_are_valid() {
        let players = two_players();
        let state = Connect4::initial_state(&players, &GameSettings::default());
        for difficulty in [Difficulty::Easy, Difficulty::Hard] {
            let mv = connect4::bot_move(&state, difficulty);
            assert!(
                Connect4::validate_move(&state, players[0], &mv).is_ok(),
                "Connect4 bot ({difficulty:?}) returned invalid move"
            );
        }
    }

    #[test]
    fn tictactoe_full_bot_game_no_panic() {
        let players = two_players();
        let mut state = TicTacToe::initial_state(&players, &GameSettings::default());
        for _ in 0..9 {
            if TicTacToe::is_terminal(&state).is_some() {
                break;
            }
            let mv = tictactoe::bot_move(&state, Difficulty::Hard);
            let player = TicTacToe::current_player(&state);
            TicTacToe::apply_move(&mut state, player, &mv);
        }
        assert!(TicTacToe::is_terminal(&state).is_some());
    }

    #[test]
    fn connect4_full_bot_game_no_panic() {
        let players = two_players();
        let mut state = Connect4::initial_state(&players, &GameSettings::default());
        for _ in 0..42 {
            if Connect4::is_terminal(&state).is_some() {
                break;
            }
            let mv = connect4::bot_move(&state, Difficulty::Easy);
            let player = Connect4::current_player(&state);
            Connect4::apply_move(&mut state, player, &mv);
        }
        assert!(Connect4::is_terminal(&state).is_some());
    }

    #[test]
    fn checkers_bot_moves_are_valid() {
        let players = two_players();
        let state = Checkers::initial_state(&players, &GameSettings::default());
        for difficulty in [Difficulty::Easy, Difficulty::Hard] {
            let mv = checkers::bot_move(&state, difficulty);
            assert!(
                Checkers::validate_move(&state, players[0], &mv).is_ok(),
                "Checkers bot ({difficulty:?}) returned invalid move"
            );
        }
    }

    #[test]
    fn checkers_full_bot_game_no_panic() {
        // Easy-vs-Easy loop: every bot move must be legal. Cap at 500 plies
        // to guard against a pathological draw spin.
        let players = two_players();
        let mut state = Checkers::initial_state(&players, &GameSettings::default());
        for _ in 0..500 {
            if Checkers::is_terminal(&state).is_some() {
                break;
            }
            let player = Checkers::current_player(&state);
            let mv = checkers::bot_move(&state, Difficulty::Easy);
            assert!(
                Checkers::validate_move(&state, player, &mv).is_ok(),
                "Checkers Easy bot returned an illegal move"
            );
            Checkers::apply_move(&mut state, player, &mv);
        }
    }

    #[test]
    fn chess_bot_moves_are_valid() {
        let players = two_players();
        let state = Chess::initial_state(&players, &GameSettings::default());
        for difficulty in [Difficulty::Easy, Difficulty::Hard] {
            let mv = chess::bot_move(&state, difficulty).expect("bot returns legal move");
            assert!(
                Chess::validate_move(&state, players[0], &mv).is_ok(),
                "Chess bot ({difficulty:?}) returned invalid move"
            );
        }
    }

    #[test]
    fn snake_bot_moves_are_valid() {
        // Run bot_move on 200 seeded initial states; direction must never be
        // a 180-degree reversal of the snake's current direction.
        for seed in 0u64..200 {
            let players = two_players();
            let settings = GameSettings {
                seed: Some(seed),
                ..GameSettings::default()
            };
            let state = SnakeEngine::initial_state(&players, &settings);
            let pid0 = state.snakes[0].player_id;
            let pid1 = state.snakes[1].player_id;
            for &pid in &[pid0, pid1] {
                let snake = state.snakes.iter().find(|s| s.player_id == pid).unwrap();
                let current = snake.direction;
                for &diff in &[Difficulty::Easy, Difficulty::Hard] {
                    let mv = snake::bot_move(&state, pid, diff);
                    assert_ne!(
                        mv.direction,
                        dir_opposite(current),
                        "seed={seed} pid={pid:?} diff={diff:?}: bot returned 180 reversal"
                    );
                }
            }
        }
    }

    #[test]
    fn chess_easy_bot_game_no_illegal_moves() {
        // Easy-vs-Easy loop capped at 80 plies; every move must validate.
        let players = two_players();
        let mut state = Chess::initial_state(&players, &GameSettings::default());
        for _ in 0..80 {
            if Chess::is_terminal(&state).is_some() {
                break;
            }
            let player = Chess::current_player(&state);
            let mv = chess::bot_move(&state, Difficulty::Easy).expect("legal move available");
            assert!(
                Chess::validate_move(&state, player, &mv).is_ok(),
                "Chess Easy bot returned an illegal move"
            );
            Chess::apply_move(&mut state, player, &mv);
        }
    }

    #[test]
    fn snake_easy_bot_game_no_illegal_moves() {
        // Both players driven by Easy bot for up to 400 ticks.
        // Must not panic, and must either reach terminal or have alive snakes.
        let players = two_players();
        let settings = GameSettings {
            seed: Some(12345),
            ..GameSettings::default()
        };
        let mut state = SnakeEngine::initial_state(&players, &settings);
        for _ in 0..400 {
            if SnakeEngine::is_terminal(&state).is_some() {
                break;
            }
            let mut inputs = HashMap::new();
            for snake in &state.snakes {
                if !snake.alive {
                    continue;
                }
                let mv = snake::bot_move(&state, snake.player_id, Difficulty::Easy);
                inputs.insert(snake.player_id, mv);
            }
            SnakeEngine::tick(&mut state, &inputs);
        }
        // Game either reached terminal or still has alive snakes — no panic is the key assertion.
        let terminal = SnakeEngine::is_terminal(&state);
        let alive = state.snakes.iter().any(|s| s.alive);
        assert!(
            terminal.is_some() || alive,
            "game should be terminal or have alive snakes after bot run"
        );
    }
}

// ============================================================
// 4. PROTOCOL: All message variants round-trip
// ============================================================
mod protocol_regression {
    use gc_shared::protocol::codec::{decode, encode};
    use gc_shared::protocol::messages::{ClientMsg, Envelope, ServerMsg};
    use gc_shared::protocol::version::PROTOCOL_VERSION;
    use gc_shared::types::{
        GameOutcome, GameSettings, GameType, PlayerId, PlayerInfo, RoomId, RoomState, SessionId,
    };

    #[test]
    fn all_client_msg_variants_roundtrip() {
        let variants: Vec<ClientMsg> = vec![
            ClientMsg::Register {
                username: "u".into(),
                password: "p".into(),
            },
            ClientMsg::Login {
                username: "u".into(),
                password: "p".into(),
            },
            ClientMsg::Authenticate { token: "t".into() },
            ClientMsg::ListRooms,
            ClientMsg::CreateRoom {
                game_type: GameType::Connect4,
                settings: GameSettings::default(),
            },
            ClientMsg::JoinRoom {
                room_id: RoomId::new(),
            },
            ClientMsg::LeaveRoom,
            ClientMsg::GameAction {
                data: vec![1, 2, 3],
            },
            ClientMsg::Ping,
            ClientMsg::Reconnect {
                session_id: SessionId::new(),
                last_seq: 42,
            },
            ClientMsg::RequestRematch,
            ClientMsg::RematchResponse { accept: true },
            ClientMsg::RematchResponse { accept: false },
        ];

        for (i, msg) in variants.into_iter().enumerate() {
            let envelope = Envelope {
                version: PROTOCOL_VERSION,
                seq: i as u64,
                payload: msg,
            };
            let bytes = encode(&envelope).unwrap_or_else(|e| panic!("encode ClientMsg[{i}]: {e}"));
            let _: Envelope<ClientMsg> =
                decode(&bytes).unwrap_or_else(|e| panic!("decode ClientMsg[{i}]: {e}"));
        }
    }

    #[test]
    fn all_server_msg_variants_roundtrip() {
        let pid = PlayerId::new();
        let variants: Vec<ServerMsg> = vec![
            ServerMsg::AuthOk {
                token: "t".into(),
                expires_at: 99,
                player_id: pid,
            },
            ServerMsg::AuthFail {
                reason: "bad".into(),
            },
            ServerMsg::RoomList(vec![
                gc_shared::protocol::messages::RoomSummary {
                    id: RoomId::new(),
                    game_type: GameType::Connect4,
                    player_count: 1,
                    max_players: 2,
                    state: RoomState::Waiting,
                    host_name: "u".into(),
                },
                gc_shared::protocol::messages::RoomSummary {
                    id: RoomId::new(),
                    game_type: GameType::TicTacToe,
                    player_count: 2,
                    max_players: 2,
                    state: RoomState::InProgress,
                    host_name: "u2".into(),
                },
            ]),
            ServerMsg::RoomJoined {
                room_id: RoomId::new(),
                players: vec![PlayerInfo {
                    id: pid,
                    username: "u".into(),
                }],
                state: RoomState::Waiting,
            },
            ServerMsg::GameStateUpdate {
                tick: 1,
                state_data: vec![0xAB],
            },
            ServerMsg::GameDelta {
                tick: 2,
                delta_data: vec![0xCD],
            },
            ServerMsg::PlayerJoined(PlayerInfo {
                id: pid,
                username: "u".into(),
            }),
            ServerMsg::PlayerLeft(pid),
            ServerMsg::GameOver {
                outcome: GameOutcome::Draw,
            },
            ServerMsg::Error {
                code: 404,
                message: "not found".into(),
            },
            ServerMsg::Pong,
            ServerMsg::ReconnectOk {
                missed_messages: vec![],
            },
            ServerMsg::ServerVersion {
                version: "0.1.0".into(),
                min_client_protocol: 1,
            },
            ServerMsg::RematchRequested,
            ServerMsg::RematchAccepted,
            ServerMsg::RematchDeclined,
        ];

        for (i, msg) in variants.into_iter().enumerate() {
            let envelope = Envelope {
                version: PROTOCOL_VERSION,
                seq: i as u64,
                payload: msg,
            };
            let bytes = encode(&envelope).unwrap_or_else(|e| panic!("encode ServerMsg[{i}]: {e}"));
            let _: Envelope<ServerMsg> =
                decode(&bytes).unwrap_or_else(|e| panic!("decode ServerMsg[{i}]: {e}"));
        }
    }
}

// ============================================================
// 5. I18N: Key parity between languages
// ============================================================
mod i18n_regression {
    use gc_shared::i18n::{Language, Translator};

    #[test]
    fn all_used_keys_exist_in_both_languages() {
        let en = Translator::new(Language::English);
        let fr = Translator::new(Language::French);

        let keys = [
            "app.title",
            "app.quit",
            "app.back",
            "app.confirm",
            "app.cancel",
            "login.title",
            "login.username",
            "login.password",
            "login.error_empty_username",
            "login.error_short_password",
            "lobby.title",
            "lobby.rooms",
            "lobby.create_room",
            "lobby.join_room",
            "lobby.no_rooms",
            "lobby.refresh",
            "game.your_turn",
            "game.opponent_turn",
            "game.you_win",
            "game.you_lose",
            "game.draw",
            "game.rematch",
            "game.leave",
            "errors.connection_failed",
        ];

        for key in keys {
            let en_val = en.get(key);
            let fr_val = fr.get(key);
            assert_ne!(en_val, key, "English missing key '{key}'");
            assert_ne!(fr_val, key, "French missing key '{key}'");
        }
    }
}

// ============================================================
// 6. GAMETYPE: All variants displayable and serializable
// ============================================================
mod gametype_regression {
    use gc_shared::protocol::codec::{decode, encode};
    use gc_shared::types::GameType;

    #[test]
    fn all_game_types_display_and_roundtrip() {
        let types = [
            GameType::TicTacToe,
            GameType::Connect4,
            GameType::Checkers,
            GameType::Chess,
            GameType::Snake,
            GameType::BlockBreaker,
            GameType::Pacman,
        ];
        for gt in types {
            assert!(!format!("{gt}").is_empty(), "{gt:?} display is empty");
            let bytes = encode(&gt).expect("encode GameType");
            let decoded: GameType = decode(&bytes).expect("decode GameType");
            assert_eq!(decoded, gt);
        }
    }
}
