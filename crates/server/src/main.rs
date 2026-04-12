mod auth;
mod config;
mod database;
mod engine;
mod lobby;
mod observability;
mod ws;

use std::sync::Arc;

use tokio::net::TcpListener;
use tracing::{error, info};

use config::ServerConfig;
use database::Database;
use ws::handler::{ServerState, handle_connection};
use ws::tls::build_tls_acceptor;

#[tokio::main]
async fn main() {
    // Load config
    let config = match ServerConfig::load(std::path::Path::new("server.toml")) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("failed to load server.toml: {e}, using defaults");
            ServerConfig::default()
        }
    };

    // Init tracing
    let log_level = config.log_level.as_deref().unwrap_or("info");
    observability::init_tracing(log_level);

    // Open database
    let db = match Database::open(&config.database.path) {
        Ok(db) => db,
        Err(e) => {
            error!("failed to open database: {e}");
            std::process::exit(1);
        }
    };

    // Read JWT secret from environment
    let jwt_secret = std::env::var(&config.auth.jwt_secret_env).unwrap_or_else(|_| {
        error!(
            "JWT secret env var '{}' not set — server cannot start securely",
            config.auth.jwt_secret_env
        );
        std::process::exit(1);
    });

    let jwt = auth::jwt::JwtManager::new(jwt_secret.as_bytes(), config.auth.token_expiry_secs);

    let lobby = lobby::manager::LobbyManager::new();
    let state = Arc::new(ServerState { db, jwt, lobby });

    // Spawn periodic room cleanup
    {
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                state.lobby.cleanup_empty_rooms().await;
            }
        });
    }

    // Build TLS
    let tls_acceptor = match build_tls_acceptor(&config.tls.cert_path, &config.tls.key_path) {
        Ok(a) => a,
        Err(e) => {
            error!("failed to load TLS certs: {e}");
            std::process::exit(1);
        }
    };

    // Bind TCP
    let listener = match TcpListener::bind(&config.bind_addr).await {
        Ok(l) => l,
        Err(e) => {
            error!("failed to bind {}: {e}", config.bind_addr);
            std::process::exit(1);
        }
    };

    info!(addr = %config.bind_addr, "server listening");

    loop {
        let (tcp_stream, peer_addr) = match listener.accept().await {
            Ok(s) => s,
            Err(e) => {
                error!("accept failed: {e}");
                continue;
            }
        };

        info!(%peer_addr, "incoming connection");

        let tls_acceptor = tls_acceptor.clone();
        let state = state.clone();

        tokio::spawn(async move {
            match tls_acceptor.accept(tcp_stream).await {
                Ok(tls_stream) => {
                    handle_connection(tls_stream, state).await;
                }
                Err(e) => {
                    error!(%peer_addr, "TLS handshake failed: {e}");
                }
            }
        });
    }
}
