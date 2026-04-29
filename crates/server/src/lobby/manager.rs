use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tokio::sync::{RwLock, mpsc, watch};
use tokio::task::JoinHandle;
use tracing::info;

use gc_shared::protocol::messages::{RoomSummary, ServerMsg};
use gc_shared::types::{GameOutcome, GameSettings, GameType, PlayerId, PlayerInfo, RoomId};

use crate::engine::realtime::{self, RealtimeGame};
use crate::engine::turn_based::TurnBasedGame;

use super::room::Room;

/// Room cleanup timeout: rooms with no players are removed after this duration.
const EMPTY_ROOM_TIMEOUT: Duration = Duration::from_secs(60);

/// Handle to the per-connection broadcast channels. Set once from main after
/// the `ServerState` is wired up so realtime tick tasks can reach clients.
pub type PlayerRegistry = Arc<RwLock<HashMap<PlayerId, mpsc::Sender<ServerMsg>>>>;

/// Manages all active game rooms.
///
/// A game type is creatable if either the turn-based or realtime engine
/// supports it (see [`is_supported`]).
#[allow(clippy::type_complexity)]
pub struct LobbyManager {
    rooms: Arc<RwLock<HashMap<RoomId, Room>>>,
    /// Tracks which room each player is in.
    player_rooms: Arc<RwLock<HashMap<PlayerId, RoomId>>>,
    /// Active turn-based games indexed by room ID.
    pub games: Arc<RwLock<HashMap<RoomId, TurnBasedGame>>>,

    /// Active realtime games indexed by room ID.
    pub realtime_games: Arc<RwLock<HashMap<RoomId, Arc<Mutex<RealtimeGame>>>>>,
    /// Latest buffered input per player per realtime room (overwrites, no queue).
    pub realtime_inputs: Arc<RwLock<HashMap<RoomId, Arc<Mutex<HashMap<PlayerId, Vec<u8>>>>>>>,
    /// Cancellation senders for each realtime tick task.
    pub tick_cancels: Arc<RwLock<HashMap<RoomId, watch::Sender<bool>>>>,
    /// Detached tick task handles — kept so we can ensure graceful shutdown on cancel.
    tick_tasks: Arc<RwLock<HashMap<RoomId, JoinHandle<()>>>>,

    /// Set once by main.rs after constructing `ServerState`. Needed for realtime broadcasts.
    players_registry: OnceLock<PlayerRegistry>,
    /// Self-reference injected after `Arc::new` so tick tasks can call back.
    self_arc: OnceLock<std::sync::Weak<LobbyManager>>,
}

/// Is the given game type playable on this server (turn-based OR realtime).
pub fn is_supported(game_type: GameType) -> bool {
    TurnBasedGame::is_supported(game_type) || RealtimeGame::is_supported(game_type)
}

#[allow(dead_code)]
impl LobbyManager {
    pub fn new() -> Self {
        Self {
            rooms: Arc::new(RwLock::new(HashMap::new())),
            player_rooms: Arc::new(RwLock::new(HashMap::new())),
            games: Arc::new(RwLock::new(HashMap::new())),
            realtime_games: Arc::new(RwLock::new(HashMap::new())),
            realtime_inputs: Arc::new(RwLock::new(HashMap::new())),
            tick_cancels: Arc::new(RwLock::new(HashMap::new())),
            tick_tasks: Arc::new(RwLock::new(HashMap::new())),
            players_registry: OnceLock::new(),
            self_arc: OnceLock::new(),
        }
    }

    /// Wire up the shared player registry and a weak self-reference. Must be called
    /// once after the lobby is placed behind an `Arc`, before realtime games can start.
    pub fn attach(self: &Arc<Self>, registry: PlayerRegistry) {
        let _ = self.players_registry.set(registry);
        let _ = self.self_arc.set(Arc::downgrade(self));
    }

    fn strong_self(&self) -> Option<Arc<LobbyManager>> {
        self.self_arc.get().and_then(|w| w.upgrade())
    }

