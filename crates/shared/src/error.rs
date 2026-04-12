use thiserror::Error;

#[derive(Debug, Error)]
pub enum GameCenterError {
    #[error("codec error: {0}")]
    Codec(String),

    #[error("authentication failed: {0}")]
    AuthFailed(String),

    #[error("invalid move: {0}")]
    InvalidMove(String),

    #[error("room error: {0}")]
    Room(String),

    #[error("connection error: {0}")]
    Connection(String),

    #[error("database error: {0}")]
    Database(String),

    #[error("config error: {0}")]
    Config(String),

    #[error("protocol version mismatch: client={client}, server_min={server_min}")]
    VersionMismatch { client: u8, server_min: u8 },
}
