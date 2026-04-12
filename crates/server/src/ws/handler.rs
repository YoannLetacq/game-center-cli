use std::collections::HashMap;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::{RwLock, mpsc};
use tokio_rustls::server::TlsStream;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

use gc_shared::protocol::codec;
use gc_shared::protocol::messages::{ClientMsg, Envelope, ServerMsg};
use gc_shared::protocol::version::{MIN_CLIENT_VERSION, PROTOCOL_VERSION};
use gc_shared::types::{PlayerId, PlayerInfo};
use uuid::Uuid;

use crate::auth::jwt::JwtManager;
use crate::database::Database;
use crate::lobby::manager::LobbyManager;
use crate::ws::session::Session;

/// Registry of connected players for broadcasting messages.
pub type PlayerRegistry = Arc<RwLock<HashMap<PlayerId, mpsc::UnboundedSender<ServerMsg>>>>;

/// Shared state passed to each connection handler.
pub struct ServerState {
    pub db: Database,
    pub jwt: JwtManager,
    pub lobby: LobbyManager,
    pub players: PlayerRegistry,
}

/// Result of handling a client message: direct response + broadcasts to other players.
struct HandleResult {
    /// Message to send back to the requesting client.
    response: Option<ServerMsg>,
    /// Messages to broadcast to specific other players.
    broadcasts: Vec<(PlayerId, ServerMsg)>,
}

impl HandleResult {
    fn reply(msg: ServerMsg) -> Self {
        Self {
            response: Some(msg),
            broadcasts: Vec::new(),
        }
    }

    #[allow(dead_code)]
    fn with_broadcasts(response: ServerMsg, broadcasts: Vec<(PlayerId, ServerMsg)>) -> Self {
        Self {
            response: Some(response),
            broadcasts,
        }
    }
}

/// Handle a single WebSocket connection over TLS.
pub async fn handle_connection(tls_stream: TlsStream<TcpStream>, state: Arc<ServerState>) {
    let ws_stream = match tokio_tungstenite::accept_async(tls_stream).await {
        Ok(ws) => ws,
        Err(e) => {
            error!("WebSocket handshake failed: {e}");
            return;
        }
    };

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    let mut session = Session::new();

    // Channel for receiving broadcast messages from other connections
    let (broadcast_tx, mut broadcast_rx) = mpsc::unbounded_channel::<ServerMsg>();

    info!(session_id = %session.session_id, "new connection");

    // Send server version as first message
    let version_msg = ServerMsg::ServerVersion {
        version: env!("CARGO_PKG_VERSION").to_string(),
        min_client_protocol: MIN_CLIENT_VERSION,
    };
    if let Err(e) = send_msg(&mut ws_sender, &mut session, &version_msg).await {
        error!("failed to send version: {e}");
        return;
    }

    loop {
        tokio::select! {
            // Incoming WebSocket message from this client
            msg_result = ws_receiver.next() => {
                let Some(msg_result) = msg_result else { break };
                let msg = match msg_result {
                    Ok(msg) => msg,
                    Err(e) => {
                        warn!(session_id = %session.session_id, "connection error: {e}");
                        break;
                    }
                };

                match msg {
                    Message::Binary(data) => {
                        let envelope: Envelope<ClientMsg> = match codec::decode(&data) {
                            Ok(e) => e,
                            Err(e) => {
                                warn!("bad message: {e}");
                                continue;
                            }
                        };

                        if envelope.version < MIN_CLIENT_VERSION {
                            let _ = send_msg(
                                &mut ws_sender,
                                &mut session,
                                &ServerMsg::Error {
                                    code: 426,
                                    message: format!(
                                        "protocol version {} too old, minimum is {}",
                                        envelope.version, MIN_CLIENT_VERSION
                                    ),
                                },
                            )
                            .await;
                            break;
                        }

                        let result = handle_client_msg(
                            &envelope.payload,
                            &mut session,
                            &state,
                            &broadcast_tx,
                        )
                        .await;

                        // Send direct response to this client
                        if let Some(resp) = result.response
                            && let Err(e) = send_msg(&mut ws_sender, &mut session, &resp).await
                        {
                            error!("send error: {e}");
                            break;
                        }

                        // Send broadcasts to other players
                        for (target_id, msg) in result.broadcasts {
                            let players = state.players.read().await;
                            if let Some(tx) = players.get(&target_id) {
                                let _ = tx.send(msg);
                            }
                        }
                    }
                    Message::Ping(data) => {
                        let _ = ws_sender.send(Message::Pong(data)).await;
                    }
                    Message::Close(_) => break,
                    _ => {}
                }

                session.touch();
            }
            // Broadcast message from another connection
            Some(msg) = broadcast_rx.recv() => {
                if let Err(e) = send_msg(&mut ws_sender, &mut session, &msg).await {
                    error!("broadcast send error: {e}");
                    break;
                }
            }
        }
    }

    // Clean up: unregister from player registry
    if let Some(player_id) = session.player_id {
        state.players.write().await.remove(&player_id);

        // Remove from room and notify remaining players
        if let Some((room_id, _)) = state.lobby.leave_room(player_id).await {
            info!(%room_id, %player_id, "player removed from room on disconnect");

            // Notify remaining room members
            if let Some(room_players) = state.lobby.get_room_players(room_id).await {
                let leave_msg = ServerMsg::PlayerLeft(player_id);
                let players = state.players.read().await;
                for p in &room_players {
                    if let Some(tx) = players.get(&p.id) {
                        let _ = tx.send(leave_msg.clone());
                    }
                }
            }
        }
    }

    info!(session_id = %session.session_id, "connection closed");
}