    /// Create a new room. Returns the room ID.
    pub async fn create_room(
        &self,
        game_type: GameType,
        settings: GameSettings,
        host: PlayerInfo,
    ) -> Result<RoomId, String> {
        if !is_supported(game_type) {
            return Err(format!("{game_type} is not yet available"));
        }

        // Validate max_players
        if settings.max_players != 2 {
            return Err("This game requires exactly 2 players".to_string());
        }

        let host_id = host.id;

        // Check if player is already in a room
        {
            let pr = self.player_rooms.read().await;
            if pr.contains_key(&host_id) {
                return Err("already in a room".to_string());
            }
        }

        let room_id = RoomId::new();
        let room = Room::new(room_id, game_type, settings, host);

        {
            let mut rooms = self.rooms.write().await;
            rooms.insert(room_id, room);
        }
        {
            let mut pr = self.player_rooms.write().await;
            pr.insert(host_id, room_id);
        }

        info!(%room_id, %host_id, ?game_type, "room created");
        Ok(room_id)
    }

    /// Join an existing room.
    pub async fn join_room(
        &self,
        room_id: RoomId,
        player: PlayerInfo,
    ) -> Result<Vec<PlayerInfo>, String> {
        let player_id = player.id;

        // Check if player is already in a room
        {
            let pr = self.player_rooms.read().await;
            if pr.contains_key(&player_id) {
                return Err("already in a room".to_string());
            }
        }

        let players = {
            let mut rooms = self.rooms.write().await;
            let room = rooms.get_mut(&room_id).ok_or("room not found")?;
            room.add_player(player)?;
            room.players.clone()
        };

        {
            let mut pr = self.player_rooms.write().await;
            pr.insert(player_id, room_id);
        }

        // Auto-start game if room is full
        let is_full = {
            let rooms = self.rooms.read().await;
            rooms.get(&room_id).is_some_and(|r| r.is_full())
        };
        if is_full {
            self.start_game(room_id).await;
        }

        info!(%room_id, %player_id, "player joined room");
        Ok(players)
    }

    /// Start a game in a room. Dispatches to turn-based or realtime engine.
    pub async fn start_game(&self, room_id: RoomId) {
        let room_info = {
            let mut rooms = self.rooms.write().await;
            rooms.get_mut(&room_id).map(|room| {
                // First game: pick starting player randomly.
                // Each rematch: alternate who goes first.
                if room.games_played == 0 {
                    let random_byte = uuid::Uuid::new_v4().as_bytes()[15];
                    room.first_player_offset = random_byte & 1;
                } else {
                    room.first_player_offset = 1 - room.first_player_offset;
                }
                room.games_played += 1;
                room.state = gc_shared::types::RoomState::InProgress;

                let mut player_ids: Vec<PlayerId> = room.players.iter().map(|p| p.id).collect();
                if room.first_player_offset == 1 && player_ids.len() == 2 {
                    player_ids.swap(0, 1);
                }
                (room.game_type, room.settings.clone(), player_ids)
            })
        };

        let Some((game_type, mut settings, player_ids)) = room_info else {
            return;
        };

        if RealtimeGame::is_supported(game_type) {
            // Fresh seed every start — rematches re-randomize food layout.
            if settings.seed.is_none() {
                settings.seed = Some(rand::random::<u64>());
            }
            if let Some(game) = RealtimeGame::new(game_type, &player_ids, &settings) {
                self.start_realtime_game(room_id, game).await;
                info!(%room_id, ?game_type, "realtime game started");
            }
        } else if let Some(game) = TurnBasedGame::new(game_type, &player_ids, &settings) {
            let mut games = self.games.write().await;
            games.insert(room_id, game);
            info!(%room_id, ?game_type, "game started");
        }
    }

