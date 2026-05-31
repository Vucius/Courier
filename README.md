# Courier

Rust native, local-first desktop email client skeleton.

## Crates

- `courier-proto`: shared DTOs, ID types, engine commands, and engine events.
- `courier-domain`: pure domain model and business rules; no IO and no async.
- `courier-storage`: SQLite metadata, FTS migrations, raw MIME, and attachment storage roots.
- `courier-mime`: MIME body selection and attachment extraction boundary.
- `courier-render`: safe email render tree for native iced rendering.
- `courier-security`: HTML sanitizing, image/link policy, and log redaction helpers.
- `courier-provider`: Gmail, Outlook, JMAP, and generic IMAP capability descriptions.
- `courier-adapter`: IMAP/SMTP/JMAP/provider remote traits and adapter shells.
- `courier-sync`: per-account sync scheduler, operation queue, send queue, and retry policy shells.
- `courier-search`: search syntax parsing and FTS query boundary.
- `courier-app`: application services and engine command/event orchestration.
- `courier-ui`: iced three-column desktop UI shell.

## Run

```powershell
cargo run -p courier-ui
```

## Check

```powershell
cargo check
```
