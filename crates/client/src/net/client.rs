use std::sync::mpsc as std_mpsc;
use std::thread;

use gc_shared::protocol::messages::{ClientMsg, ServerMsg};
use tokio::sync::mpsc;
use tracing::{error, info};

use super::connection::Connection;

/// Commands sent from the TUI thread to the network thread.
#[derive(Debug)]
#[allow(dead_code)] // AuthWithToken used when token restore is wired
pub enum NetCommand {
    Register {
        server_url: String,
        username: String,
        password: String,
    },
    Login {
        server_url: String,
        username: String,
        password: String,
    },
    AuthWithToken {
        server_url: String,
        token: String,
    },
    ListRooms,
    CreateRoom {
        game_type: gc_shared::types::GameType,
        settings: gc_shared::types::GameSettings,
    },
    JoinRoom {
        room_id: gc_shared::types::RoomId,
    },
    LeaveRoom,
    GameAction {
        data: Vec<u8>,
    },
    RequestRematch,
    RematchResponse {
        accept: bool,
    },
    Disconnect,
}

/// Events sent from the network thread back to the TUI.
#[derive(Debug)]
pub enum NetEvent {
    Connected,
    AuthResult(ServerMsg),
    RoomList(Vec<gc_shared::protocol::messages::RoomSummary>),
    RoomJoined {
        room_id: gc_shared::types::RoomId,
        players: Vec<gc_shared::types::PlayerInfo>,
        state: gc_shared::types::RoomState,
    },
    PlayerJoined(gc_shared::types::PlayerInfo),
    PlayerLeft(gc_shared::types::PlayerId),
    GameStateUpdate {
        state_data: Vec<u8>,
    },
    GameOver {
        outcome: gc_shared::types::GameOutcome,
    },
    Error(String),
    Disconnected,
    /// Opponent has sent a rematch request — show accept/decline modal.
    RematchRequested,
    /// Rematch accepted — a new GameStateUpdate follows.
    RematchAccepted,
    /// Rematch declined — both players return to lobby.
    RematchDeclined,
    #[allow(dead_code)]
    ServerMessage(ServerMsg),
}

/// Manages the network thread and communication channels.
pub struct NetworkClient {
    /// Async-compatible sender for commands (tokio mpsc).
    cmd_tx: mpsc::UnboundedSender<NetCommand>,
    /// Std receiver for events (read by TUI thread).
    pub event_rx: std_mpsc::Receiver<NetEvent>,
    _handle: thread::JoinHandle<()>,
}

