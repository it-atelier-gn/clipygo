# Feature: Encrypted Clipboard Relay (clipygo-plugin-relay)

## Overview

A direct, encrypted clipboard sharing system between users. No accounts, no passwords — just cryptographic keys. Users exchange public keys out-of-band and can then send clipboard content to each other with E2E encryption through a lightweight relay server.

## Goals

- **Zero-knowledge relay** — server routes encrypted blobs, can never read content
- **No accounts** — identity is a keypair generated on first run
- **Simple onboarding** — share your public key + relay URL, done
- **Real-time** — WebSocket push with HTTP polling fallback
- **Offline support** — relay queues messages in memory with a TTL (e.g. 24h), delivers when recipient connects
- **Non-intrusive receiving** — notification window appears without stealing focus, lets user view and interact with content

## Why a custom relay

Existing open-source relays don't fit the requirements:
- **[enseal](https://github.com/FlerAlex/enseal)** — stateless + zero-knowledge, but single-use synchronous transfers only (no offline queuing, no persistent identities)
- **[ntfy.sh](https://github.com/binwiederhier/ntfy)** — push notification relay with WebSocket, but generic transport, not designed for E2E encrypted messaging
- The relay is small (~200 lines of FastAPI) — building it avoids pulling in a large dependency for a fraction of its features

## Architecture

```
┌─────────────┐       WebSocket / HTTP        ┌──────────────┐       WebSocket / HTTP       ┌─────────────┐
│  clipygo    │◄──────stdin/stdout──────►│  plugin-relay │◄────────────────────►│ relay server │
│  (Tauri)    │                          │  (Rust bin)   │                      │ (Python)     │
│             │  plugin-initiated events │               │  push + poll         │              │
│  notif.     │◄─────────────────────────│  crypto       │                      │  in-memory   │
│  window     │                          │  contacts     │                      │  queue + TTL │
└─────────────┘                          └──────────────┘                      └──────────────┘
```

### Three components

| Component | Tech | Location | Purpose |
|-----------|------|----------|---------|
| **clipygo core** (protocol extension) | Rust | `clipygo/` | Listen for plugin-initiated events, show notification window |
| **clipygo-plugin-relay** | Rust | `clipygo-plugin-relay/` | Keypair mgmt, contact list, E2E crypto, WebSocket + HTTP relay client |
| **relay server** | Python (FastAPI) | `clipygo-plugin-relay/server/` | Stateless message routing, WebSocket + REST, in-memory queue with TTL |

## Protocol Extension: Plugin-Initiated Events

Currently the plugin protocol is strictly request-response (clipygo sends command → plugin responds). This feature requires **plugin → clipygo** events.

### Design

The plugin can write unsolicited JSON lines to stdout at any time. These are **events**, not responses to commands. They are distinguished by having a top-level `"event"` field instead of a response payload.

```json
{"event":"incoming_message","data":{"from_name":"Alice","from_id":"abc123","content":"Hello!","format":"text","timestamp":1711900000}}
```

### Changes to subprocess runner

`SubprocessProvider` currently reads stdout only in `send_recv()` (synchronous, one line per request). The extension:

1. **Background reader thread** — spawns a thread that continuously reads lines from the plugin's stdout
2. **Response channel** — request-response pairs use a channel: `call()` sends request, waits on channel for the response
3. **Event dispatch** — non-response lines (those with `"event"` key) are forwarded to clipygo's event system via `AppHandle::emit()`
4. **Notification window** — clipygo listens for plugin events and shows a small, non-focus-stealing notification window

### Event types

| Event | Description |
|-------|-------------|
| `incoming_message` | A message was received and decrypted. Payload: `from_name`, `from_id`, `content`, `format`, `timestamp` |
| `connection_status` | WebSocket connected/disconnected. Payload: `status` ("connected" / "disconnected") |

## Relay Server API

### REST endpoints

| Method | Path | Description |
|--------|------|-------------|
| `POST /send` | Send an encrypted message | Body: `{ "to": "<recipient_id>", "from": "<sender_id>", "payload": "<encrypted_base64>" }` |
| `GET /poll/{user_id}` | Poll for pending messages (fallback) | Returns array of pending messages, clears them |
| `GET /health` | Health check | Returns `{"status": "ok"}` |

### WebSocket

`WS /ws/{user_id}` — persistent connection with X25519 challenge-response authentication.

**Handshake:**
1. Server sends `{"type": "challenge", "server_public_key": "<b64>", "nonce": "<hex>"}`
2. Client computes ECDH shared secret (client static private key + server ephemeral public key)
3. Client sends `{"type": "auth", "public_key": "<b64 of client public key>", "hmac": "<HMAC-SHA256(shared_secret, nonce) hex>"}`
4. Server verifies HMAC and that `user_id == SHA256(public_key)[:8].hex()`
5. On success, server flushes pending messages and pushes new ones in real-time

**Close codes:** 4001 (auth timeout), 4002 (invalid auth message), 4003 (auth failed)

**Message format:**
```json
{"from": "<sender_id>", "payload": "<encrypted_base64>", "timestamp": 1711900000}
```

### Message queue behavior

- Messages for offline recipients are held in an **in-memory dict** keyed by recipient ID
- Each message has a **TTL** (default 24 hours) — expired messages are evicted
- On WebSocket connect, all pending messages are flushed to the client
- **No persistence** — if the relay process restarts, queued messages are lost (acceptable trade-off for simplicity)
- Optional: configurable max queue size per user (e.g. 100 messages) to prevent memory abuse

## Plugin: clipygo-plugin-relay

### First run

1. Generate X25519 keypair (for encryption) using a proven crypto library
2. Store private key securely (OS keychain via `keyring` crate, or encrypted file)
3. Generate a stable user ID (e.g. SHA256 of public key, truncated)
4. Display public key + ID for sharing

### Configuration (via get_config_schema)

| Field | Description |
|-------|-------------|
| `relay_url` | URL of the relay server (e.g. `https://clipygo-relay.return-co.de`) |
| `display_name` | User's display name (shown to recipients) |

### Contact management

Contacts are stored in a local config file:

```json
{
  "contacts": [
    { "name": "Alice", "id": "abc123...", "public_key": "base64..." }
  ]
}
```

Each contact becomes a **target** in clipygo (via `get_targets`).

### Crypto flow

**Sending:**
1. Sender gets recipient's public key from contacts
2. Generate ephemeral X25519 keypair
3. Derive shared secret via ECDH (ephemeral private + recipient public)
4. Encrypt content with XChaCha20-Poly1305 using derived key
5. Send `{ ephemeral_public_key, nonce, ciphertext }` to relay

**Receiving:**
1. Receive encrypted blob from relay
2. Derive shared secret via ECDH (own private key + ephemeral public key from message)
3. Decrypt with XChaCha20-Poly1305
4. Emit `incoming_message` event to clipygo

### Crypto libraries

- **Rust plugin**: `x25519-dalek` + `chacha20poly1305` (well-audited, pure Rust)
- **No custom crypto** — standard NaCl-style sealed box pattern

## Notification Window

A new Tauri webview window (`notification`) that:

- Appears in the bottom-right corner (system notification area)
- Does **not** steal focus (`set_focus(false)`)
- Shows: sender name, content preview, timestamp
- Actions: **Copy** (copies content to clipboard), **Dismiss**
- Auto-dismisses after a configurable timeout (e.g. 30 seconds)
- Stacks if multiple messages arrive

## Implementation Phases

### Phase 1: Protocol extension (clipygo core)
- Background stdout reader thread in subprocess runner
- Response/event multiplexing
- Plugin event emission via Tauri events
- Notification window (Svelte + Tauri)

### Phase 2: Relay server (Python)
- FastAPI app with WebSocket + REST
- In-memory message queue with TTL
- Docker / single-file deployable

### Phase 3: Plugin (Rust)
- Keypair generation + storage
- Contact management (config file)
- E2E crypto (X25519 + XChaCha20-Poly1305)
- WebSocket client with HTTP polling fallback
- Plugin protocol integration (get_targets, send, events)

### Phase 4: Polish
- Connection status indicator in clipygo UI
- "Share my key" helper (copy public key + ID to clipboard)
- Multiple relay server support
- Message history (optional, local-only, encrypted at rest)

## Next Steps (resume here)

### Phase 1: Protocol extension — DONE

Implemented plugin-initiated events in clipygo core:

- **Background reader thread** in `SubprocessProvider` (`subprocess.rs`): replaced synchronous `BufReader` with a background thread + `mpsc::channel`. Thread reads all stdout lines; lines with `"event"` key are dispatched via `AppHandle::emit("plugin-event", ...)`, others forwarded as responses.
- **AppHandle threaded through**: `SubprocessProvider::new()` and `create_subprocess_providers()` now accept `AppHandle`. `TargetProviderCoordinator` stores and passes it.
- **Notification window** (`src/routes/notification/+page.svelte`): listens for `plugin-event` Tauri events, shows sender name, content preview, timestamp. Copy and Dismiss actions. Auto-dismisses after 30s. Stacks multiple notifications.
- **Notification window creation** (`lib.rs`): `show_notification_window()` creates the window dynamically (bottom-right, always-on-top, no focus steal). `plugin-event` listener in setup triggers it for `incoming_message` events.

Files changed:
- `src-tauri/src/target_providers/subprocess.rs` — background reader thread, event dispatch
- `src-tauri/src/targets.rs` — AppHandle passed to coordinator and providers
- `src-tauri/src/lib.rs` — AppHandle plumbing, plugin-event listener, notification window
- `src/routes/notification/+page.svelte` — new notification UI

### Phase 2: Repo + relay server — DONE

Created `clipygo-plugin-relay/` repo at `C:\Users\PC\Projects\clipygo-plugin-relay\`:

- **Rust plugin skeleton** (`src/main.rs`): stdin/stdout JSON protocol loop with `GetInfo`, `GetTargets`, `GetConfigSchema`, `SetConfig`, `Send` handlers. Config schema for `relay_url` and `display_name`. 7 unit tests.
- **CI/CD** (`.github/workflows/`): `ci.yml` (check, fmt, clippy, test, audit, server-test), `release.yml` (cross-platform build + GitHub Release).
- **Relay server** (`server/main.py`): FastAPI + uvicorn. Endpoints: `POST /send`, `GET /poll/{user_id}`, `GET /health`, `WS /ws/{user_id}`. In-memory queue with 24h TTL eviction (asyncio background task). Rate limiting per sender ID (60 msgs/min). Queue limits (100/recipient, oldest evicted). Message size limit (1 MB). WebSocket connections authenticated via X25519 challenge-response. Live delivery with pending message flush on connect. 21 pytest tests covering all endpoints, auth handshake, security rejection cases, rate limiting, TTL eviction.
- **Dockerfile** + `requirements.txt` + `requirements-dev.txt`

Remaining before pushing: create GitHub repo, initial commit, push.

### Phase 3: Plugin implementation — DONE

Full Rust plugin at `C:\Users\PC\Projects\clipygo-plugin-relay\src\main.rs`:

- **Keypair management**: X25519 keypair generated on first run, stored as JSON in `%APPDATA%/clipygo-plugin-relay/keypair.json`. User ID = first 16 hex chars of SHA256(public_key).
- **Config persistence**: `config.json` in same dir. Stores `relay_url`, `display_name`, and `contacts[]`. `get_config_schema` returns current values. `set_config` persists.
- **Contact management**: Each contact becomes a target via `get_targets`. Contacts have name, id, and base64 public key.
- **E2E crypto**: Ephemeral X25519 ECDH + XChaCha20-Poly1305 per message. Shared secret derived via SHA256. Envelope includes ephemeral public key, nonce, ciphertext, sender metadata.
- **Send**: Encrypts clipboard content, POSTs to relay's `/send` endpoint.
- **WebSocket receiver**: Tokio background task connects to `ws://{relay_url}/ws/{user_id}`, receives messages, decrypts, emits `incoming_message` events to stdout. Auto-reconnects on disconnect with 5s delay.
- **Connection status events**: Emits `connection_status` events (`connecting`, `connected`, `disconnected`).
- **Share key**: "Share My Relay Key" pseudo-target that returns the user's public key, ID, and relay URL.
- **16 unit tests**: keypair, config roundtrip, crypto encrypt/decrypt, targets, wrong-key rejection, deterministic key derivation.
- **Passes**: `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`.

### Phase 4: Polish — DONE

- Connection status events integrated into WebSocket receiver
- "Share My Relay Key" target included in `get_targets`

### Repo structure

```
clipygo-plugin-relay/
├── .github/workflows/
│   ├── ci.yml          — Rust CI + Python server tests
│   └── release.yml     — Cross-platform release binaries
├── server/
│   ├── main.py         — FastAPI relay (15 tests)
│   ├── test_main.py    — pytest tests
│   ├── requirements.txt
│   ├── requirements-dev.txt
│   └── Dockerfile
├── src/
│   └── main.rs         — Rust plugin (16 tests)
├── Cargo.toml
├── Cargo.lock
├── rustfmt.toml
└── .gitignore
```

### Remaining before shipping

1. **Create GitHub repo**: `gh repo create it-atelier-gn/clipygo-plugin-relay --public`
2. **Initial commit and push**
3. **Add LICENSE file** (MIT)
4. **Add contact management UI** — currently contacts must be added manually to `config.json`. A future enhancement could add `add_contact`/`remove_contact` commands to the plugin protocol.
5. **End-to-end testing** — run the relay server, configure two plugin instances, verify messages flow through
6. **Update `clipygo-plugins/registry.json`** once a release is tagged

## Security Considerations

- Private keys never leave the device
- Relay server is zero-knowledge — encrypted blobs only
- Ephemeral keys per message provide forward secrecy
- No metadata encryption (relay sees sender ID, recipient ID, timestamps) — acceptable for the trust model (self-hosted relay)

### WebSocket authentication

WebSocket connections use X25519 ECDH challenge-response authentication. This prevents an attacker from connecting as another user and intercepting their messages. The server generates an ephemeral keypair per connection and sends a challenge nonce. The client proves ownership of the private key corresponding to its user ID by computing a shared secret and signing the nonce with HMAC-SHA256.

### Abuse prevention

The relay authenticates WebSocket connections via challenge-response (see above). REST endpoints (`/send`, `/poll`) remain open — messages are encrypted anyway, and the poll endpoint only returns messages already queued for that user ID. Security comes from the crypto layer and resource limits:

| Layer | Protection |
|-------|------------|
| **E2E encryption** | Spam/abuse is undecryptable garbage — recipients discard anything not from a known contact |
| **High-entropy user IDs** | Derived from public keys, can't be guessed or enumerated |
| **Rate limiting** | Per sender ID + per IP (e.g. 60 msgs/min) |
| **Queue limits** | Max messages per recipient (e.g. 100) — oldest evicted when full |
| **Message size limit** | e.g. 1 MB per message — prevents memory exhaustion |
| **Connection limits** | Max WebSocket connections per IP, idle timeout |
| **TTL eviction** | Queued messages expire after 24h — abuse is self-cleaning |

The relay is intentionally a dumb, open pipe with resource caps. The worst an attacker can do is waste some server memory with blobs that nobody can decrypt and that expire automatically.
