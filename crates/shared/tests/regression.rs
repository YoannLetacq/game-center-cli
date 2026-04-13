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
    use gc_shared::game::connect4::{COLS, Connect4, Connect4Move, Connect4State, ROWS};
    use gc_shared::game::tictactoe::{TicTacToe, TicTacToeMove, TicTacToeState};
    use gc_shared::game::traits::GameEngine;
    use gc_shared::protocol::codec::{decode, encode};
    use gc_shared::types::{GameSettings, PlayerId};

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
    use gc_shared::game::connect4::{Connect4, Connect4Move};
    use gc_shared::game::tictactoe::{TicTacToe, TicTacToeMove};
    use gc_shared::game::traits::GameEngine;
    use gc_shared::types::{GameOutcome, GameSettings, PlayerId};

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
}

// ============================================================
// 3. BOT: Every game bot must return valid moves
// ============================================================
mod bot_regression {
    use gc_shared::game::connect4::{self, Connect4};
    use gc_shared::game::tictactoe::{self, TicTacToe};
    use gc_shared::game::traits::GameEngine;
    use gc_shared::types::{Difficulty, GameSettings, PlayerId};

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
