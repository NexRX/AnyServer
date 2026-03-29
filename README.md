<p align="center">
  <h1 align="center">AnyServer</h1>
  <p align="center">
    A self-hosted panel for running <strong>any binary</strong> as a managed server.<br>
    Auto-start, auto-restart, live console, file management, templates — one application, zero external dependencies.
  </p>
</p>

<p align="center">
  <a href="#quick-start"><strong>Quick Start</strong></a> ·
  <a href="#features"><strong>Features</strong></a> ·
  <a href="#configuration"><strong>Configuration</strong></a> ·
  <a href="#development"><strong>Development</strong></a>
</p>

---

## Quick Start

The fastest way to get running is with Docker:

```sh
git clone https://github.com/NexRX/AnyServer.git
cd AnyServer
docker compose up -d
```

Open **http://localhost:3001**, create your admin account, and set up your first server.

That's it. Server data persists in a Docker volume across restarts.

> **Ports:** `3001` — Web UI & API · `2222` — SFTP

<details>
<summary><strong>Standalone Docker image</strong></summary>

```sh
docker build -t anyserver .
docker run -d \
  -p 3001:3001 \
  -p 2222:2222 \
  -v anyserver-data:/app/data \
  anyserver
```

</details>

<details>
<summary><strong>Production hardening</strong></summary>

Set a stable JWT secret so sessions survive container restarts:

```sh
docker run -d \
  -p 3001:3001 \
  -p 2222:2222 \
  -v anyserver-data:/app/data \
  -e ANYSERVER_JWT_SECRET="$(openssl rand -base64 48)" \
  anyserver
```

See the [Configuration](#configuration) table for all available environment variables.

</details>

---

## Features

### 🖥️ Process Management

- **Host any binary** — point at any executable and manage it through the web UI
- **Start / stop / restart** with graceful shutdown via stdin commands
- **Auto-start & auto-restart** — servers survive reboots and crashes automatically
- **Process sandboxing** — optional Landlock, namespace isolation, and seccomp profiles per server

### 📡 Live Console

- Real-time stdout/stderr streaming via WebSocket
- Send stdin commands directly from the browser
- Scrollback history replayed on connect
- Automatic reconnection with ticket-based auth

### 📁 File Management

- **Web file manager** — browse, view, edit, create, and delete files per server
- Directory creation and Unix permission management
- Path traversal protection
- **SFTP access** — embedded SSH/SFTP server with per-server credentials, jailed to each server's directory *(authentication works; file transfer wire-up is in progress)*

### 📦 Templates & Pipelines

- **Built-in templates** for Minecraft (Paper), Valheim, and Terraria (TShock)
- Create and share your own templates with import/export
- **Install & update pipelines** — multi-step automation with archive extraction, downloads, and variable substitution
- **GitHub release integration** — dynamic version selection and asset downloads from any repo
- **Template parameters** — turn configs into reusable templates with user-fillable `${variables}`

### 🔒 Security & Access Control

- JWT authentication with refresh token rotation and family-based revocation
- API tokens with scoped permissions
- Invite code system for controlled registration
- Per-server permission levels: Viewer, Operator, Manager, Admin, Owner
- Rate limiting, CSP headers, CORS configuration, SSRF protection

### 📊 Monitoring & Alerts

- System health dashboard with CPU, memory, disk, and network metrics
- Configurable update checking via API polling, templates, or shell commands
- SMTP email alerts with per-server mute controls and cooldowns
- Java and .NET runtime detection, SteamCMD availability checking

### 🏗️ Architecture

- **End-to-end type safety** — Rust structs generate TypeScript types via [ts-rs](https://github.com/Aleph-Alpha/ts-rs)
- **No external database** — everything stored in an embedded SQLite database
- **Single binary** — frontend bundled into the release build via [rust-embed](https://github.com/pyrossh/rust-embed)

---

## Configuration

| Variable | Default | Description |
|---|---|---|
| `ANYSERVER_DATA_DIR` | `./data` | Root directory for all server data |
| `ANYSERVER_HTTP_PORT` | `3001` | HTTP API and WebSocket port |
| `ANYSERVER_SFTP_PORT` | `2222` | Embedded SFTP server port |
| `ANYSERVER_JWT_SECRET` | *generated* | Secret for signing JWTs. If unset, a random key is persisted to `data/jwt_secret`. **Set this in production.** |
| `ANYSERVER_CORS_ORIGIN` | *any* | Allowed CORS origin(s), comma-separated. Defaults to any origin in dev, same-origin in production builds. |
| `ANYSERVER_TRUSTED_PROXIES` | *none* | Trusted reverse-proxy IPs/CIDRs (e.g. `127.0.0.1,10.0.0.0/8`). **Required behind a reverse proxy** — without it, all requests appear to come from the proxy's IP. |
| `ANYSERVER_COOKIE_SECURE` | `auto` | `Secure` flag on refresh cookies. `true` = HTTPS only, `false` = plain HTTP, `auto` = based on build type. |
| `ANYSERVER_CSP` | *auto* | Custom `Content-Security-Policy` header. Empty string disables it. |
| `ANYSERVER_DB_MAX_CONNECTIONS` | `16` | SQLite connection pool size. |

Each server gets its own directory under `data/servers/<uuid>/`. All file access is jailed to that directory.

---

## Development

### Prerequisites

- **Rust** 1.75+ (with `cargo`)
- **Node.js** 20+ (with `pnpm` or `npm`)

### Getting Started

```sh
# 1. Start the backend (API on :3001, SFTP on :2222)
cd backend
cargo run

# 2. Start the frontend dev server (SPA on :3000, proxies /api to backend)
cd frontend
pnpm install
pnpm run dev
```

Open **http://localhost:3000**, create your admin account, and you're developing.

### TypeScript Bindings

Rust types are automatically exported to `frontend/src/types/bindings.ts` via ts-rs. Regenerate them with:

```sh
cd backend && cargo test
```

The generated file is committed to the repo, so you can skip this step initially.

### Production Build

```sh
cd frontend && pnpm run build
cd ../backend && cargo build --release --features bundle-frontend
```

This produces a single binary with the frontend embedded.

---

## Testing

| Layer | Command | Location | What it covers |
|---|---|---|---|
| **Integration** | `cargo test --test integration` | `backend/tests/integration/` | API routes via in-process Axum router |
| **Unit** | `pnpm test` | `frontend/src/**/*.test.*` | Components, utilities, types (Vitest) |
| **E2E** | `pnpm test:e2e` | `frontend/e2e/` | Full stack with a real browser (Playwright) |
| **E2E (for NixOS)** | `cd frontend && nix-shell --run "pnpm test:e2e"` | `frontend/e2e/` | Full stack with a real browser (Playwright), but with all runtime dependencies for testing via nix |

---

## License

AnyServer is licensed under the [GNU Affero General Public License v3.0](LICENSE).

You are free to use, modify, and distribute this software. If you run a modified version as a network service, you must make the source code available to its users. See the [LICENSE](LICENSE) file for full terms.
