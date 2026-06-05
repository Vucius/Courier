# Courier

Courier is a Rust-native, local-first desktop email client. The current
closure target is a usable Generic IMAP/SMTP client with local SQLite state,
OS keyring credentials, real remote sync/send paths, and a compact iced UI.

## Current Runnable Path

The baseline supported path is a real Generic IMAP/SMTP account using password
auth:

- add an account in the desktop UI;
- save the password into the OS keyring;
- sync mailboxes and messages over IMAP TLS or STARTTLS;
- read sanitized message bodies from local SQLite;
- download, preview, and open stored attachments;
- send drafts over SMTP with Undo Send, retry, cancel, and local Sent copy;
- apply Mark read, Archive, Trash, and Move locally first, then write back
  through the sync queue.

Gmail, Outlook, and JMAP provider paths are wired into the same runtime
boundaries, but they may still need provider-specific account validation and
compatibility fixes. They are not the baseline acceptance path for the current
closure pass.

## Run

```powershell
cargo run -p courier-ui
```

Runtime data is stored in the local `.courier` data directory. Credentials are
stored through the OS keyring backend and are not written to SQLite.

## Closure Checks

Run these from the repository root:

```powershell
cargo fmt
cargo check
cargo clippy --workspace -- -D warnings
powershell -ExecutionPolicy Bypass -File packaging\verify-release-smoke.ps1
```

Real account login is intentionally a manual acceptance step. Use a Generic
IMAP/SMTP password account to verify account save, sync, read, attachment
download, send, retry/cancel, and local-first mail actions.

## Workspace Crates

- `courier-proto`: shared DTOs, ID types, engine commands, and engine events.
- `courier-domain`: pure domain model and business rules.
- `courier-storage`: SQLite metadata, migrations, raw MIME, FTS, and
  attachment storage.
- `courier-mime`: RFC822/MIME body selection and attachment extraction.
- `courier-render`: safe email render tree for native iced rendering.
- `courier-security`: HTML sanitizing, remote image blocking, link policy, and
  log redaction.
- `courier-credential`: OS keyring credential storage.
- `courier-provider`: provider capability and OAuth2 metadata.
- `courier-adapter`: IMAP/SMTP/JMAP/provider remote implementations.
- `courier-sync`: sync scheduler, local operation queue, send queue, and retry
  policy.
- `courier-search`: search syntax parsing and FTS query boundary.
- `courier-app`: application services and engine command/event orchestration.
- `courier-ui`: iced desktop UI.

## Non-Blocking Enhancements

These are future quality improvements, not current runnable blockers:

- full JMAP queryChanges/send/writeback semantics;
- provider-specific Gmail/Outlook labels, IDLE, expunge, and special-folder
  behavior;
- IMAP BODYSTRUCTURE part-level attachment fetch;
- native desktop notifications, tray, and background residency;
- native bitmap/PDF inline rendering;
- full installer artifact generation in CI;
- broader automated compatibility tests.
