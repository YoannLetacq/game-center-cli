use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

#[allow(dead_code)] // Methods used in tests and later phases
impl Database {
    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
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
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER PRIMARY KEY
            );

            CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY,
                username TEXT UNIQUE NOT NULL,
                password_hash TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS match_results (
                id TEXT PRIMARY KEY,
                game_type TEXT NOT NULL,
                player1_id TEXT NOT NULL,
                player2_id TEXT NOT NULL,
                winner_id TEXT,
                finished_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (player1_id) REFERENCES users(id),
                FOREIGN KEY (player2_id) REFERENCES users(id)
            );",
        )?;
        Ok(())
    }

    /// Register a new user.
    pub fn create_user(
        &self,
        id: &str,
        username: &str,
        password_hash: &str,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO users (id, username, password_hash) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, username, password_hash],
        )?;
        Ok(())
    }

    /// Look up a user by username. Returns (id, password_hash).
    pub fn get_user_by_username(
        &self,
        username: &str,
    ) -> Result<Option<(String, String)>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare("SELECT id, password_hash FROM users WHERE username = ?1")?;
        let mut rows = stmt.query(rusqlite::params![username])?;

        if let Some(row) = rows.next()? {
            Ok(Some((row.get(0)?, row.get(1)?)))
        } else {
            Ok(None)
        }
    }

    /// Check if a username is already taken.
    pub fn username_exists(&self, username: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM users WHERE username = ?1",
            rusqlite::params![username],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Get a clone of the connection Arc for async wrapping.
    pub fn conn(&self) -> Arc<Mutex<Connection>> {
        self.conn.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_find_user() {
        let db = Database::open_in_memory().unwrap();
        db.create_user("u1", "alice", "hash123").unwrap();

        let result = db.get_user_by_username("alice").unwrap();
        assert!(result.is_some());
        let (id, hash) = result.unwrap();
        assert_eq!(id, "u1");
        assert_eq!(hash, "hash123");
    }

    #[test]
    fn username_not_found() {
        let db = Database::open_in_memory().unwrap();
        let result = db.get_user_by_username("nobody").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn duplicate_username_rejected() {
        let db = Database::open_in_memory().unwrap();
        db.create_user("u1", "alice", "hash1").unwrap();
        let result = db.create_user("u2", "alice", "hash2");
        assert!(result.is_err());
    }

    #[test]
    fn username_exists_check() {
        let db = Database::open_in_memory().unwrap();
        assert!(!db.username_exists("bob").unwrap());
        db.create_user("u1", "bob", "hash").unwrap();
        assert!(db.username_exists("bob").unwrap());
    }
}
