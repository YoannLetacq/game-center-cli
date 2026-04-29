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
    /// Request a rematch after a game ends.
    RequestRematch,
    /// Respond to an incoming rematch request.
    RematchResponse { accept: bool },
    /// Cancel a previously sent rematch request — the requester gives up
    /// waiting without leaving the room. The server forwards a
    /// `RematchCanceled` to other room members so their incoming-request
    /// modal disappears.
    CancelRematch,
}

/// Messages sent from server to client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMsg {
    /// Authentication succeeded.
    AuthOk {
        token: String,
        expires_at: i64,
        player_id: crate::types::PlayerId,
    },
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
    /// Opponent has requested a rematch.
    RematchRequested,
    /// Rematch accepted — a new GameStateUpdate will follow immediately.
    RematchAccepted,
    /// Rematch declined — both players are removed from the room.
    RematchDeclined,
    /// The opponent canceled their pending rematch request. Clients receiving
    /// this should clear the incoming "Y/N" overlay and return to the
    /// game-over screen.
    RematchCanceled,
    /// Game type for the room the client just joined or created.
    /// Sent immediately after `RoomJoined` so the client knows which game to display
    /// without inspecting the room list. Additive — backward-compatible.
    RoomGameType {
        room_id: RoomId,
        game_type: GameType,
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
