use serde::{Serialize, de::DeserializeOwned};

use crate::error::GameCenterError;

/// Encode a message to MessagePack bytes.
pub fn encode<T: Serialize>(msg: &T) -> Result<Vec<u8>, GameCenterError> {
    rmp_serde::to_vec(msg).map_err(|e| GameCenterError::Codec(e.to_string()))
}

/// Decode a message from MessagePack bytes.
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, GameCenterError> {
    rmp_serde::from_slice(bytes).map_err(|e| GameCenterError::Codec(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::messages::{ClientMsg, Envelope, ServerMsg};
    use crate::protocol::version::PROTOCOL_VERSION;
    use crate::types::{GameSettings, GameType, RoomId, SessionId};

    #[test]
    fn roundtrip_client_msg() {
        let envelope = Envelope {
            version: PROTOCOL_VERSION,
            seq: 42,
            payload: ClientMsg::CreateRoom {
                game_type: GameType::TicTacToe,
                settings: GameSettings::default(),
            },
        };

        let bytes = encode(&envelope).expect("encode failed");
        let decoded: Envelope<ClientMsg> = decode(&bytes).expect("decode failed");

        assert_eq!(decoded.version, PROTOCOL_VERSION);
        assert_eq!(decoded.seq, 42);
        match decoded.payload {
            ClientMsg::CreateRoom { game_type, .. } => {
                assert_eq!(game_type, GameType::TicTacToe);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn roundtrip_server_msg() {
        let envelope = Envelope {
            version: PROTOCOL_VERSION,
            seq: 1,
            payload: ServerMsg::AuthOk {
                token: "test-jwt".to_string(),
                expires_at: 1700000000,
            },
        };

        let bytes = encode(&envelope).expect("encode failed");
        let decoded: Envelope<ServerMsg> = decode(&bytes).expect("decode failed");

        assert_eq!(decoded.seq, 1);
        match decoded.payload {
            ServerMsg::AuthOk { token, expires_at } => {
                assert_eq!(token, "test-jwt");
                assert_eq!(expires_at, 1700000000);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn roundtrip_reconnect() {
        let session_id = SessionId::new();
        let envelope = Envelope {
            version: PROTOCOL_VERSION,
            seq: 99,
            payload: ClientMsg::Reconnect {
                session_id,
                last_seq: 50,
            },
        };

        let bytes = encode(&envelope).expect("encode failed");
        let decoded: Envelope<ClientMsg> = decode(&bytes).expect("decode failed");

        match decoded.payload {
            ClientMsg::Reconnect {
                session_id: sid,
                last_seq,
            } => {
                assert_eq!(sid, session_id);
                assert_eq!(last_seq, 50);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn roundtrip_all_game_types() {
        for game_type in [
            GameType::TicTacToe,
            GameType::Connect4,
            GameType::Checkers,
            GameType::Chess,
            GameType::Snake,
            GameType::BlockBreaker,
            GameType::Pacman,
        ] {
            let envelope = Envelope {
                version: PROTOCOL_VERSION,
                seq: 0,
                payload: ClientMsg::CreateRoom {
                    game_type,
                    settings: GameSettings::default(),
                },
            };
            let bytes = encode(&envelope).expect("encode failed");
            let decoded: Envelope<ClientMsg> = decode(&bytes).expect("decode failed");
            match decoded.payload {
                ClientMsg::CreateRoom { game_type: gt, .. } => assert_eq!(gt, game_type),
                _ => panic!("wrong variant"),
            }
        }
    }

    #[test]
    fn decode_garbage_returns_error() {
        let result: Result<Envelope<ClientMsg>, _> = decode(&[0xFF, 0x00, 0xDE, 0xAD]);
        assert!(result.is_err());
    }

    #[test]
    fn roundtrip_room_list() {
        let rooms = vec![crate::protocol::messages::RoomSummary {
            id: RoomId::new(),
            game_type: GameType::Chess,
            player_count: 1,
            max_players: 2,
            state: crate::types::RoomState::Waiting,
            host_name: "Alice".to_string(),
        }];

        let envelope = Envelope {
            version: PROTOCOL_VERSION,
            seq: 5,
            payload: ServerMsg::RoomList(rooms),
        };

        let bytes = encode(&envelope).expect("encode failed");
        let decoded: Envelope<ServerMsg> = decode(&bytes).expect("decode failed");

        match decoded.payload {
            ServerMsg::RoomList(rooms) => {
                assert_eq!(rooms.len(), 1);
                assert_eq!(rooms[0].game_type, GameType::Chess);
                assert_eq!(rooms[0].host_name, "Alice");
            }
            _ => panic!("wrong variant"),
        }
    }
}
