# Threat model

## Protected assets

- privileged action authority and rollback state;
- authenticity and ordering of IPC requests;
- integrity of collected evidence and configuration;
- user privacy and local runtime paths.

## Attacker assumptions

An unprivileged local process may copy process names, race PID reuse, connect to the
pipe, replay observed messages, tamper with user-writable configuration, create
symlinks, or send oversized/malformed frames. A compromised administrator or kernel
is outside the first-release boundary.

## Current controls

- a strict typed command allowlist containing read-only commands only;
- 1 MiB frame limits and structured deserialization;
- HMAC-SHA256 request signatures with constant-time verification;
- protocol version, timestamp window, UUID, and nonce replay checks;
- bounded replay cache and token-bucket request limiting;
- exclusive secret creation, canonical runtime paths, and no shell construction;
- strong process identity includes PID and creation time;
- SQLite foreign keys, integrity checks, and explicit rollback states;
- Win32 handles are least-rights and closed through RAII wrappers.

## Installation gate

The initial development service runs as the interactive user. Installing it as a
privileged Windows service is blocked until the installer creates service-owned
state and an explicit named-pipe/file ACL, then provisions client authentication
without exposing a writable shared key. This is an intentional privilege boundary,
not an implicit installer behavior.

## Permanently blocked actions

ForgePulse will not disable Windows security, Windows Update, firewall, BitLocker,
boot security, essential services, or anti-cheat; inject into third-party processes;
install unsigned drivers; clean the registry; modify BIOS/voltage; or delete unknown
files. The safety model represents these as `Blocked`, not as hidden settings.

