# Game Center CLI

A cross-platform, multiplayer game platform running entirely in the terminal. Server-authoritative architecture over WebSocket+TLS, with a ratatui-based TUI client.

Play classic games against a bot locally or against other players online.

## Games

| Game | Status | Solo vs Bot | Online Multiplayer |
|------|--------|-------------|-------------------|
| Tic-Tac-Toe | Available | Easy / Hard (minimax) | 2 players |
| Connect 4 | Available | Easy / Hard (alpha-beta depth 6) | 2 players |
| Checkers | Available | Easy / Hard | 2 players |
| Chess | Available | Easy / Hard | 2 players |
| Snake | Available | Easy / Hard | 2 players |
| Block Breaker | Planned | - | - |
| Pacman | Planned | - | - |

## Quick Start

### Prerequisites

- Rust stable toolchain (`rustup` recommended)
- OpenSSL development headers (for TLS)

### Quick Dev Start (no certs or secrets needed)

For a fresh clone that "just works", use dev mode:

```bash
# Server with auto-generated ephemeral TLS cert and built-in JWT secret
cargo run -p gc-server -- --dev
```

In another terminal:

```bash
# Client connecting to dev server
cargo run -p gc-client -- --dev
```

Login with any credentials and start playing. Certificates and secrets are generated/synthesized at startup—nothing to configure.

Alternatively, set the environment variable:

```bash
GC_DEV=1 cargo run -p gc-server
GC_DEV=1 cargo run -p gc-client
```

Or for client-only insecure TLS:

```bash
GC_INSECURE_TLS=1 cargo run -p gc-client
```

### Production Setup

For a real deployment, generate proper TLS certificates and a strong JWT secret:

```bash
# Generate self-signed certs for development
mkdir -p certs
openssl req -x509 -newkey rsa:2048 -keyout certs/server.key -out certs/server.crt \
  -days 365 -nodes -subj '/CN=localhost'

# Copy and edit server config
cp server.example.toml server.toml

# Generate a strong JWT secret
export GC_JWT_SECRET="$(openssl rand -base64 32)"

# Start the server
cargo run -p gc-server
```

The server listens on `wss://0.0.0.0:8443` by default.

### Connect a Client (Production)

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
| **G** | Cycle game type (Tic-Tac-Toe → Connect 4 → Checkers → Chess → Snake) |
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

### In Game (Checkers)
| Key | Action |
|-----|--------|
| **Arrow keys** | Move cursor |
| **Enter** | Select piece / confirm target / continue jump chain |
| **Esc** | Cancel selection / leave game |

### In Game (Chess)
| Key | Action |
|-----|--------|
| **Arrow keys** | Move cursor |
| **Enter** | Select piece / confirm target / choose promotion piece |
| **Esc** | Cancel selection / leave game |

### In Game (Snake)
| Key | Action |
|-----|--------|
| **Arrow keys** | Change snake direction |
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
- **Game engines**: Two traits for different game styles:
  - **Turn-based** (`GameEngine`): Tic-Tac-Toe, Connect 4, Checkers, Chess. Shared between client and server.
  - **Realtime** (`RealtimeGameEngine`): Snake. Server-driven tick loop (10 Hz) with client inputs buffered per-tick.
- **Authentication**: Argon2 password hashing (concurrency-capped to prevent DoS), JWT tokens (HS256, 7-day expiry).
- **i18n**: English and French. Auto-detected from `$LANG`.

## Security

The server implements defense-in-depth hardening:

- **Connection cap**: Maximum 1024 concurrent connections to prevent FD exhaustion and TLS handshake DoS.
- **Auth rate limiting**: Failed login attempts are rate-limited per IP to prevent brute force.
- **Argon2 concurrency cap**: Password hashing semaphore limits concurrent operations to half available CPUs (minimum 2), preventing hash-bomb DoS.
- **Payload caps**: Message size limits prevent unbounded memory consumption.
- **Race-free room operations**: Room state transitions are atomic; no TOCTOU windows.
- **Server-authoritative validation**: All moves validated server-side before broadcasting. Client-side validation is for UX only.
- **Session ownership**: Disconnect messages only accepted from the session's owning connection.

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
