use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub struct ClientDatabase {
    conn: Arc<Mutex<Connection>>,
}

#[allow(dead_code)] // Methods used in TUI integration and later phases
impl ClientDatabase {
    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.run_migrations()?;
        Ok(db)
    }

    pub fn open_in_memory() -> Result<Self, rusqlite::Error> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.run_migrations()?;
        Ok(db)
    }

    fn run_migrations(&self) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER PRIMARY KEY
            );

            CREATE TABLE IF NOT EXISTS profile (
                id INTEGER PRIMARY KEY,
                username TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS auth_tokens (
                id INTEGER PRIMARY KEY,
                token TEXT NOT NULL,
                expires_at INTEGER NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS match_history (
                id INTEGER PRIMARY KEY,
                game_type TEXT NOT NULL,
                opponent TEXT,
                result TEXT NOT NULL,
                played_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )?;
        Ok(())
    }

    /// Save or update the local profile.
    pub fn save_profile(&self, username: &str) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM profile", [])?;
        conn.execute(
            "INSERT INTO profile (username) VALUES (?1)",
            rusqlite::params![username],
        )?;
        Ok(())
    }

    /// Get the stored profile username.
    pub fn get_profile(&self) -> Result<Option<String>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT username FROM profile LIMIT 1")?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    /// Store an auth token.
    pub fn save_token(&self, token: &str, expires_at: i64) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM auth_tokens", [])?;
        conn.execute(
            "INSERT INTO auth_tokens (token, expires_at) VALUES (?1, ?2)",
            rusqlite::params![token, expires_at],
        )?;
        Ok(())
    }

    /// Get the stored auth token if it exists and hasn't expired.
    pub fn get_valid_token(&self) -> Result<Option<(String, i64)>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let mut stmt = conn
            .prepare("SELECT token, expires_at FROM auth_tokens WHERE expires_at > ?1 LIMIT 1")?;
        let mut rows = stmt.query(rusqlite::params![now])?;
        if let Some(row) = rows.next()? {
            Ok(Some((row.get(0)?, row.get(1)?)))
        } else {
            Ok(None)
        }
    }

    /// Clear stored auth tokens.
    pub fn clear_tokens(&self) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM auth_tokens", [])?;
        Ok(())
    }

    /// Record a match result.
    pub fn record_match(
        &self,
        game_type: &str,
        opponent: Option<&str>,
        result: &str,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO match_history (game_type, opponent, result) VALUES (?1, ?2, ?3)",
            rusqlite::params![game_type, opponent, result],
        )?;
        Ok(())
    }

    /// Get match history (most recent first).
    pub fn get_match_history(&self, limit: usize) -> Result<Vec<MatchRecord>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT game_type, opponent, result, played_at FROM match_history ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(rusqlite::params![limit as i64], |row| {
            Ok(MatchRecord {
                game_type: row.get(0)?,
                opponent: row.get(1)?,
                result: row.get(2)?,
                played_at: row.get(3)?,
            })
        })?;
        rows.collect()
    }

    /// Get a clone of the connection Arc for async wrapping.
    pub fn conn(&self) -> Arc<Mutex<Connection>> {
        self.conn.clone()
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MatchRecord {
    pub game_type: String,
    pub opponent: Option<String>,
    pub result: String,
    pub played_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_and_get_profile() {
        let db = ClientDatabase::open_in_memory().unwrap();
        assert!(db.get_profile().unwrap().is_none());

        db.save_profile("alice").unwrap();
        assert_eq!(db.get_profile().unwrap().as_deref(), Some("alice"));

        // Updating profile replaces the old one
        db.save_profile("bob").unwrap();
        assert_eq!(db.get_profile().unwrap().as_deref(), Some("bob"));
    }

    #[test]
    fn save_and_get_token() {
        let db = ClientDatabase::open_in_memory().unwrap();
        let future = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            + 3600;

        db.save_token("my-jwt", future).unwrap();
        let result = db.get_valid_token().unwrap();
        assert!(result.is_some());
        let (token, exp) = result.unwrap();
        assert_eq!(token, "my-jwt");
        assert_eq!(exp, future);
    }

    #[test]
    fn expired_token_not_returned() {
        let db = ClientDatabase::open_in_memory().unwrap();
        db.save_token("old-jwt", 1000).unwrap();
        assert!(db.get_valid_token().unwrap().is_none());
    }

    #[test]
    fn clear_tokens() {
        let db = ClientDatabase::open_in_memory().unwrap();
        let future = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            + 3600;

        db.save_token("jwt", future).unwrap();
        db.clear_tokens().unwrap();
        assert!(db.get_valid_token().unwrap().is_none());
    }

    #[test]
    fn record_and_get_match_history() {
        let db = ClientDatabase::open_in_memory().unwrap();
        db.record_match("TicTacToe", Some("bob"), "win").unwrap();
        db.record_match("Chess", None, "loss").unwrap();
        db.record_match("Connect4", Some("charlie"), "draw")
            .unwrap();

        let history = db.get_match_history(10).unwrap();
        assert_eq!(history.len(), 3);
        // Most recent first
        assert_eq!(history[0].game_type, "Connect4");
        assert_eq!(history[0].result, "draw");
        assert_eq!(history[1].opponent.as_deref(), None);
    }
}
