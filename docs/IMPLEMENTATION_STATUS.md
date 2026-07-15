# ForgePulse implementation status

Verification date: 2026-07-14 (Europe/Berlin)

## Foundation decision

The Phase 1 foundation is complete and ready for Phase 2 development. Phase 2 was not
started. All required default-feature and all-feature compilation, lint, test, frontend,
metadata, dependency, package-isolation, and manual smoke gates passed.

The production privileged-service installer boundary remains deliberately gated as
described under **Remaining limitations**; this does not affect the verified current-user
console-mode foundation.

## Build environment

Native Cargo commands were run from `C:\Users\gianl\Documents\ForgePulse` after loading
the installed MSVC x64 environment with this exact wrapper:

```powershell
cmd.exe /d /s /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 >nul && cargo <arguments>'
```

Cargo repeatedly printed the environmental message
`warn: could not canonicalize path C:\Users\gianl`. It comes from the managed execution
environment's protected profile parent, did not identify a crate or source warning, and
did not change any command exit code. Rust/Clippy warning count is zero.

## Required command results

| Command | Result |
| --- | --- |
| `cargo check --workspace --all-targets` | Pass; all 17 workspace packages and targets compile |
| `cargo clippy --workspace --all-targets -- -D warnings` | Pass; zero Clippy/compiler warnings |
| `cargo test --workspace` | Pass; 17 passed, 0 failed, 0 ignored |
| `cargo fmt --all --check` | Pass |
| `npm.cmd run build` from `forge-ui` | Pass; TypeScript and Vite production build, 30 modules transformed |
| `cargo tree -d` | Pass; duplicate graph reviewed |
| `cargo metadata --no-deps --format-version 1` | Pass; 17 workspace members, valid metadata v1 |
| `cargo test --workspace --all-features` | Pass; 17 passed, 0 failed, 0 ignored |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Pass; zero Clippy/compiler warnings |

On this Windows host `npm` resolves to a PowerShell script blocked by execution policy,
so the executable-equivalent `npm.cmd run build` was used. Its final output was:

```text
vite v7.3.6 building client environment for production...
30 modules transformed.
dist/index.html                  0.45 kB
dist/assets/index-BmDCkYTn.css  7.63 kB
dist/assets/index-DMdMmFMX.js   202.22 kB
built in 848ms
```

Package isolation was also checked with the following exact command. Every package
completed successfully when selected independently:

```powershell
cmd.exe /d /s /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 >nul && for %p in (forge-core forge-monitor forge-events forge-storage forge-analysis forge-benchmark forge-experiments forge-profiles forge-automation forge-network forge-hardware forge-windows forge-security forge-export forge-service forge-cli forge-ui) do @cargo check -p %p || exit /b 1'
```

Test count is 17 unique tests. They were executed once with default features and once
with all features, for 34 successful test executions. The storage test uses a
`tempfile::TempDir` database and no test required administrator privileges, a fixed PID,
specific hardware, or a Windows username.

## Duplicate dependency review

The avoidable direct `rand 0.8` / `rand_core` / `getrandom 0.2` chain was removed.
Cryptographic key and nonce generation now use the already-current `getrandom 0.4` API
and propagate entropy failures.

The remaining version duplicates are transitive compatibility boundaries in Tauri,
SQLx, WebView2/Windows support, and their build tooling: `bitflags` 1/2, `foldhash`
0.1/0.2, `getrandom` 0.3/0.4, `hashbrown` 0.12/0.15/0.17, `indexmap` 1/2,
`thiserror` 1/2, `toml` 0.9/1.1, `windows-link` 0.1/0.2, `windows-sys` 0.59/0.61,
and `winnow` 0.7/1.0. Same-version entries also occur in distinct host/build/target
feature graphs. Forcing these versions together would fight upstream compatibility
requirements and was not done.

## Manual foundation smoke test

The service was executed in console mode with the real binary and argument:

