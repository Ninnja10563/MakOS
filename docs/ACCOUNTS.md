# Accounts and sessions

MakOS AArch64 uses a real persistent account database rather than a hard-coded
login comparison.

## Storage and password handling

- Kernel-owned database: hidden VFS object `/home/user/.accounts`, persisted by
  MakFS dynamic-file transactions.
- Format: bounded version-1 records, SHA-256 integrity tag, at most eight users.
- Passwords: per-account salt plus PBKDF2-HMAC-SHA256 with 100,000 iterations.
  Records contain username, unique UID/GID, iteration count, salt, and hash;
  never plaintext.
- The initial `marcus` account migrates without changing its existing password.
  New accounts receive UIDs/GIDs from 2000 upward, avoiding built-in service
  identities.
- The account codec rejects corruption, duplicate names/IDs, invalid next-ID
  state, malformed usernames, weak passwords, and capacity overflow before
  changing live state.

## User flow

At an authenticated terminal:

```text
adduser alice
New password: ********
Retype password: ********
user alice created (uid=2000, gid=2000)
```

Username rules: 1-31 bytes; first character lowercase ASCII letter; remaining
characters lowercase letters, digits, `_`, or `-`. Passwords are 8-64 bytes.
The shell collects passwords without echoing plaintext, sends a bounded private
request, then wipes password, confirmation, and request buffers. Passwords are
not command-history or serial-log content.

`signout` synchronously terminates all non-shell session apps, including Browser,
and runs normal resource cleanup for surfaces, sockets, VFS/TTY handles, VM
regions, and address spaces. PID 1 surfaces/files close, every per-process
credential binding is cleared, terminal/input state resets, and the retained
login form is redrawn. The same PID 1 shell loops back to login, so another user
can authenticate without rebooting.

Prompts, `whoami`, and `pwd` read the selected session identity. Each session has
a generation; stale process credential bindings cannot survive sign-out.

## Verification

```sh
cargo test -p makos-accounts
cargo check -p makos-kernel --target aarch64-unknown-none
cargo check -p makos-kernel --target x86_64-unknown-none
```

Deterministic tests cover username policy, the legacy account, correct/wrong and
unknown-user authentication, unique UID/GID allocation, unique salts, duplicate
and capacity errors, byte-exact encoding, persistence round trips, integrity
failure, and absence of plaintext passwords in encoded records.
