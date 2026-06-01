# Courier Migration Strategy

Courier stores user data under the runtime data directory named `.courier`.
The SQLite database file is `courier.db`.

## Rules

- Migrations must be additive by default: create tables, add nullable columns, or add columns with safe defaults.
- Runtime initialization must tolerate existing databases by checking schema shape with `PRAGMA table_info` before issuing `ALTER TABLE`.
- Destructive data rewrites must be split into explicit backup, rewrite, and verification phases.
- Release builds must ship the migration notes and release manifest next to the binary.
- A future installer must not delete `.courier` during uninstall unless the user explicitly requests data removal.

## Current Schema

- `001_init.sql`: base accounts, mailboxes, messages, attachments, identities, labels, tasks, and operation queue.
- `002_search.sql`: FTS search table and mailbox-scoped query support.
- Runtime compatibility gates currently add missing `accounts.enabled`, `tasks.retry_count`, and `tasks.last_error` columns for older databases.

## Upgrade Checklist

1. Back up `courier.db` before any non-additive migration.
2. Run all SQL migrations in lexical order.
3. Run compatibility gates for columns introduced during active development.
4. Recount mailbox/thread derived counters after mailbox membership changes.
5. Leave raw `.eml` and attachment blobs untouched unless their database rows were successfully migrated.
