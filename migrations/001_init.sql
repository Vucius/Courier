CREATE TABLE IF NOT EXISTS accounts (
    id          TEXT PRIMARY KEY,
    email       TEXT NOT NULL,
    provider    TEXT NOT NULL,
    imap_host   TEXT NOT NULL,
    imap_port   INTEGER NOT NULL DEFAULT 993,
    smtp_host   TEXT NOT NULL,
    smtp_port   INTEGER NOT NULL DEFAULT 587,
    auth_type   TEXT NOT NULL,
    created_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS mailboxes (
    id              TEXT PRIMARY KEY,
    account_id      TEXT NOT NULL REFERENCES accounts(id),
    name            TEXT NOT NULL,
    role            TEXT NOT NULL,
    uid_validity    INTEGER,
    last_uid        INTEGER DEFAULT 0,
    highest_modseq  INTEGER,
    unread_count    INTEGER DEFAULT 0,
    total_count     INTEGER DEFAULT 0
);

CREATE TABLE IF NOT EXISTS threads (
    id                 TEXT PRIMARY KEY,
    account_id         TEXT NOT NULL REFERENCES accounts(id),
    provider_thread_id TEXT,
    subject            TEXT NOT NULL,
    last_message_ts    INTEGER NOT NULL,
    unread_count       INTEGER DEFAULT 0,
    message_count      INTEGER DEFAULT 0
);

CREATE TABLE IF NOT EXISTS messages (
    id                  TEXT PRIMARY KEY,
    account_id          TEXT NOT NULL REFERENCES accounts(id),
    thread_id           TEXT REFERENCES threads(id),
    message_id_header   TEXT,
    in_reply_to         TEXT,
    "references"        TEXT,
    "from"              TEXT NOT NULL,
    "to"                TEXT NOT NULL,
    cc                  TEXT,
    subject             TEXT NOT NULL,
    snippet             TEXT,
    timestamp           INTEGER NOT NULL,
    flags               INTEGER DEFAULT 0,
    has_attachments     INTEGER DEFAULT 0,
    raw_path            TEXT,
    conflict_state      TEXT DEFAULT 'none'
);

CREATE TABLE IF NOT EXISTS message_mailboxes (
    message_id TEXT NOT NULL REFERENCES messages(id),
    mailbox_id TEXT NOT NULL REFERENCES mailboxes(id),
    remote_uid INTEGER,
    PRIMARY KEY (message_id, mailbox_id)
);

CREATE INDEX IF NOT EXISTS idx_message_mailboxes_mailbox ON message_mailboxes(mailbox_id, remote_uid);
CREATE INDEX IF NOT EXISTS idx_messages_thread ON messages(thread_id, timestamp ASC);
CREATE INDEX IF NOT EXISTS idx_messages_msgid ON messages(message_id_header);

CREATE TABLE IF NOT EXISTS message_bodies (
    message_id      TEXT PRIMARY KEY REFERENCES messages(id),
    content_type    TEXT NOT NULL,
    body            TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS attachments (
    id          TEXT PRIMARY KEY,
    message_id  TEXT NOT NULL REFERENCES messages(id),
    filename    TEXT NOT NULL,
    mime_type   TEXT NOT NULL,
    size        INTEGER NOT NULL,
    blob_path   TEXT
);

CREATE TABLE IF NOT EXISTS contacts (
    id          TEXT PRIMARY KEY,
    account_id  TEXT NOT NULL REFERENCES accounts(id),
    name        TEXT,
    email       TEXT NOT NULL,
    frequency   INTEGER DEFAULT 0,
    UNIQUE(account_id, email)
);

CREATE TABLE IF NOT EXISTS labels (
    id          TEXT PRIMARY KEY,
    account_id  TEXT NOT NULL REFERENCES accounts(id),
    name        TEXT NOT NULL,
    color       TEXT
);

CREATE TABLE IF NOT EXISTS message_labels (
    message_id  TEXT NOT NULL REFERENCES messages(id),
    label_id    TEXT NOT NULL REFERENCES labels(id),
    PRIMARY KEY (message_id, label_id)
);

CREATE TABLE IF NOT EXISTS op_queue (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id  TEXT NOT NULL REFERENCES accounts(id),
    op_type     TEXT NOT NULL,
    payload     TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    status      TEXT DEFAULT 'pending',
    retry_count INTEGER DEFAULT 0,
    last_error  TEXT
);

CREATE TABLE IF NOT EXISTS tasks (
    id          TEXT PRIMARY KEY,
    task_type   TEXT NOT NULL,
    payload     TEXT NOT NULL,
    run_at      INTEGER NOT NULL,
    status      TEXT DEFAULT 'pending'
);
