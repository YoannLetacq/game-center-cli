use std::sync::mpsc;
use std::thread;

use gc_shared::protocol::messages::{ClientMsg, ServerMsg};
use tracing::{error, info};

use super::connection::Connection;

/// Commands sent from the TUI thread to the network thread.
#[derive(Debug)]
#[allow(dead_code)] // AuthWithToken used when token restore is wired
pub enum NetCommand {
    /// Connect to server and register.
    Register {
        server_url: String,
        username: String,
        password: String,
    },
    /// Connect to server and login.
    Login {
        server_url: String,
        username: String,
        password: String,
    },
    /// Authenticate with an existing token.
    AuthWithToken { server_url: String, token: String },
    /// List available rooms.
    ListRooms,
    /// Create a new room.
    CreateRoom {
        game_type: gc_shared::types::GameType,
        settings: gc_shared::types::GameSettings,
    },
    /// Join a room.
    JoinRoom { room_id: gc_shared::types::RoomId },
    /// Leave the current room.
    LeaveRoom,
    /// Disconnect.
    Disconnect,
}

/// Events sent from the network thread back to the TUI.
#[derive(Debug)]
pub enum NetEvent {
    /// Successfully connected and authenticated.
    Connected,
    /// Authentication response from server.
    AuthResult(ServerMsg),
    /// Room list received.
    RoomList(Vec<gc_shared::protocol::messages::RoomSummary>),
    /// Joined a room.
    RoomJoined {
        room_id: gc_shared::types::RoomId,
        players: Vec<gc_shared::types::PlayerInfo>,
    },
    /// A player joined the room we're in.
    PlayerJoined(gc_shared::types::PlayerInfo),
    /// A player left the room we're in.
    PlayerLeft(gc_shared::types::PlayerId),
    /// Server error.
    Error(String),
    /// Connection lost.
    Disconnected,
    /// Generic server message (for anything not specifically handled).
    #[allow(dead_code)]
    ServerMessage(ServerMsg),
}

/// Manages the network thread and communication channels.
pub struct NetworkClient {
    cmd_tx: mpsc::Sender<NetCommand>,
    pub event_rx: mpsc::Receiver<NetEvent>,
    _handle: thread::JoinHandle<()>,
}

impl NetworkClient {
    /// Spawn a new network client with a background tokio runtime.
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<NetCommand>();
        let (event_tx, event_rx) = mpsc::channel::<NetEvent>();

        let handle = thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to create tokio runtime");

            rt.block_on(async move {
                network_loop(cmd_rx, event_tx).await;
            });
        });

        Self {
            cmd_tx,
            event_rx,
            _handle: handle,
        }
    }

    /// Send a command to the network thread.
    pub fn send(&self, cmd: NetCommand) -> Result<(), String> {
        self.cmd_tx
            .send(cmd)
            .map_err(|e| format!("network send failed: {e}"))
    }

    /// Try to receive a network event (non-blocking).
    pub fn try_recv(&self) -> Option<NetEvent> {
        self.event_rx.try_recv().ok()
    }
}