    async fn start_realtime_game(&self, room_id: RoomId, game: RealtimeGame) {
        let Some(registry) = self.players_registry.get().cloned() else {
            tracing::error!(%room_id, "player registry not attached; cannot start realtime game");
            return;
        };
        let Some(lobby_arc) = self.strong_self() else {
            tracing::error!(%room_id, "lobby self-ref missing; cannot start realtime game");
            return;
        };

        // Broadcast initial snapshot so clients have a synchronized starting state
        // before the first delta arrives.
        let initial_snapshot = game.snapshot_bytes();
        let recipients = game.players();
        {
            let reg = registry.read().await;
            let msg = ServerMsg::GameStateUpdate {
                tick: 0,
                state_data: initial_snapshot,
            };
            for pid in &recipients {
                if let Some(tx) = reg.get(pid) {
                    let _ = tx.try_send(msg.clone());
                }
            }
        }

        let game_arc = Arc::new(Mutex::new(game));
        let inputs_arc: Arc<Mutex<HashMap<PlayerId, Vec<u8>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (cancel_tx, cancel_rx) = watch::channel(false);

        {
            let mut rg = self.realtime_games.write().await;
            rg.insert(room_id, Arc::clone(&game_arc));
        }
        {
            let mut ri = self.realtime_inputs.write().await;
            ri.insert(room_id, Arc::clone(&inputs_arc));
        }
        {
            let mut tc = self.tick_cancels.write().await;
            tc.insert(room_id, cancel_tx);
        }

        let handle = realtime::spawn_tick_task(
            room_id, game_arc, inputs_arc, registry, cancel_rx, lobby_arc,
        );

        let mut tasks = self.tick_tasks.write().await;
        tasks.insert(room_id, handle);
    }

    /// Store the latest input bytes for a player in a realtime room.
    /// Overwrites any previous pending input — no queueing.
    pub async fn push_realtime_input(&self, room_id: RoomId, player_id: PlayerId, data: Vec<u8>) {
        let inputs = {
            let ri = self.realtime_inputs.read().await;
            ri.get(&room_id).cloned()
        };
        if let Some(inputs) = inputs {
            let mut guard = inputs.lock().unwrap_or_else(|e| e.into_inner());
            guard.insert(player_id, data);
        }
    }

    /// Is this room running a realtime game right now?
    pub async fn is_realtime_room(&self, room_id: RoomId) -> bool {
        let rg = self.realtime_games.read().await;
        rg.contains_key(&room_id)
    }

    /// Clean up a realtime game's state after normal termination (game over / rematch).
    pub async fn finish_realtime_game(&self, room_id: RoomId) {
        let cancel = {
            let mut tc = self.tick_cancels.write().await;
            tc.remove(&room_id)
        };
        if let Some(tx) = cancel {
            let _ = tx.send(true);
        }
        {
            let mut rg = self.realtime_games.write().await;
            rg.remove(&room_id);
        }
        {
            let mut ri = self.realtime_inputs.write().await;
            ri.remove(&room_id);
        }
        {
            let mut tasks = self.tick_tasks.write().await;
            tasks.remove(&room_id);
            // Handle dropped; task will exit on its next select due to cancel signal.
        }
        {
            let mut rooms = self.rooms.write().await;
            if let Some(room) = rooms.get_mut(&room_id) {
                room.state = gc_shared::types::RoomState::Finished;
            }
        }
    }

    /// Cancel a realtime game because a player left mid-game. Broadcasts GameOver
    /// awarding the win to any remaining player, then cleans up.
    pub async fn cancel_realtime_for_disconnect(&self, room_id: RoomId, leaver: PlayerId) {
        let had_realtime = {
            let rg = self.realtime_games.read().await;
            rg.contains_key(&room_id)
        };
        if !had_realtime {
            return;
        }

        // Determine winner from the remaining room roster (not from game state —
        // the leaver may have already been marked dead by the engine).
        let winner = {
            let rooms = self.rooms.read().await;
            rooms
                .get(&room_id)
                .and_then(|r| r.players.iter().find(|p| p.id != leaver).map(|p| p.id))
        };

        if let (Some(winner), Some(registry)) = (winner, self.players_registry.get().cloned()) {
            let recipients: Vec<PlayerId> = {
                let rooms = self.rooms.read().await;
                rooms
                    .get(&room_id)
                    .map(|r| r.players.iter().map(|p| p.id).collect())
                    .unwrap_or_default()
            };
            let msg = ServerMsg::GameOver {
                outcome: GameOutcome::Win(winner),
            };
            let reg = registry.read().await;
            for pid in &recipients {
                if let Some(tx) = reg.get(pid) {
                    let _ = tx.try_send(msg.clone());
                }
            }
        }

        self.finish_realtime_game(room_id).await;
    }