```powershell
C:\Users\gianl\Documents\ForgePulse\target\debug\forge-service.exe console
```

The Vite development server and rebuilt Tauri executable were started with:

```powershell
npm.cmd run dev
C:\Users\gianl\Documents\ForgePulse\target\debug\forge-ui.exe
```

The Codex desktop and shell have intentionally different Windows sandbox tokens. Small
temporary launch/probe executables in `target` were therefore used only to pass these
exact real commands into the same desktop token as Tauri. They contained no mock
collector or service implementation and were deleted after the smoke test.

| Check | Result |
| --- | --- |
| Service starts in console mode | Pass; real collector, SQLite pool, and named-pipe host remained running |
| Tauri dashboard connects to service | Pass; header reported `Monitoring locally` through real Tauri `invoke` commands |
| Live CPU and memory update | Pass; CPU warmed from unavailable to 26.1% and later 23.2%/30.1%; memory showed 38% and changed from 12.2 to 12.3 GiB used |
| Real process inventory | Pass; 165-167 real processes with names, PIDs, paths, CPU, memory, and handles were displayed |
| Process exit and PID-reuse safety | Pass; Notepad was observed as PID 11812 with creation time `134285304369808262`; after closing it, PID 11812 was absent and the service continued sampling. Identity is `(pid, creation_time_100ns)`, and the PID-reuse unit test also passed |
| Samples written to SQLite | Pass; live inspection first found 8 chunks/240 samples and later 13 chunks/390 samples using `msgpack+zstd` schema version 1 |
| Restart and persistent read | Pass; service PID changed from 836 to 22280, the dashboard reconnected after refresh, `PRAGMA quick_check` returned `ok`, all 13 payload SHA-256 checksums validated, and `forge-storage::latest_snapshot` decoded sequence 180 with 161 real processes |
| Invalid IPC rejected | Pass; a structurally valid request with an invalid HMAC returned `authentication_failed`, `retryable: false`; the service stayed alive |
| Monitored process termination | Pass; Notepad and short-lived CLI probe processes exited without a service crash; samples advanced from 52 to 60 with zero drops during the explicit termination check |
| Adaptive sampling under load | Pass; 12 CPU workers produced observed intervals of 4000, 500, 1000, and 2000 ms as incident cadence and overhead backoff adapted; recovery returned to 2000 ms with zero dropped samples |
| Storage/retention configuration readable | Pass; `database_limit_mib = 2048`, `retention_days = 30`, `chunk_sample_count = 30`, with the expected local database path |

The invalid request received this exact service response:

```json
{"response":"error","data":{"code":"authentication_failed","message":"request authentication failed","retryable":false}}
```

After the restart, the actual storage decoding path was invoked with a temporary example
(deleted immediately afterward) using this exact command:

```powershell
cmd.exe /d /s /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 >nul && cargo run -p forge-storage --example read_persistent -- "C:\Users\gianl\AppData\Local\ForgePulse\ForgePulse\data\forgepulse.db"'
```

It returned:

```text
integrity=ok sequence=180 captured_at=2026-07-14 19:33:19.115991300 UTC processes=161
```

The adaptive probe recorded these meaningful transitions:

```text
load       176 ms: 2000 ms
load       953 ms: 4000 ms (collector-overhead backoff)
load      5078 ms:  500 ms (CPU incident cadence)
load      5997 ms: 1000 ms
load      7266 ms: 2000 ms
load      9293 ms: 1000 ms
load     10353 ms:  500 ms
load     11028 ms: 1000 ms
recovery 14596 ms: 4000 ms
recovery 18543 ms: 2000 ms
final    24596 ms: 2000 ms, zero dropped samples
```

## Native and IPC audit

- Every `OpenProcess` handle is immediately wrapped in non-cloneable RAII ownership and
  closed exactly once with `CloseHandle`.