async fn network_loop(cmd_rx: mpsc::Receiver<NetCommand>, event_tx: mpsc::Sender<NetEvent>) {
    let mut conn: Option<Connection> = None;

    loop {
        // Check for commands (non-blocking in async context)
        let cmd = match cmd_rx.try_recv() {
            Ok(cmd) => Some(cmd),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => break,
        };

        if let Some(cmd) = cmd {
            match cmd {
                NetCommand::Register {
                    server_url,
                    username,
                    password,
                } => {
                    conn = handle_connect_and_auth(
                        &server_url,
                        ClientMsg::Register { username, password },
                        &event_tx,
                    )
                    .await;
                }
                NetCommand::Login {
                    server_url,
                    username,
                    password,
                } => {
                    conn = handle_connect_and_auth(
                        &server_url,
                        ClientMsg::Login { username, password },
                        &event_tx,
                    )
                    .await;
                }
                NetCommand::AuthWithToken { server_url, token } => {
                    conn = handle_connect_and_auth(
                        &server_url,
                        ClientMsg::Authenticate { token },
                        &event_tx,
                    )
                    .await;
                }
                NetCommand::ListRooms => {
                    if let Some(ref mut c) = conn {
                        let _ = c.send(ClientMsg::ListRooms).await;
                    }
                }
                NetCommand::CreateRoom {
                    game_type,
                    settings,
                } => {
                    if let Some(ref mut c) = conn {
                        let _ = c
                            .send(ClientMsg::CreateRoom {
                                game_type,
                                settings,
                            })
                            .await;
                    }
                }
                NetCommand::JoinRoom { room_id } => {
                    if let Some(ref mut c) = conn {
                        let _ = c.send(ClientMsg::JoinRoom { room_id }).await;
                    }
                }
                NetCommand::LeaveRoom => {
                    if let Some(ref mut c) = conn {
                        let _ = c.send(ClientMsg::LeaveRoom).await;
                    }
                }
                NetCommand::Disconnect => {
                    if let Some(ref mut c) = conn {
                        c.close().await;
                    }
                    drop(conn);
                    let _ = event_tx.send(NetEvent::Disconnected);
                    break;
                }
            }
        }

        // Check for incoming server messages
        if let Some(ref mut c) = conn {
            // Use a short timeout to avoid blocking the command loop
            match tokio::time::timeout(std::time::Duration::from_millis(10), c.recv()).await {
                Ok(Some(msg)) => {
                    dispatch_server_msg(msg, &event_tx);
                }
                Ok(None) => {
                    // Connection closed
                    info!("server connection closed");
                    conn = None;
                    let _ = event_tx.send(NetEvent::Disconnected);
                }
                Err(_) => {} // Timeout, no message — continue loop
            }
        } else {
            // No connection, sleep briefly to avoid busy-waiting
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }
}

async fn handle_connect_and_auth(
    server_url: &str,
    auth_msg: ClientMsg,
    event_tx: &mpsc::Sender<NetEvent>,
) -> Option<Connection> {
    let mut c = match Connection::connect(server_url).await {
        Ok(c) => c,
        Err(e) => {
            error!("connection failed: {e}");
            let _ = event_tx.send(NetEvent::Error(e));
            return None;
        }
    };

    // Read the ServerVersion message first
    if let Some(msg) = c.recv().await {
        match msg {
            ServerMsg::ServerVersion { version, .. } => {
                info!(server_version = %version, "connected to server");
            }
            other => {
                let _ = event_tx.send(NetEvent::ServerMessage(other));
            }
        }
    }

    // Send auth message
    if let Err(e) = c.send(auth_msg).await {
        let _ = event_tx.send(NetEvent::Error(e));
        return None;
    }

    // Wait for auth response
    if let Some(msg) = c.recv().await {
        match &msg {
            ServerMsg::AuthOk { .. } => {
                let _ = event_tx.send(NetEvent::AuthResult(msg));
                let _ = event_tx.send(NetEvent::Connected);
                return Some(c);
            }
            ServerMsg::AuthFail { reason } => {
                let _ = event_tx.send(NetEvent::Error(reason.clone()));
                return None;
            }
            _ => {
                let _ = event_tx.send(NetEvent::AuthResult(msg));
            }
        }
    }

    None
}

fn dispatch_server_msg(msg: ServerMsg, event_tx: &mpsc::Sender<NetEvent>) {
    let event = match msg {
        ServerMsg::RoomList(rooms) => NetEvent::RoomList(rooms),
        ServerMsg::RoomJoined {
            room_id, players, ..
        } => NetEvent::RoomJoined { room_id, players },
        ServerMsg::PlayerJoined(info) => NetEvent::PlayerJoined(info),
        ServerMsg::PlayerLeft(id) => NetEvent::PlayerLeft(id),
        ServerMsg::Error { message, .. } => NetEvent::Error(message),
        ServerMsg::AuthFail { reason } => NetEvent::Error(reason),
        other => NetEvent::ServerMessage(other),
    };
    let _ = event_tx.send(event);
}
