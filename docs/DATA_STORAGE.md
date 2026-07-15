# Data storage

ForgePulse stores runtime files beneath `%LOCALAPPDATA%\\ForgePulse` by default.
`forgepulse.db` uses SQLite WAL mode, foreign keys, a busy timeout, and migrations
embedded in `forge-storage`.

High-frequency metric samples are grouped into bounded chunks. Each chunk has an
indexed time range, sample count, schema version, MessagePack payload, zstd codec,
and checksum. Process/service lifecycle events, incidents, evidence, sessions,
experiments, rollbacks, notes, and configuration changes remain relational records.

Retention deletes whole old chunks in small transactions and can later compact old
data into lower-resolution chunks. Database size limits are configuration values;
there is no unbounded collector or log queue.

The `doctor` path runs `PRAGMA quick_check`, reports pending rollback state, and
checks the IPC and collector capability states. Schema changes are forward-only,
transactional migrations. A backup is required before a destructive migration.

