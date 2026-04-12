use std::time::Instant;

use gc_shared::types::{GameSettings, GameType, PlayerId, PlayerInfo, RoomId, RoomState};

/// A game room on the server.
#[allow(dead_code)]
pub struct Room {
    pub id: RoomId,
    pub game_type: GameType,
    pub settings: GameSettings,
    pub state: RoomState,
    pub players: Vec<PlayerInfo>,
    pub host: PlayerId,
    pub created_at: Instant,
    pub last_activity: Instant,
}

#[allow(dead_code)]
impl Room {
    pub fn new(id: RoomId, game_type: GameType, settings: GameSettings, host: PlayerInfo) -> Self {
        let host_id = host.id;
        Self {
            id,
            game_type,
            settings,
            state: RoomState::Waiting,
            players: vec![host],
            host: host_id,
            created_at: Instant::now(),
            last_activity: Instant::now(),
        }
    }

    pub fn is_full(&self) -> bool {
        self.players.len() >= self.settings.max_players as usize
    }

    pub fn player_count(&self) -> u8 {
        self.players.len() as u8
    }

    pub fn has_player(&self, player_id: PlayerId) -> bool {
        self.players.iter().any(|p| p.id == player_id)
    }

    pub fn add_player(&mut self, player: PlayerInfo) -> Result<(), String> {
        if self.is_full() {
            return Err("room is full".to_string());
        }
        if self.has_player(player.id) {
            return Err("already in room".to_string());
        }
        if self.state != RoomState::Waiting {
            return Err("game already in progress".to_string());
        }
        self.players.push(player);
        self.last_activity = Instant::now();
        Ok(())
    }

    pub fn remove_player(&mut self, player_id: PlayerId) -> bool {
        let before = self.players.len();
        self.players.retain(|p| p.id != player_id);
        self.last_activity = Instant::now();
        self.players.len() < before
    }

    pub fn is_empty(&self) -> bool {
        self.players.is_empty()
    }

    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
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

    #[test]
    fn create_room() {
        let host = make_player("alice");
        let room = Room::new(
            RoomId::new(),
            GameType::TicTacToe,
            GameSettings::default(),
            host.clone(),
        );
        assert_eq!(room.player_count(), 1);
        assert!(room.has_player(host.id));
        assert!(!room.is_full());
    }

    #[test]
    fn add_and_remove_player() {
        let host = make_player("alice");
        let guest = make_player("bob");
        let mut room = Room::new(
            RoomId::new(),
            GameType::TicTacToe,
            GameSettings::default(),
            host.clone(),
        );

        room.add_player(guest.clone()).unwrap();
        assert_eq!(room.player_count(), 2);
        assert!(room.is_full()); // default max_players = 2

        room.remove_player(guest.id);
        assert_eq!(room.player_count(), 1);
        assert!(!room.has_player(guest.id));
    }

    #[test]
    fn cannot_join_full_room() {
        let host = make_player("alice");
        let guest = make_player("bob");
        let extra = make_player("charlie");
        let mut room = Room::new(
            RoomId::new(),
            GameType::TicTacToe,
            GameSettings::default(),
            host,
        );

        room.add_player(guest).unwrap();
        let result = room.add_player(extra);
        assert!(result.is_err());
    }

    #[test]
    fn cannot_join_twice() {
        let host = make_player("alice");
        let host_clone = PlayerInfo {
            id: host.id,
            username: "alice".to_string(),
        };
        let mut room = Room::new(
            RoomId::new(),
            GameType::TicTacToe,
            GameSettings {
                max_players: 4,
                ..GameSettings::default()
            },
            host,
        );

        let result = room.add_player(host_clone);
        assert!(result.is_err());
    }

    #[test]
    fn remove_all_makes_empty() {
        let host = make_player("alice");
        let host_id = host.id;
        let mut room = Room::new(
            RoomId::new(),
            GameType::Chess,
            GameSettings::default(),
            host,
        );

        room.remove_player(host_id);
        assert!(room.is_empty());
    }
}