    /// Mark a turn-based game as finished and clean up its state.
    pub async fn finish_game(&self, room_id: RoomId) {
        {
            let mut games = self.games.write().await;
            games.remove(&room_id);
        }
        {
            let mut rooms = self.rooms.write().await;
            if let Some(room) = rooms.get_mut(&room_id) {
                room.state = gc_shared::types::RoomState::Finished;
            }
        }
        info!(%room_id, "game finished and state cleaned up");
    }

    /// Leave the current room.
    ///
    /// Returns `(room_id, is_empty, game_aborted, was_realtime)`.
    /// `was_realtime` is `true` when the aborted game was a realtime room — in that
    /// case `cancel_realtime_for_disconnect` has already broadcast `GameOver`, so
    /// callers must NOT emit a second `GameOver`.
    pub async fn leave_room(&self, player_id: PlayerId) -> Option<(RoomId, bool, bool, bool)> {
        let room_id = {
            let mut pr = self.player_rooms.write().await;
            pr.remove(&player_id)?
        };

        let is_empty = {
            let mut rooms = self.rooms.write().await;
            if let Some(room) = rooms.get_mut(&room_id) {
                room.remove_player(player_id);
                let empty = room.is_empty();
                if empty {
                    rooms.remove(&room_id);
                }
                empty
            } else {
                return None;
            }
        };

        let mut game_aborted = false;
        // Clean up turn-based game if room empties or player leaves mid-game
        {
            let mut games = self.games.write().await;
            if is_empty {
                games.remove(&room_id);
            } else if games.contains_key(&room_id) {
                games.remove(&room_id);
                game_aborted = true;
            }
        }

        // Realtime: tear down tick task + maps. cancel_realtime_for_disconnect
        // handles the winner broadcast; for empty rooms just clean up silently.
        let had_realtime = {
            let rg = self.realtime_games.read().await;
            rg.contains_key(&room_id)
        };
        if had_realtime {
            if is_empty {
                self.finish_realtime_game(room_id).await;
            } else {
                self.cancel_realtime_for_disconnect(room_id, player_id)
                    .await;
                game_aborted = true;
            }
        }

        if game_aborted {
            let mut rooms = self.rooms.write().await;
            if let Some(room) = rooms.get_mut(&room_id) {
                room.state = gc_shared::types::RoomState::Waiting;
            }
        }

        info!(%room_id, %player_id, is_empty, game_aborted, had_realtime, "player left room");
        Some((room_id, is_empty, game_aborted, had_realtime))
    }

    /// Get the list of rooms as summaries for the lobby screen.
    pub async fn list_rooms(&self) -> Vec<RoomSummary> {
        let rooms = self.rooms.read().await;
        rooms
            .values()
            .map(|room| RoomSummary {
                id: room.id,
                game_type: room.game_type,
                player_count: room.player_count(),
                max_players: room.settings.max_players,
                state: room.state.clone(),
                host_name: room
                    .players
                    .first()
                    .map(|p| p.username.clone())
                    .unwrap_or_default(),
            })
            .collect()
    }

    /// Get players in a specific room.
    pub async fn get_room_players(&self, room_id: RoomId) -> Option<Vec<PlayerInfo>> {
        let rooms = self.rooms.read().await;
        rooms.get(&room_id).map(|r| r.players.clone())
    }

    /// Get the room a player is currently in.
    pub async fn get_player_room(&self, player_id: PlayerId) -> Option<RoomId> {
        let pr = self.player_rooms.read().await;
        pr.get(&player_id).copied()
    }

    /// Clean up empty rooms that have been idle for too long.
    pub async fn cleanup_empty_rooms(&self) -> usize {
        let now = Instant::now();
        let mut rooms = self.rooms.write().await;
        let before = rooms.len();

        rooms.retain(|_, room| {
            !(room.is_empty() && now.duration_since(room.last_activity) > EMPTY_ROOM_TIMEOUT)
        });

        let removed = before - rooms.len();
        if removed > 0 {
            info!(removed, "cleaned up empty rooms");
        }
        removed
    }