async fn handle_client_msg(
    msg: &ClientMsg,
    session: &mut Session,
    state: &ServerState,
    broadcast_tx: &mpsc::UnboundedSender<ServerMsg>,
) -> HandleResult {
    match msg {
        ClientMsg::Register { username, password } => {
            let resp = handle_register(username, password, session, state).await;
            // Register in player registry on successful auth
            if let Some(ServerMsg::AuthOk { .. }) = &resp
                && let Some(pid) = session.player_id
            {
                state
                    .players
                    .write()
                    .await
                    .insert(pid, broadcast_tx.clone());
            }
            HandleResult {
                response: resp,
                broadcasts: Vec::new(),
            }
        }
        ClientMsg::Login { username, password } => {
            let resp = handle_login(username, password, session, state).await;
            // Register in player registry on successful auth
            if let Some(ServerMsg::AuthOk { .. }) = &resp
                && let Some(pid) = session.player_id
            {
                state
                    .players
                    .write()
                    .await
                    .insert(pid, broadcast_tx.clone());
            }
            HandleResult {
                response: resp,
                broadcasts: Vec::new(),
            }
        }
        ClientMsg::Authenticate { token } => {
            let resp = handle_authenticate(token, session, state);
            if let Some(ServerMsg::AuthOk { .. }) = &resp
                && let Some(pid) = session.player_id
            {
                state
                    .players
                    .write()
                    .await
                    .insert(pid, broadcast_tx.clone());
            }
            HandleResult {
                response: resp,
                broadcasts: Vec::new(),
            }
        }
        ClientMsg::Ping => HandleResult::reply(ServerMsg::Pong),
        ClientMsg::ListRooms => {
            if !session.authenticated {
                return HandleResult::reply(ServerMsg::AuthFail {
                    reason: "not authenticated".to_string(),
                });
            }
            let rooms = state.lobby.list_rooms().await;
            HandleResult::reply(ServerMsg::RoomList(rooms))
        }
        ClientMsg::CreateRoom {
            game_type,
            settings,
        } => {
            let player_id = match session.player_id {
                Some(id) => id,
                None => {
                    return HandleResult::reply(ServerMsg::AuthFail {
                        reason: "not authenticated".to_string(),
                    });
                }
            };
            let player = PlayerInfo {
                id: player_id,
                username: session.username.clone().unwrap_or_default(),
            };
            match state
                .lobby
                .create_room(*game_type, settings.clone(), player)
                .await
            {
                Ok(room_id) => {
                    session.current_room = Some(room_id);
                    let players = state
                        .lobby
                        .get_room_players(room_id)
                        .await
                        .unwrap_or_default();
                    HandleResult::reply(ServerMsg::RoomJoined {
                        room_id,
                        players,
                        state: gc_shared::types::RoomState::Waiting,
                    })
                }
                Err(reason) => HandleResult::reply(ServerMsg::Error {
                    code: 400,
                    message: reason,
                }),
            }
        }
        ClientMsg::JoinRoom { room_id } => {
            let player_id = match session.player_id {
                Some(id) => id,
                None => {
                    return HandleResult::reply(ServerMsg::AuthFail {
                        reason: "not authenticated".to_string(),
                    });
                }
            };
            let joiner = PlayerInfo {
                id: player_id,
                username: session.username.clone().unwrap_or_default(),
            };

            // Get existing players BEFORE joining (to know who to notify)
            let existing_players = state
                .lobby
                .get_room_players(*room_id)
                .await
                .unwrap_or_default();

            match state.lobby.join_room(*room_id, joiner.clone()).await {
                Ok(all_players) => {
                    session.current_room = Some(*room_id);

                    // Broadcast PlayerJoined to existing players
                    let mut broadcasts: Vec<(PlayerId, ServerMsg)> = existing_players
                        .iter()
                        .filter(|p| p.id != player_id)
                        .map(|p| (p.id, ServerMsg::PlayerJoined(joiner.clone())))
                        .collect();

                    // Check if game auto-started (room became full)
                    let room_state = {
                        let games = state.lobby.games.read().await;
                        if let Some(game) = games.get(room_id) {
                            let state_data = game.encode_state();
                            for player in &all_players {
                                broadcasts.push((
                                    player.id,
                                    ServerMsg::GameStateUpdate {
                                        tick: 0,
                                        state_data: state_data.clone(),
                                    },
                                ));
                            }
                            gc_shared::types::RoomState::InProgress
                        } else {
                            gc_shared::types::RoomState::Waiting
                        }
                    };

                    HandleResult {
                        response: Some(ServerMsg::RoomJoined {
                            room_id: *room_id,
                            players: all_players,
                            state: room_state,
                        }),
                        broadcasts,
                    }
                }
                Err(reason) => HandleResult::reply(ServerMsg::Error {
                    code: 400,
                    message: reason,
                }),
            }
        }
        ClientMsg::LeaveRoom => {
            let player_id = match session.player_id {
                Some(id) => id,
                None => {
                    return HandleResult::reply(ServerMsg::AuthFail {
                        reason: "not authenticated".to_string(),
                    });
                }
            };

            // Get room players BEFORE leaving (to know who to notify)
            let room_id = session.current_room;
            let room_players = if let Some(rid) = room_id {
                state.lobby.get_room_players(rid).await.unwrap_or_default()
            } else {
                Vec::new()
            };

            session.current_room = None;
            if let Some((_rid, _is_empty)) = state.lobby.leave_room(player_id).await {
                // Broadcast PlayerLeft to remaining room members
                let broadcasts: Vec<(PlayerId, ServerMsg)> = room_players
                    .iter()
                    .filter(|p| p.id != player_id)
                    .map(|p| (p.id, ServerMsg::PlayerLeft(player_id)))
                    .collect();

                HandleResult {
                    response: Some(ServerMsg::RoomList(state.lobby.list_rooms().await)),
                    broadcasts,
                }
            } else {
                HandleResult::reply(ServerMsg::Error {
                    code: 400,
                    message: "not in a room".to_string(),
                })
            }
        }
        ClientMsg::GameAction { data } => {
            let player_id = match session.player_id {
                Some(id) => id,
                None => {
                    return HandleResult::reply(ServerMsg::AuthFail {
                        reason: "not authenticated".to_string(),
                    });
                }
            };
            let room_id = match session.current_room {
                Some(id) => id,
                None => {
                    return HandleResult::reply(ServerMsg::Error {
                        code: 400,
                        message: "not in a room".to_string(),
                    });
                }
            };

            let mut games = state.lobby.games.write().await;
            let game = match games.get_mut(&room_id) {
                Some(g) => g,
                None => {
                    return HandleResult::reply(ServerMsg::Error {
                        code: 400,
                        message: "no active game in this room".to_string(),
                    });
                }
            };

            let (response, broadcasts) = game.apply_action(player_id, data);
            HandleResult {
                response: Some(response),
                broadcasts,
            }
        }
        _ => {
            if !session.authenticated {
                HandleResult::reply(ServerMsg::AuthFail {
                    reason: "not authenticated".to_string(),
                })
            } else {
                HandleResult::reply(ServerMsg::Error {
                    code: 501,
                    message: "not yet implemented".to_string(),
                })
            }
        }
    }
}

