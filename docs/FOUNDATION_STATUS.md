# Foundation status

This slice intentionally stops before Phase 2 expansion. It establishes and tests
the boundaries that later collectors and actions must use.

## Implemented

- all requested workspace crates with concrete domain models and validation;
- documented Win32 CPU tick, physical/commit memory, process CPU, process memory,
  path, handle, and strong PID/creation-time identity collection;
- adaptive sampling with incident cadence and measured wall-time backoff;
- compressed MessagePack + zstd SQLite metric chunks and the complete initial
  relational schema, migrations, integrity checks, and bounded retention deletion;
- signed named-pipe IPC with versioning, frame limits, timestamp checks, nonce replay
  prevention, rate limiting, and an enum command allowlist;
- Windows service lifecycle plus portable foreground mode;
- CLI status, process, live monitor, doctor, and ping commands with JSON output;
- Tauri 2/TypeScript live dashboard with offline, loading, unavailable-capability,
  search, responsive, and keyboard-accessible states;
- deterministic statistics, incident, automation dry-run, application profile,
  experiment rollback, report, anonymization, hardware, and network primitives;
- local-only privacy posture and a current-user NSIS bundle configuration.

## Deliberately gated

The installed service is not elevated yet. The named pipe now has an explicit,
local-only ACL for authenticated desktop and packaged clients and still requires
HMAC authentication. A production privileged install additionally requires an
installer-created service-owned state directory, explicit key/database file ACLs,
and a user-to-service authentication provisioning flow. Foreground mode is usable
without that boundary. No mutating IPC command exists in this slice.

GPU, disk, network, event log, sessions, historical timeline, incidents persistence,
and experiments are the next implementation phases; the UI labels these as not yet
connected and never substitutes mock readings.

## Verification environment note

Rust/Tauri native compilation requires Visual C++ Build Tools and the Windows SDK.
If `link.exe` is absent, install the `Desktop development with C++` workload, open a
Developer PowerShell, and rerun the commands in the root README. The TypeScript/Vite
build is independent and can still be verified with `npm.cmd run build`.
