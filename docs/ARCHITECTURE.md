# ForgePulse architecture

## Trust and process boundaries

ForgePulse uses three process roles. `forge-service` is the only process allowed to
collect continuously, persist system state, or eventually perform approved system
actions. `forge-ui` is an unprivileged Tauri shell. `forge-cli` is an unprivileged
automation client. UI web content never receives a privileged handle and cannot
invoke a system action directly.

Clients communicate over `\\.\\pipe\\forgepulse-v1` using bounded, length-prefixed
JSON frames. Every request carries a protocol version, UUID request ID, Unix
timestamp, random nonce, typed command, and HMAC-SHA256 signature. The service
checks version, timestamp skew, signature, nonce replay, frame size, and per-client
rate before dispatch. Commands are enums rather than arbitrary strings.

```text
 TypeScript UI -> Tauri command --\
                                  +-> signed named-pipe IPC -> service -> collectors
 CLI ----------------------------/                             |       -> analysis
                                                               +-------> SQLite
```

## Crate responsibilities

| Crate | Boundary |
| --- | --- |
| `forge-core` | IDs, metric/evidence models, configuration, protocol types, errors |
| `forge-monitor` | collector contract, adaptive cadence, overhead budget |
| `forge-events` | normalized event timeline and ordering |
| `forge-storage` | migrations, integrity checks, compressed metric chunks |
| `forge-analysis` | deterministic incident rules and evidence weighting |
| `forge-benchmark` | descriptive statistics and result classification |
| `forge-experiments` | experiment state machine and durable rollback plan |
| `forge-profiles` | executable profile validation and actions |
| `forge-automation` | typed trigger/condition/action rules and dry-run evaluation |
| `forge-network` | bounded network diagnostic definitions and summaries |
| `forge-hardware` | hardware identity/change models and capability state |
| `forge-windows` | documented Win32 API adapters; all FFI is isolated here |
| `forge-security` | HMAC signing, replay/rate checks, framed named-pipe client |
| `forge-export` | local report models and anonymization |
| `forge-service` | lifecycle, sampling loop, IPC authorization/dispatch |
| `forge-cli` | machine/human-readable local client |
| `forge-ui` | Tauri desktop shell and accessible TypeScript dashboard |

Dependencies point inward toward `forge-core`; collectors do not know about UI or
storage. This allows mock collectors to exercise the complete pipeline without
hardware dependencies.

## Sampling and storage flow

The Windows collector samples CPU tick deltas, `GlobalMemoryStatusEx`, and a bounded
process inventory from documented Win32 APIs. CPU percentages require two points;
the first sample correctly reports no delta-derived value. The adaptive controller
selects idle, normal, incident, gaming, or benchmark cadence while respecting the
configured minimum interval and overhead budget.

Samples are buffered into short, ordered batches, MessagePack encoded, then zstd
compressed into `metric_chunks`. Query dimensions and min/max timestamps remain
indexed columns. Event-like entities remain normalized rows. This avoids one SQLite
row per metric while retaining range-query and retention behavior.

## Failure and rollback invariants

Every future mutating action must follow: validate target identity -> capture current
state -> durably commit a pending rollback record -> apply an allowlisted operation
-> verify -> commit result. A rollback is idempotent and transitions through explicit
states. Startup recovery processes any `pending` or `restoring` record before new
mutations. PID plus process creation time prevents PID reuse attacks.

The current service exposes read-only commands only. Mutating protocol variants are
not present, so an incomplete action handler cannot accidentally become reachable.

## Capability degradation

Collectors report `available`, `permission_denied`, `unsupported`, or `failed`
per capability. Missing temperature, GPU, or privileged process data is never
converted to zero. Analysis rules require the capabilities their evidence depends
on and retain contradictory evidence.