    /// Start a periodic cleanup task.
    pub fn spawn_cleanup_task(self: &Arc<Self>) {
        let lobby = Arc::clone(self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            loop {
                interval.tick().await;
                lobby.cleanup_empty_rooms().await;
            }
        });
    }
}

impl Default for LobbyManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_player(name: &str) -> PlayerInfo {
        PlayerInfo {
            id: PlayerId::new(),
            username: name.to_string(),
        }
    }

    #[tokio::test]
    async fn create_and_list_rooms() {
        let lobby = LobbyManager::new();
        let host = make_player("alice");

        let room_id = lobby
            .create_room(GameType::TicTacToe, GameSettings::default(), host)
            .await
            .unwrap();

        let rooms = lobby.list_rooms().await;
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].id, room_id);
        assert_eq!(rooms[0].game_type, GameType::TicTacToe);
        assert_eq!(rooms[0].player_count, 1);
        assert_eq!(rooms[0].host_name, "alice");
    }

    #[tokio::test]
    async fn join_room() {
        let lobby = LobbyManager::new();
        let host = make_player("alice");
        let guest = make_player("bob");

        let room_id = lobby
            .create_room(GameType::TicTacToe, GameSettings::default(), host)
            .await
            .unwrap();

        let players = lobby.join_room(room_id, guest).await.unwrap();
        assert_eq!(players.len(), 2);

        let rooms = lobby.list_rooms().await;
        assert_eq!(rooms[0].player_count, 2);
    }

    #[tokio::test]
    async fn cannot_join_nonexistent_room() {
        let lobby = LobbyManager::new();
        let player = make_player("alice");
        let result = lobby.join_room(RoomId::new(), player).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn cannot_create_two_rooms() {
        let lobby = LobbyManager::new();
        let host = make_player("alice");
        let host_id = host.id;

        lobby
            .create_room(GameType::TicTacToe, GameSettings::default(), host)
            .await
            .unwrap();

        let host2 = PlayerInfo {
            id: host_id,
            username: "alice".to_string(),
        };
        let result = lobby
            .create_room(GameType::Chess, GameSettings::default(), host2)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn leave_room() {
        let lobby = LobbyManager::new();
        let host = make_player("alice");
        let host_id = host.id;
        let guest = make_player("bob");

        let room_id = lobby
            .create_room(GameType::TicTacToe, GameSettings::default(), host)
            .await
            .unwrap();
        lobby.join_room(room_id, guest).await.unwrap();

        let result = lobby.leave_room(host_id).await;
        assert_eq!(result, Some((room_id, false, true, false))); // not empty, bob still there, turn-based game aborted

        let rooms = lobby.list_rooms().await;
        assert_eq!(rooms[0].player_count, 1);
    }

    #[tokio::test]
    async fn leave_last_player_marks_empty() {
        let lobby = LobbyManager::new();
        let host = make_player("alice");
        let host_id = host.id;

        let room_id = lobby
            .create_room(GameType::TicTacToe, GameSettings::default(), host)
            .await
            .unwrap();

        let result = lobby.leave_room(host_id).await;
        assert_eq!(result, Some((room_id, true, false, false))); // empty now, no game was running
    }

    #[tokio::test]
    async fn player_room_tracking() {
        let lobby = LobbyManager::new();
        let host = make_player("alice");
        let host_id = host.id;

        assert!(lobby.get_player_room(host_id).await.is_none());

        let room_id = lobby
            .create_room(GameType::TicTacToe, GameSettings::default(), host)
            .await
            .unwrap();

        assert_eq!(lobby.get_player_room(host_id).await, Some(room_id));

        lobby.leave_room(host_id).await;
        assert!(lobby.get_player_room(host_id).await.is_none());
    }

    #[tokio::test]
    async fn unsupported_game_type_rejected() {
        let lobby = LobbyManager::new();
        let host = make_player("alice");

        let result = lobby
            .create_room(GameType::BlockBreaker, GameSettings::default(), host)
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not yet available"));
    }

    #[tokio::test]
    async fn snake_room_accepted() {
        let lobby = LobbyManager::new();
        let host = make_player("alice");

        let result = lobby
            .create_room(GameType::Snake, GameSettings::default(), host)
            .await;
        assert!(result.is_ok(), "Snake should be supported (realtime)");
    }
}
