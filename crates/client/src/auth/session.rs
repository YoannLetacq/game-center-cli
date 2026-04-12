/// Holds the current authentication state for the client.
#[derive(Debug, Default)]
#[allow(dead_code)] // Used in later phases (TUI integration)
pub struct AuthSession {
    pub token: Option<String>,
    pub expires_at: Option<i64>,
    pub username: Option<String>,
}

impl AuthSession {
    pub fn new() -> Self {
        Self::default()
    }

    /// Store a new auth token.
    pub fn set_token(&mut self, token: String, expires_at: i64, username: String) {
        self.token = Some(token);
        self.expires_at = Some(expires_at);
        self.username = Some(username);
    }

    /// Check if the session has a valid (non-expired) token.
    pub fn is_authenticated(&self) -> bool {
        match (self.token.as_ref(), self.expires_at) {
            (Some(_), Some(exp)) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;
                exp > now
            }
            _ => false,
        }
    }

    /// Clear the session.
    pub fn clear(&mut self) {
        self.token = None;
        self.expires_at = None;
        self.username = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_session_is_unauthenticated() {
        let session = AuthSession::new();
        assert!(!session.is_authenticated());
    }

    #[test]
    fn session_with_future_token_is_authenticated() {
        let mut session = AuthSession::new();
        let future_exp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            + 3600;
        session.set_token("tok".to_string(), future_exp, "alice".to_string());
        assert!(session.is_authenticated());
    }

    #[test]
    fn session_with_past_token_is_not_authenticated() {
        let mut session = AuthSession::new();
        session.set_token("tok".to_string(), 1000, "bob".to_string());
        assert!(!session.is_authenticated());
    }

    #[test]
    fn clear_removes_auth() {
        let mut session = AuthSession::new();
        let future_exp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            + 3600;
        session.set_token("tok".to_string(), future_exp, "alice".to_string());
        session.clear();
        assert!(!session.is_authenticated());
    }
}
