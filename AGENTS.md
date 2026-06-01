# AGENTS.md

Guidance for coding agents working in this repository.

## Project Scope

Courier is a Rust native, local-first desktop email client skeleton. The repository is a Cargo workspace under this directory. Keep changes scoped to this workspace unless the user explicitly asks to touch sibling projects.

## Workspace Layout

- `crates/courier-proto`: shared DTOs, ID types, engine commands, and engine events.
- `crates/courier-domain`: pure domain model and business rules. Keep this crate free of IO and async runtime concerns.
- `crates/courier-storage`: SQLite metadata, migrations, raw MIME, attachment storage roots, and FTS boundaries.
- `crates/courier-mime`: MIME parsing, body selection, and attachment extraction boundaries.
- `crates/courier-render`: safe email render tree for native rendering.
- `crates/courier-security`: sanitizing, remote-content policy, link policy, and log redaction.
- `crates/courier-provider`: provider capability descriptions.
- `crates/courier-adapter`: IMAP/SMTP/JMAP/provider remote traits and adapter shells.
- `crates/courier-sync`: sync scheduler, operation queue, send queue, and retry policy shells.
- `crates/courier-search`: search syntax parsing and FTS query boundaries.
- `crates/courier-app`: application services and engine command/event orchestration.
- `crates/courier-ui`: Iced desktop UI shell.

## Common Commands

Run checks from the project root:

```powershell
cargo check
```

For UI-only changes:

```powershell
cargo check -p courier-ui
```

Run the desktop shell:

```powershell
cargo run -p courier-ui
```

Format before finishing code edits:

```powershell
cargo fmt
```

## Engineering Rules

- Prefer existing workspace crates and local boundaries over adding new dependencies.
- Keep DTO and command changes in `courier-proto` deliberate, because they affect the app, sync, storage, and UI layers.
- Keep IO out of `courier-domain`.
- Keep provider/network details out of UI crates.
- Use typed Rust APIs and structured data. Avoid ad hoc string parsing when a typed boundary exists.
- Do not introduce blocking network or storage work directly into Iced view functions.
- Keep demo data isolated in UI or app bootstrap code until real storage/provider data is wired in.

## Current Implementation State

The project is beyond a static shell. Preserve and extend these paths instead of reintroducing hard-coded UI state:

- `courier-storage` initializes SQLite, stores accounts/mailboxes/threads/messages/bodies, maintains FTS rows, and can load thread bodies.
- `courier-storage` can import raw RFC822 through `courier-mime`, persist the raw `.eml`, write attachment blobs/metadata, and expose attachment summaries.
- `courier-storage` supports mailbox-scoped thread listing and search. `None` means unified inbox; `Some(MailboxId)` means the explicit mailbox.
- `courier-storage` writes local user actions into `op_queue`. Mark-read and move/archive/trash must update local rows first, then enqueue an op.
- `courier-sync::SyncScheduler::sync_now` converts pending local ops into `courier-adapter::RemoteOp`, applies them through a `MailRemote`, marks ops done after adapter success, then pulls remote mailbox deltas into local storage.
- `courier-sync::SyncScheduler::send_draft` loads saved drafts from storage, calls `MailRemote::send_message`, persists a local Sent copy, and marks the draft task done after adapter success.
- `courier-adapter::NoopRemote` is the current local adapter used to exercise remote writeback and remote-delta ingestion without network access.
- `courier-app` seeds demo data, dispatches engine commands into storage/search/sync, and broadcasts `EngineEvent`s from storage-backed state.
- `courier-ui` subscribes to engine events. Do not repopulate UI demo data directly in the UI layer.
- `courier-ui` should display thread snapshots from `EngineEvent::ThreadsUpdated` directly. Search and mailbox filtering belong in app/storage, not in a second UI-only filter.
- `courier-ui` renders selected message bodies through `courier-render`; HTML content must be sanitized by `courier-security` before it is shown.
- `courier-ui` has a reusable component baseline modeled after Mailspring-style mail UI primitives: action bars, pane surfaces, outline/list rows, badges, avatars, form rows, notices, status pills, attachment chips, search, and empty states.
- `courier-mime` has a dependency-light RFC822/MIME parser covering header unfolding, multipart boundaries, text/html body selection, base64, quoted-printable, RFC5987 filenames, and attachment extraction.

## Local-First Operation Contract

For user actions that mutate mail:

- Update SQLite first so the UI can refresh immediately from local state.
- Insert an `op_queue` row containing a structured JSON payload for remote writeback.
- Publish a fresh mailbox/thread snapshot from `courier-app`.
- Let `courier-sync` consume or retry pending ops. Remote adapters must mark ops `done` only after successful server writeback.

Current local ops:

- `mark_read`: updates `messages.flags`, recounts unread state, enqueues `mark_read`.
- `move`: moves the message to the target mailbox, sets archive/trash flags when applicable, enqueues `move`.

