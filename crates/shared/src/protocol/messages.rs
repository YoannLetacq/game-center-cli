use serde::{Deserialize, Serialize};

use crate::types::{
    GameOutcome, GameSettings, GameType, PlayerId, PlayerInfo, RoomId, RoomState, SessionId,
};

/// Wire envelope wrapping every message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope<P> {
    /// Protocol version for compatibility checks.
    pub version: u8,
    /// Sequence number for ordering and reconnection replay.
    pub seq: u64,
    /// The actual message payload.
    pub payload: P,
}

/// Messages sent from client to server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMsg {
    /// Register a new account.
    Register { username: String, password: String },
    /// Authenticate with existing credentials.
    Authenticate { token: String },
    /// Login with username/password to obtain a token.
    Login { username: String, password: String },
    /// Request list of available rooms.
    ListRooms,
    /// Create a new game room.
    CreateRoom {
        game_type: GameType,
        settings: GameSettings,
    },
    /// Join an existing room.
    JoinRoom { room_id: RoomId },
    /// Leave the current room.
    LeaveRoom,
    /// Submit a game action (opaque bytes, validated by game engine).
    GameAction { data: Vec<u8> },
    /// Keepalive ping.
    Ping,
    /// Reconnect to a previous session.
    Reconnect {
        session_id: SessionId,
        last_seq: u64,
    },
}

/// Messages sent from server to client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMsg {
    /// Authentication succeeded. Contains a JWT and its expiry (unix timestamp).
    AuthOk { token: String, expires_at: i64 },
    /// Authentication failed.
    AuthFail { reason: String },
    /// List of available rooms.
    RoomList(Vec<RoomSummary>),
    /// Successfully joined a room.
    RoomJoined {
        room_id: RoomId,
        players: Vec<PlayerInfo>,
        state: RoomState,
    },
    /// Full game state snapshot (for reconciliation or initial sync).
    GameStateUpdate { tick: u64, state_data: Vec<u8> },
    /// Incremental game update.
    GameDelta { tick: u64, delta_data: Vec<u8> },
    /// A player joined the room.
    PlayerJoined(PlayerInfo),
    /// A player left the room.
    PlayerLeft(PlayerId),
    /// Game over.
    GameOver { outcome: GameOutcome },
    /// Error response.
    Error { code: u16, message: String },
    /// Keepalive pong.
    Pong,
    /// Reconnection succeeded; contains missed messages to replay.
    ReconnectOk { missed_messages: Vec<ServerMsg> },
    /// Server version info, sent on connection.
    ServerVersion {
        version: String,
        min_client_protocol: u8,
    },
}

/// Summary of a room for the lobby list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomSummary {
    pub id: RoomId,
    pub game_type: GameType,
    pub player_count: u8,
    pub max_players: u8,
    pub state: RoomState,
    pub host_name: String,
}