- Pipe security descriptors allocated by
  `ConvertStringSecurityDescriptorToSecurityDescriptorW` are released exactly once with
  `LocalFree`; the descriptor remains alive for the complete `CreateNamedPipeW` call.
- `unsafe` blocks are limited to Win32 calls and each block documents pointer validity,
  buffer size, lifetime, ownership, and cleanup assumptions.
- Win32 structure sizes and returned lengths use checked conversions. FILETIME halves
  use lossless `u32` to `u64` widening, memory counters use checked `usize` to `u64`
  conversion, tick accumulation is saturating, and duration conversions saturate at
  representable limits.
- The named pipe moved to `\\.\pipe\LOCAL\forgepulse-v1`, the namespace Windows permits
  packaged desktop clients to access. Its explicit descriptor supports authenticated,
  restricted, and packaged local tokens, has a low-integrity label for sandboxed desktop
  hosts, and Tokio still sets `PIPE_REJECT_REMOTE_CLIENTS`.
- IPC HMAC, version, timestamp, nonce/replay, rate, framing, size, and command-allowlist
  checks remain enabled. A bounded retry handles only transient Win32
  `ERROR_PIPE_BUSY`; all other errors remain failures.
- The Tauri commands call `AuthenticatedClient` for service status, live snapshots, and
  doctor reports. No placeholder metric source is present.

## Unsupported hardware metrics

The current collector supports total CPU, physical/commit memory, per-process CPU and
memory, process identity/path, and handle counts. It does not yet collect:

- GPU utilization, VRAM, engine counters, temperature, clocks, power, or fan speed;
- CPU per-core utilization, package temperature, clocks, voltage, or power/energy;
- disk throughput, latency, queue depth, SMART/health, or per-process I/O;
- network throughput, interface counters, latency monitoring, or per-process network I/O;
- sensor inventory, hardware/driver change collection, or process thread counts.

Protected or rapidly exiting processes can deny path/memory access to an unelevated
collector. That condition is reported as `permission_denied` capability degradation and
is never converted into a false zero.

## Remaining limitations

- The manual service gate covered current-user console mode, not installation through
  the Windows Service Control Manager or administrator-only behavior.
- A privileged production service still needs an installer-created service-owned state
  directory, explicit key/database file ACLs, and a user-to-service key provisioning
  flow. The pipe ACL itself is now explicit and HMAC authentication remains mandatory.
- The Tauri debug executable requires its configured Vite `devUrl`; the production
  frontend bundle itself passed `npm.cmd run build`.
- Windows Graphics Capture in this Codex desktop session returned
  `SetIsBorderRequired ... No such interface supported`; UI validation therefore used
  the Windows accessibility tree and live text rather than screenshots.
- Phase 2 collectors and features listed under unsupported metrics were intentionally
  not started.

None of these limitations invalidates the requested Phase 1 console-mode foundation
gate. They are explicit boundaries for later installer/privilege and collector phases.

## Verification-driven files changed

- `Cargo.toml`
- `Cargo.lock`
- `crates/forge-core/src/config.rs`
- `crates/forge-core/src/ipc.rs`
- `crates/forge-core/src/lib.rs`
- `crates/forge-security/Cargo.toml`
- `crates/forge-security/src/authentication.rs`
- `crates/forge-security/src/client.rs`
- `crates/forge-service/Cargo.toml`
- `crates/forge-service/src/lib.rs`
- `crates/forge-windows/src/collector.rs`
- `forge-ui/src-tauri/icons/icon-source.png`
- `forge-ui/src-tauri/icons/icon.ico`
- `forge-ui/src-tauri/icons/32x32.png`
- `forge-ui/src-tauri/icons/128x128.png`
- `forge-ui/src-tauri/icons/128x128@2x.png`
- `docs/FOUNDATION_STATUS.md`
- `docs/IMPLEMENTATION_STATUS.md`

The repository was initially untracked, so `git diff` cannot distinguish earlier
foundation creation from this verification pass; the list above is the set changed as
part of completing this gate.
