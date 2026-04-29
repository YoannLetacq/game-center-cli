mod auth;
mod config;
mod database;
mod engine;
mod lobby;
mod observability;
mod ws;

use std::sync::Arc;

use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tracing::{error, info, warn};

/// Global cap on concurrent connections — prevents FD exhaustion / TLS handshake DoS.
const MAX_CONCURRENT_CONNECTIONS: usize = 1024;

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

    // Enforce minimum secret entropy — a short secret makes token forgery trivial.
    if jwt_secret.len() < 32 {
        error!(
            "JWT secret is too short ({} bytes, minimum 32). \
             Generate a strong secret with: openssl rand -base64 32",
            jwt_secret.len()
        );
        std::process::exit(1);
    }

    let jwt = auth::jwt::JwtManager::new(jwt_secret.as_bytes(), config.auth.token_expiry_secs);

    // Cap concurrent Argon2 operations to half the available CPUs (min 2).
    let argon2_permits = {
        let cpus = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2);
        std::cmp::max(2, cpus / 2)
    };
    let argon2_semaphore = Arc::new(Semaphore::new(argon2_permits));

    let auth_limiter = auth::rate_limit::AuthRateLimiter::new();

    let lobby = Arc::new(lobby::manager::LobbyManager::new());
    let players = ws::handler::PlayerRegistry::default();
    lobby.attach(Arc::clone(&players));
    let state = Arc::new(ServerState {
        db,
        jwt,
        lobby,
        players,
        auth_limiter,
        argon2_semaphore,
    });

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

    let conn_semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_CONNECTIONS));

    loop {
        let (tcp_stream, peer_addr) = match listener.accept().await {
            Ok(s) => s,
            Err(e) => {
                error!("accept failed: {e}");
                continue;
            }
        };

        // Reject connection if global cap reached (rather than queueing indefinitely).
        let permit = match Arc::clone(&conn_semaphore).try_acquire_owned() {
            Ok(p) => p,
            Err(_) => {
                warn!(%peer_addr, "connection cap reached; dropping");
                drop(tcp_stream);
                continue;
            }
        };

        info!(%peer_addr, "incoming connection");

        let tls_acceptor = tls_acceptor.clone();
        let state = state.clone();

        tokio::spawn(async move {
            match tls_acceptor.accept(tcp_stream).await {
                Ok(tls_stream) => {
                    handle_connection(tls_stream, peer_addr, state).await;
                }
                Err(e) => {
                    error!(%peer_addr, "TLS handshake failed: {e}");
                }
            }
            drop(permit);
        });
    }
}
