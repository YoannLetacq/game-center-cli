use std::collections::VecDeque;
use std::time::Instant;

use gc_shared::protocol::messages::ServerMsg;
use gc_shared::types::{PlayerId, SessionId};

/// Per-connection session state, tracking a player's connection lifecycle.
#[allow(dead_code)] // Fields/methods used in reconnection (Phase 3)
pub struct Session {
    pub session_id: SessionId,
    pub player_id: Option<PlayerId>,
    pub username: Option<String>,
    pub authenticated: bool,
    pub seq: u64,
    /// Ring buffer of recent server messages for reconnection replay.
    recent_messages: VecDeque<(u64, ServerMsg)>,
    /// When the session was last active.
    pub last_active: Instant,
    /// Max messages to buffer for replay.
    max_buffer: usize,
}

#[allow(dead_code)]
impl Session {
    pub fn new() -> Self {
        Self {
            session_id: SessionId::new(),
            player_id: None,
            username: None,
            authenticated: false,
            seq: 0,
            recent_messages: VecDeque::with_capacity(100),
            last_active: Instant::now(),
            max_buffer: 100,
        }
    }

    /// Record a sent message for potential reconnection replay.
    /// Uses the provided seq (from the envelope that was already sent).
    pub fn record_message(&mut self, seq: u64, msg: ServerMsg) {
        if self.recent_messages.len() >= self.max_buffer {
            self.recent_messages.pop_front();
        }
        self.recent_messages.push_back((seq, msg));
    }

    /// Get all messages after the given sequence number (for reconnection).
    pub fn messages_since(&self, last_seq: u64) -> Vec<ServerMsg> {
        self.recent_messages
            .iter()
            .filter(|(seq, _)| *seq > last_seq)
            .map(|(_, msg)| msg.clone())
            .collect()
    }

    /// Allocate the next sequence number for an outgoing message.
    pub fn next_seq(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }

    /// Mark session as active.
    pub fn touch(&mut self) {
        self.last_active = Instant::now();
    }

    /// Mark session as authenticated with a player identity.
    pub fn authenticate(&mut self, player_id: PlayerId, username: String) {
        self.player_id = Some(player_id);
        self.username = Some(username);
        self.authenticated = true;
        self.touch();
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_message_buffer() {
        let mut session = Session::new();
        for i in 0..5u64 {
            let seq = session.next_seq();
            session.record_message(seq, ServerMsg::Pong);
            assert_eq!(session.seq, i + 1);
        }

        let missed = session.messages_since(2);
        assert_eq!(missed.len(), 3); // messages 3, 4, 5
    }

    #[test]
    fn session_buffer_overflow() {
        let mut session = Session::new();
        session.max_buffer = 3;

        for _ in 0..5 {
            let seq = session.next_seq();
            session.record_message(seq, ServerMsg::Pong);
        }

        // Buffer holds last 3: seq 3, 4, 5
        assert_eq!(session.recent_messages.len(), 3);
        let missed = session.messages_since(0);
        assert_eq!(missed.len(), 3);
    }

    #[test]
    fn session_authentication() {
        let mut session = Session::new();
        assert!(!session.authenticated);

        let pid = PlayerId::new();
        session.authenticate(pid, "alice".to_string());

        assert!(session.authenticated);
        assert_eq!(session.player_id, Some(pid));
        assert_eq!(session.username.as_deref(), Some("alice"));
    }
}