async fn handle_register(
    username: &str,
    password: &str,
    session: &mut Session,
    state: &ServerState,
) -> Option<ServerMsg> {
    if username.is_empty() || password.len() < 8 {
        return Some(ServerMsg::AuthFail {
            reason: "username required, password must be at least 8 characters".to_string(),
        });
    }

    let pw = password.to_string();
    let password_hash = match tokio::task::spawn_blocking(move || hash_password(&pw)).await {
        Ok(Ok(h)) => h,
        Ok(Err(e)) => {
            error!("password hash failed: {e}");
            return Some(ServerMsg::Error {
                code: 500,
                message: "internal error".to_string(),
            });
        }
        Err(e) => {
            error!("hash task failed: {e}");
            return Some(ServerMsg::Error {
                code: 500,
                message: "internal error".to_string(),
            });
        }
    };

    let user_id = Uuid::new_v4().to_string();

    let db_conn = state.db.conn();
    let uid = user_id.clone();
    let uname = username.to_string();
    let phash = password_hash;
    let created = tokio::task::spawn_blocking(move || {
        let conn = db_conn.lock().unwrap();
        conn.execute(
            "INSERT INTO users (id, username, password_hash) VALUES (?1, ?2, ?3)",
            rusqlite::params![uid, uname, phash],
        )
    })
    .await;

    match created {
        Ok(Ok(_)) => {
            let (token, expires_at) = match state.jwt.create_token(&user_id, username) {
                Ok(t) => t,
                Err(e) => {
                    error!("token creation failed: {e}");
                    return Some(ServerMsg::Error {
                        code: 500,
                        message: "internal error".to_string(),
                    });
                }
            };
            // Authenticate the session
            let pid = Uuid::parse_str(&user_id)
                .map(PlayerId)
                .expect("just-created UUID is valid");
            session.authenticate(pid, username.to_string());
            info!(username, "user registered");
            Some(ServerMsg::AuthOk {
                token,
                expires_at,
                player_id: pid,
            })
        }
        Ok(Err(rusqlite::Error::SqliteFailure(err, _)))
            if err.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            Some(ServerMsg::AuthFail {
                reason: "username already taken".to_string(),
            })
        }
        _ => Some(ServerMsg::Error {
            code: 500,
            message: "failed to create user".to_string(),
        }),
    }
}

