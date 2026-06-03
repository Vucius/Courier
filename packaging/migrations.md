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
- Runtime compatibility gates currently add missing `accounts.enabled`, `attachments.content_id`, `attachments.inline`, `tasks.retry_count`, and `tasks.last_error` columns for older databases.
- `Storage::initialize_with_report` is the executable migration runner used by the app runtime. It applies SQL migrations, runs compatibility gates, and returns the migration names plus any ad-hoc compatibility columns added during startup.

## Upgrade Checklist

1. Back up `courier.db` before any non-additive migration.
2. Run all SQL migrations in lexical order.
3. Run compatibility gates for columns introduced during active development.
4. Recount mailbox/thread derived counters after mailbox membership changes.
5. Leave raw `.eml` and attachment blobs untouched unless their database rows were successfully migrated.

## Release Smoke

1. Start the release binary against an existing `.courier/courier.db`.
2. Confirm startup logs include the `storage migration runner completed` entry.
3. Confirm the migration report lists `001_init.sql`, `002_search.sql`, and any compatibility columns added for that database.
4. Open a message with attachments and verify existing attachment rows still load even when `content_id` and `inline` were added by compatibility gates.
