# Game Center CLI

A cross-platform, multiplayer game platform running entirely in the terminal. Server-authoritative architecture over WebSocket+TLS, with a ratatui-based TUI client.

Play classic games against a bot locally or against other players online.

## Games

| Game | Status | Solo vs Bot | Online Multiplayer |
|------|--------|-------------|-------------------|
| Tic-Tac-Toe | Available | Easy / Hard (minimax) | 2 players |
| Connect 4 | Available | Easy / Hard (alpha-beta depth 6) | 2 players |
| Checkers | Planned | - | - |
| Chess | Planned | - | - |
| Snake | Planned | - | - |
| Block Breaker | Planned | - | - |
| Pacman | Planned | - | - |

## Quick Start

### Prerequisites

- Rust stable toolchain (`rustup` recommended)
- OpenSSL development headers (for TLS)

### Play Solo (no server needed)

```bash
cargo run -p gc-client
```

Login with any credentials, then press **B** in the lobby to start a solo game against the bot. Choose **E** for Easy or **H** for Hard difficulty.

### Run the Server

```bash
# Generate self-signed certs for development
mkdir -p certs
openssl req -x509 -newkey rsa:2048 -keyout certs/server.key -out certs/server.crt \
  -days 365 -nodes -subj '/CN=localhost'

# Copy and edit server config
cp server.example.toml server.toml

# Set the JWT signing secret
export GC_JWT_SECRET="your-secret-here"

# Start the server
cargo run -p gc-server
```

The server listens on `wss://0.0.0.0:8443` by default.

### Connect a Client

```bash
cargo run -p gc-client
```

Register or login, then create or join a room from the lobby.

## Controls

### Lobby
| Key | Action |
|-----|--------|
| **B** | Play vs Bot (solo) |
| **C** | Create online room |
| **Enter** | Join selected room |
| **R** | Refresh room list |
| **Up/Down** | Navigate room list |
| **Esc** | Quit |

### In Game (Tic-Tac-Toe)
| Key | Action |
|-----|--------|
| **Arrow keys** | Move cursor |
| **Enter** | Place piece |
| **R** | Rematch (after game over) |
| **Esc** | Leave game |

### In Game (Connect 4)
| Key | Action |
|-----|--------|
| **Left/Right** | Move cursor |
| **Enter** | Drop piece |
| **R** | Rematch (after game over) |
| **Esc** | Leave game |

## Architecture

```
game-center-cli/
├── crates/
│   ├── shared/    (gc-shared)  — Protocol, types, game engines, i18n
│   ├── server/    (gc-server)  — WebSocket+TLS server
│   ├── client/    (gc-client)  — TUI client (ratatui)
│   └── installer/ (gc-installer) — Installer (planned)
├── i18n/          — Translation files (EN, FR)
└── server.example.toml
```

- **Wire protocol**: MessagePack over WebSocket+TLS. Every message wrapped in `Envelope { version, seq, payload }`.
- **Server authority**: Server validates all moves via `GameEngine::validate_move()` before broadcasting. Clients cannot cheat.
- **Authentication**: Argon2 password hashing, JWT tokens (HS256, 7-day expiry).
- **Game engines**: Shared between client and server. The `GameEngine` trait defines `initial_state`, `validate_move`, `apply_move`, `is_terminal`, `current_player`.
- **i18n**: English and French. Auto-detected from `$LANG`.

## Configuration

### Server (`server.toml`)

```toml
bind_addr = "0.0.0.0:8443"
log_level = "info"

[tls]
cert_path = "certs/server.crt"
key_path = "certs/server.key"

[auth]
jwt_secret_env = "GC_JWT_SECRET"    # Env var name, NOT the secret itself
token_expiry_secs = 604800          # 7 days

[database]
path = "gamecenter.db"
```

### Client

Client config is stored at `~/.gamecenter/config.toml`. The client also maintains a local SQLite database for profile, auth tokens, and match history.

## Development

```bash
cargo fmt --all -- --check           # Check formatting
cargo clippy --workspace -- -D warnings  # Lint
cargo test --workspace               # All tests
cargo test -p gc-shared --test regression  # Regression suite
```

CI runs on Linux, macOS, and Windows via GitHub Actions.

## License

MIT