async fn handle_login(
    username: &str,
    password: &str,
    session: &mut Session,
    state: &ServerState,
) -> Option<ServerMsg> {
    let db_conn = state.db.conn();
    let uname = username.to_string();
    let user = tokio::task::spawn_blocking(move || {
        let conn = db_conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, password_hash FROM users WHERE username = ?1")
            .ok()?;
        let mut rows = stmt.query(rusqlite::params![uname]).ok()?;
        let row = rows.next().ok()??;
        Some((row.get::<_, String>(0).ok()?, row.get::<_, String>(1).ok()?))
    })
    .await
    .unwrap_or(None);

    let (user_id, stored_hash) = match user {
        Some(u) => u,
        None => {
            return Some(ServerMsg::AuthFail {
                reason: "invalid credentials".to_string(),
            });
        }
    };

    let pw = password.to_string();
    let hash = stored_hash.clone();
    let valid = tokio::task::spawn_blocking(move || verify_password(&pw, &hash))
        .await
        .unwrap_or(false);

    if !valid {
        return Some(ServerMsg::AuthFail {
            reason: "invalid credentials".to_string(),
        });
    }

    let (token, expires_at) = match state.jwt.create_token(&user_id, username) {
        Ok(t) => t,
        Err(e) => {
            error!("token creation failed: {e}");
            return Some(ServerMsg::Error {
                code: 500,
                message: "internal error".to_string(),
            });
        }
    };

    let pid = Uuid::parse_str(&user_id)
        .map(PlayerId)
        .expect("DB user_id is valid UUID");
    session.authenticate(pid, username.to_string());

    info!(username, "user logged in");
    Some(ServerMsg::AuthOk {
        token,
        expires_at,
        player_id: pid,
    })
}

fn handle_authenticate(
    token: &str,
    session: &mut Session,
    state: &ServerState,
) -> Option<ServerMsg> {
    match state.jwt.validate_token(token) {
        Ok(claims) => {
            let pid = Uuid::parse_str(&claims.sub)
                .map(PlayerId)
                .expect("JWT sub is valid UUID");
            session.authenticate(pid, claims.username.clone());
            info!(username = claims.username, "token authenticated");
            Some(ServerMsg::AuthOk {
                token: token.to_string(),
                expires_at: claims.exp,
                player_id: pid,
            })
        }
        Err(reason) => Some(ServerMsg::AuthFail { reason }),
    }
}

fn hash_password(password: &str) -> Result<String, String> {
    use argon2::password_hash::SaltString;
    use argon2::password_hash::rand_core::OsRng;
    use argon2::{Argon2, PasswordHasher};

    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| e.to_string())
}

fn verify_password(password: &str, hash: &str) -> bool {
    use argon2::password_hash::PasswordHash;
    use argon2::{Argon2, PasswordVerifier};

    let parsed_hash = match PasswordHash::new(hash) {
        Ok(h) => h,
        Err(_) => return false,
    };

    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}

async fn send_msg<S>(sender: &mut S, session: &mut Session, msg: &ServerMsg) -> Result<(), String>
where
    S: SinkExt<Message> + Unpin,
    S::Error: std::fmt::Display,
{
    let seq = session.next_seq();
    let envelope = Envelope {
        version: PROTOCOL_VERSION,
        seq,
        payload: msg.clone(),
    };
    let bytes = codec::encode(&envelope).map_err(|e| e.to_string())?;
    sender
        .send(Message::Binary(bytes.into()))
        .await
        .map_err(|e| e.to_string())?;
    session.record_message(seq, msg.clone());
    Ok(())
}
