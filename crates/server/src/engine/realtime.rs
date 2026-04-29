use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{RwLock, mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;
use tracing::{info, warn};

use gc_shared::game::snake::{SnakeEngine, SnakeInput, SnakeState};
use gc_shared::game::traits::RealtimeGameEngine;
use gc_shared::protocol::codec;
use gc_shared::protocol::messages::ServerMsg;
use gc_shared::types::{GameOutcome, GameSettings, GameType, PlayerId, RoomId};

use crate::lobby::manager::LobbyManager;

/// Realtime tick period (100 ms = 10 Hz).
const TICK_MS: u64 = 100;
/// Periodic full-state resync every N ticks (~5 seconds at 10 Hz).
const RESYNC_EVERY: u64 = 50;

pub enum RealtimeGame {
    Snake(SnakeState),
}

impl RealtimeGame {
    pub fn new(game_type: GameType, players: &[PlayerId], settings: &GameSettings) -> Option<Self> {
        match game_type {
            GameType::Snake => Some(Self::Snake(SnakeEngine::initial_multiplayer_state(players, settings))),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn game_type(&self) -> GameType {
        match self {
            Self::Snake(_) => GameType::Snake,
        }
    }

    /// Advance one tick. Decodes per-player input bytes, dispatches to engine,
    /// encodes the resulting delta, and reports terminal outcome if any.
    pub fn tick(
        &mut self,
        inputs_bytes: &HashMap<PlayerId, Vec<u8>>,
    ) -> Result<(u64, Vec<u8>, Option<GameOutcome>), String> {
        match self {
            Self::Snake(state) => {
                let mut decoded: HashMap<PlayerId, SnakeInput> =
                    HashMap::with_capacity(inputs_bytes.len());
                for (pid, bytes) in inputs_bytes {
                    // Skip malformed inputs — a single bad client should not poison the tick.
                    match codec::decode::<SnakeInput>(bytes) {
                        Ok(input) => {
                            decoded.insert(*pid, input);
                        }
                        Err(e) => {
                            warn!(%pid, "dropping malformed snake input: {e}");
                        }
                    }
                }
                let delta = SnakeEngine::tick(state, &decoded);
                let tick = delta.tick;
                let bytes = codec::encode(&delta).map_err(|e| e.to_string())?;
                let outcome = SnakeEngine::is_terminal(state);
                Ok((tick, bytes, outcome))
            }
        }
    }

    pub fn snapshot_bytes(&self) -> Vec<u8> {
        match self {
            Self::Snake(state) => match codec::encode(state) {
                Ok(v) => v,
                Err(e) => {
                    warn!("failed to encode Snake state snapshot: {}", e);
                    Vec::new()
                }
            },
        }
    }

    #[allow(dead_code)]
    pub fn current_tick(&self) -> u64 {
        match self {
            Self::Snake(state) => state.tick,
        }
    }

    pub fn players(&self) -> Vec<PlayerId> {
        match self {
            Self::Snake(state) => state
                .arenas
                .iter()
                .filter_map(|a| a.owner)
                .collect(),
        }
    }

    pub fn is_supported(game_type: GameType) -> bool {
        matches!(game_type, GameType::Snake)
    }
}

/// Spawn the tick loop for a realtime room. The returned `JoinHandle` exits
/// either when the watch channel flips to `true` (cancelled) or the game ends.
pub fn spawn_tick_task(
    room_id: RoomId,
    game: Arc<Mutex<RealtimeGame>>,
    inputs: Arc<Mutex<HashMap<PlayerId, Vec<u8>>>>,
    players_registry: Arc<RwLock<HashMap<PlayerId, mpsc::Sender<ServerMsg>>>>,
    mut cancel: watch::Receiver<bool>,
    lobby: Arc<LobbyManager>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(TICK_MS));
        // Burst: catch up missed ticks without long sleeps if the scheduler lagged.
        interval.set_missed_tick_behavior(MissedTickBehavior::Burst);

        loop {
            tokio::select! {
                biased;
                changed = cancel.changed() => {
                    if changed.is_err() || *cancel.borrow() {
                        info!(%room_id, "realtime tick task cancelled");
                        return;
                    }
                }
                _ = interval.tick() => {}
            }

            // Drain inputs without holding the lock across the game tick.
            let drained = {
                let mut guard = inputs.lock().unwrap_or_else(|e| e.into_inner());
                std::mem::take(&mut *guard)
            };

            // Tick the game; release lock promptly.
            let (tick, delta_bytes, outcome, recipients, snapshot) = {
                let mut guard = game.lock().unwrap_or_else(|e| e.into_inner());
                let tick_result = guard.tick(&drained);
                match tick_result {
                    Ok((tick, bytes, outcome)) => {
                        let recipients = guard.players();
                        let snapshot = if tick % RESYNC_EVERY == 0 {
                            Some(guard.snapshot_bytes())
                        } else {
                            None
                        };
                        (tick, bytes, outcome, recipients, snapshot)
                    }
                    Err(e) => {
                        warn!(%room_id, "tick failed: {e}");
                        continue;
                    }
                }
            };

            // Broadcast delta via bounded channels — never await send to avoid
            // one stuck client blocking every other player's tick.
            {
                let registry = players_registry.read().await;
                let delta_msg = ServerMsg::GameDelta {
                    tick,
                    delta_data: delta_bytes,
                };
                for pid in &recipients {
                    if let Some(tx) = registry.get(pid)
                        && let Err(e) = tx.try_send(delta_msg.clone())
                    {
                        warn!(%pid, "dropping delta for slow/full client: {e}");
                    }
                }
                if let Some(snap) = snapshot {
                    let snap_msg = ServerMsg::GameStateUpdate {
                        tick,
                        state_data: snap,
                    };
                    for pid in &recipients {
                        if let Some(tx) = registry.get(pid)
                            && let Err(e) = tx.try_send(snap_msg.clone())
                        {
                            warn!(%pid, "dropping resync for slow/full client: {e}");
                        }
                    }
                }
                if let Some(outcome) = outcome.clone() {
                    let over_msg = ServerMsg::GameOver { outcome };
                    for pid in &recipients {
                        if let Some(tx) = registry.get(pid) {
                            let _ = tx.try_send(over_msg.clone());
                        }
                    }
                }
            }

            if outcome.is_some() {
                info!(%room_id, "realtime game finished");
                lobby.finish_realtime_game(room_id).await;
                return;
            }
        }
    })
}
