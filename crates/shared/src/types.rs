use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlayerId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RoomId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub Uuid);

impl PlayerId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for PlayerId {
    fn default() -> Self {
        Self::new()
    }
}

impl RoomId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for RoomId {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for PlayerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for RoomId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GameType {
    TicTacToe,
    Connect4,
    Checkers,
    Chess,
    Snake,
    BlockBreaker,
    Pacman,
}

impl fmt::Display for GameType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GameType::TicTacToe => write!(f, "Tic-Tac-Toe"),
            GameType::Connect4 => write!(f, "Connect 4"),
            GameType::Checkers => write!(f, "Checkers"),
            GameType::Chess => write!(f, "Chess"),
            GameType::Snake => write!(f, "Snake"),
            GameType::BlockBreaker => write!(f, "Block Breaker"),
            GameType::Pacman => write!(f, "Pacman"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Difficulty {
    Easy,
    Hard,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameSettings {
    pub difficulty: Option<Difficulty>,
    pub max_players: u8,
    pub turn_timeout_secs: Option<u64>,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            difficulty: None,
            max_players: 2,
            turn_timeout_secs: Some(60),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerInfo {
    pub id: PlayerId,
    pub username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameOutcome {
    Win(PlayerId),
    Draw,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RoomState {
    Waiting,
    InProgress,
    Finished,
}
