# MailSpring Rust

Rust workspace skeleton for a Mailspring-style email client.

## Crates

- `mailproto`: shared domain types, engine commands, and engine events.
- `mailcore`: UI-independent engine shell for sync, storage, IMAP, SMTP, and search.
- `mailspring-ui`: iced desktop UI shell with a three-column layout.

## Run

```powershell
cargo run -p mailspring-ui
```

## Check

```powershell
cargo check
```