impl NetworkClient {
    pub fn new() -> Self {
        // tokio mpsc for commands: TUI (sync) -> network (async)
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<NetCommand>();
        // std mpsc for events: network (async) -> TUI (sync)
        let (event_tx, event_rx) = std_mpsc::channel::<NetEvent>();

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

    pub fn send(&self, cmd: NetCommand) -> Result<(), String> {
        self.cmd_tx
            .send(cmd)
            .map_err(|e| format!("network send failed: {e}"))
    }

    pub fn try_recv(&self) -> Option<NetEvent> {
        self.event_rx.try_recv().ok()
    }
}

async fn network_loop(
    mut cmd_rx: mpsc::UnboundedReceiver<NetCommand>,
    event_tx: std_mpsc::Sender<NetEvent>,
) {
    let mut conn: Option<Connection> = None;

    loop {
        match conn.as_mut() {
            Some(c) => {
                // Connected: select between commands and incoming messages
                tokio::select! {
                    cmd = cmd_rx.recv() => {
                        let Some(cmd) = cmd else { break };
                        if handle_command(cmd, &mut conn, &event_tx).await {
                            break;
                        }
                    }
                    msg = c.recv() => {
                        match msg {
                            Some(msg) => dispatch_server_msg(msg, &event_tx),
                            None => {
                                info!("server connection closed");
                                conn = None;
                                let _ = event_tx.send(NetEvent::Disconnected);
                            }
                        }
                    }
                }
            }
            None => {
                // Not connected: just wait for commands
                let Some(cmd) = cmd_rx.recv().await else {
                    break;
                };
                if handle_command(cmd, &mut conn, &event_tx).await {
                    break;
                }
            }
        }
    }
}

/// Handle a single command. Returns true if we should exit the loop.
async fn handle_command(
    cmd: NetCommand,
    conn: &mut Option<Connection>,
    event_tx: &std_mpsc::Sender<NetEvent>,
) -> bool {
    match cmd {
        NetCommand::Register {
            server_url,
            username,
            password,
        } => {
            // Close existing connection before reconnecting
            if let Some(c) = conn {
                c.close().await;
            }
            *conn = handle_connect_and_auth(
                &server_url,
                ClientMsg::Register { username, password },
                event_tx,
            )
            .await;
        }
        NetCommand::Login {
            server_url,
            username,
            password,
        } => {
            if let Some(c) = conn {
                c.close().await;
            }
            *conn = handle_connect_and_auth(
                &server_url,
                ClientMsg::Login { username, password },
                event_tx,
            )
            .await;
        }
        NetCommand::AuthWithToken { server_url, token } => {
            if let Some(c) = conn {
                c.close().await;
            }
            *conn =
                handle_connect_and_auth(&server_url, ClientMsg::Authenticate { token }, event_tx)
                    .await;
        }
        NetCommand::ListRooms => {
            if let Some(c) = conn {
                let _ = c.send(ClientMsg::ListRooms).await;
            }
        }
        NetCommand::CreateRoom {
            game_type,
            settings,
        } => {
            if let Some(c) = conn {
                let _ = c
                    .send(ClientMsg::CreateRoom {
                        game_type,
                        settings,
                    })
                    .await;
            }
        }
        NetCommand::JoinRoom { room_id } => {
            if let Some(c) = conn {
                let _ = c.send(ClientMsg::JoinRoom { room_id }).await;
            }
        }
        NetCommand::GameAction { data } => {
            if let Some(c) = conn {
                let _ = c.send(ClientMsg::GameAction { data }).await;
            }
        }
        NetCommand::LeaveRoom => {
            if let Some(c) = conn {
                let _ = c.send(ClientMsg::LeaveRoom).await;
            }
        }
        NetCommand::RequestRematch => {
            if let Some(c) = conn {
                let _ = c.send(ClientMsg::RequestRematch).await;
            }
        }
        NetCommand::RematchResponse { accept } => {
            if let Some(c) = conn {
                let _ = c.send(ClientMsg::RematchResponse { accept }).await;
            }
        }
        NetCommand::Disconnect => {
            if let Some(c) = conn {
                c.close().await;
            }
            let _ = event_tx.send(NetEvent::Disconnected);
            return true;
        }
    }
    false
}

async fn handle_connect_and_auth(
    server_url: &str,
    auth_msg: ClientMsg,
    event_tx: &std_mpsc::Sender<NetEvent>,
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

fn dispatch_server_msg(msg: ServerMsg, event_tx: &std_mpsc::Sender<NetEvent>) {
    let event = match msg {
        ServerMsg::RoomList(rooms) => NetEvent::RoomList(rooms),
        ServerMsg::RoomJoined {
            room_id,
            players,
            state,
        } => NetEvent::RoomJoined {
            room_id,
            players,
            state,
        },
        ServerMsg::PlayerJoined(info) => NetEvent::PlayerJoined(info),
        ServerMsg::PlayerLeft(id) => NetEvent::PlayerLeft(id),
        ServerMsg::GameStateUpdate { state_data, .. } => NetEvent::GameStateUpdate { state_data },
        ServerMsg::GameOver { outcome } => NetEvent::GameOver { outcome },
        ServerMsg::Error { message, .. } => NetEvent::Error(message),
        ServerMsg::AuthFail { reason } => NetEvent::Error(reason),
        ServerMsg::RematchRequested => NetEvent::RematchRequested,
        ServerMsg::RematchAccepted => NetEvent::RematchAccepted,
        ServerMsg::RematchDeclined => NetEvent::RematchDeclined,
        other => NetEvent::ServerMessage(other),
    };
    let _ = event_tx.send(event);
}