`courier-sync` owns queued-op to remote-op translation:

- `mark_read` payload maps to `RemoteOp::MarkRead`.
- `move` payload maps to `RemoteOp::Move`.
- Unsupported op types should fail visibly instead of being silently acknowledged.

## Adapter Contract

- Implement `courier-adapter::MailRemote` for real IMAP/Gmail/Outlook/JMAP backends.
- Use `NoopRemote` only for local smoke tests and non-network skeleton behavior.
- `apply_ops` is the writeback seam for local-first operations.
- `fetch_delta` is the inbound-sync seam. Return typed `RemoteMessage` values and a new `SyncCursor`; `courier-sync` owns persistence into SQLite.
- `send_message` is the outbound-send seam. `courier-sync` owns draft loading, sent-copy persistence, and task status transitions.
- Real adapters should return errors when server writeback fails; `courier-sync` will keep the op pending and increment retry metadata.
- Real adapters should only advance the returned cursor to a point that is safe to persist after all included messages are stored.
- `SyncReport.mailbox_updates` is the app-facing list of inbound messages that were actually persisted. Use it for `EngineEvent::NewMessages`; do not synthesize fake sync-refresh mail in `courier-app`.

## Draft And Send Contract

- UI should save a `DraftMessage` before issuing `EngineCommand::SendMessage`.
- `courier-app` should route `SendMessage` through `SyncScheduler::send_draft`; do not emit a successful `SendResult` without adapter acknowledgement.
- After successful send, `courier-storage` persists the sent copy under the account's Sent mailbox and marks the draft task `done`.
- Failed sends should mark the draft task `failed` and surface the error through `EngineEvent::SendResult`.

## Rendering And Security Contract

- Do not display raw HTML bodies directly in UI views.
- `courier-security` owns HTML sanitization, remote image blocking, active-content stripping, and log redaction helpers.
- `courier-render` owns conversion from sanitized message content into a native render tree.
- UI views should compose `RenderTree` nodes into Iced widgets and keep parsing out of view functions.
- Remote images stay blocked unless a future explicit per-message or trusted-sender policy is added.

## Remaining Work Snapshot

Major project areas still open:

- Real account setup and identity management UI.
- Real IMAP/SMTP, Gmail, Outlook, and JMAP adapter implementations.
- Production-grade MIME parsing for edge cases beyond the current built-in parser, including richer charsets, nested message/rfc822 parts, inline CID resolution, and malformed server payload recovery.
- Attachment preview/open policy, download lifecycle, and reader integration for stored attachments.
- Robust send queue retry/backoff beyond the current single `send_draft` path.
- Incremental sync cursors per provider, mailbox discovery reconciliation, deletions, and conflict handling.
- Account settings, trusted sender policy, and per-message remote image allow controls.
- More complete reader rendering for tables, quoted replies, inline images, and link-click confirmation.
- Packaging, app icons, release profile, and installer/runtime data migration strategy.

## Thread And Mailbox Semantics

- Unified Inbox is represented by `mailbox_id: None` in app/storage APIs.
- Specific mailbox views use the concrete `MailboxId`.
- Archive and Trash should not appear in Unified Inbox after the local move.
- FTS search should filter through current `message_mailboxes`; do not rely on stale mailbox IDs stored in FTS rows.

## UI Conventions

The UI is built with `iced`.

- `crates/courier-ui/src/app.rs` owns top-level state, messages, update logic, and layout composition.
- `crates/courier-ui/src/views/` contains domain-specific views such as mailbox list, thread list, reader, and composer.
- `crates/courier-ui/src/components/` contains reusable UI primitives such as surfaces, action bars, search, status, and empty states.
- `crates/courier-ui/src/theme.rs` contains shared layout constants and colors.

When adding UI:

- Put reusable styling and layout wrappers in `components/`.
- Keep view modules thin and focused on composing data into UI.
- Prefer the existing components before adding view-local row/header/input styling.
- Use `components/list.rs` for mailbox/thread row patterns, `components/form.rs` for composer fields, `components/notice.rs` for inline warnings/status, and `components/attachment.rs` for attachment/image placeholders.
- Add new `Message` variants in `app.rs` only when an interaction needs app-level state or side effects.
- Match the three-column mail client pattern already present: mailbox sidebar, thread list, reader/composer pane.
- Prefer compact, utilitarian controls suitable for repeated desktop email workflows.

## Verification Expectations

- Run `cargo fmt` after Rust edits.
- Run `cargo check -p <crate>` for targeted changes.
- Run full `cargo check` when touching shared crates, workspace dependencies, protocol types, storage, sync, or app orchestration.
- If a check cannot be run, report the reason and the residual risk.

## Git And File Safety

- Do not revert user changes unless explicitly requested.
- Avoid broad refactors unless required for the task.
- Keep generated or local runtime data out of commits, including `.courier/` and `target/`.
