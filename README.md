# ForgePulse

ForgePulse is a local-first Windows performance laboratory. It collects evidence,
records synchronized system state, detects measurable incidents, and is designed
to run reversible A/B experiments without cloud services or invasive kernel code.

This repository currently contains the first production foundation slice:

- a Rust workspace with explicit subsystem boundaries;
- a Windows service/console host with adaptive real CPU, memory, and process sampling;
- authenticated, replay-resistant local named-pipe IPC;
- SQLite migrations and compressed time-series chunks;
- a command-line client and a Tauri 2 + TypeScript desktop dashboard;
- deterministic incident, statistics, automation, profile, rollback, export, and
  validation primitives used by later phases.

No dashboard value is synthetic. Unsupported or unavailable metrics remain absent
and are represented through collector capability state.

## Prerequisites

- Windows 10 or Windows 11
- Stable Rust with the MSVC toolchain
- Node.js LTS
- Microsoft C++ Build Tools and WebView2 (for the desktop shell)

## Build and verify

```powershell
cargo fmt --all --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd forge-ui
npm.cmd install
npm.cmd run build
```

## Run locally

Start the service in foreground development mode, then use the CLI or UI:

```powershell
cargo run -p forge-service -- console
cargo run -p forge-cli -- status
cargo run -p forge-cli -- processes --limit 20
cd forge-ui
npm.cmd run tauri dev
```

Runtime state is stored below `%LOCALAPPDATA%\\ForgePulse`. The service creates a
32-byte IPC key using exclusive file creation. Production installation will apply
an explicit Windows ACL before elevating the service account; until that installer
step lands, foreground mode intentionally runs as the interactive user.

See [Architecture](docs/ARCHITECTURE.md), [Foundation status](docs/FOUNDATION_STATUS.md),
[Data storage](docs/DATA_STORAGE.md), [Threat model](docs/THREAT_MODEL.md), and
[Privacy](docs/PRIVACY.md).
