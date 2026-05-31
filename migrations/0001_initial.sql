CREATE TABLE accounts (
    id TEXT PRIMARY KEY,
    email TEXT NOT NULL,
    provider TEXT NOT NULL,
    imap_host TEXT NOT NULL,
    smtp_host TEXT NOT NULL,
    auth_type TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE folders (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    name TEXT NOT NULL,
    role TEXT NOT NULL,
    uid_validity INTEGER,
    unread_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE threads (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    subject TEXT NOT NULL,
    last_message_ts INTEGER NOT NULL,
    unread_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    folder_id TEXT NOT NULL REFERENCES folders(id),
    thread_id TEXT REFERENCES threads(id),
    imap_uid INTEGER,
    rfc_message_id TEXT,
    sender TEXT NOT NULL,
    recipients TEXT NOT NULL,
    cc TEXT NOT NULL DEFAULT '[]',
    subject TEXT NOT NULL,
    sent_at INTEGER NOT NULL,
    flags TEXT NOT NULL DEFAULT '[]',
    snippet TEXT NOT NULL DEFAULT '',
    has_attachments INTEGER NOT NULL DEFAULT 0,
    raw_path TEXT
);

CREATE TABLE message_bodies (
    message_id TEXT PRIMARY KEY REFERENCES messages(id),
    content_type TEXT NOT NULL,
    body TEXT NOT NULL
);

CREATE TABLE attachments (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL REFERENCES messages(id),
    filename TEXT NOT NULL,
    mime TEXT NOT NULL,
    size INTEGER NOT NULL,
    blob_path TEXT NOT NULL
);

CREATE TABLE contacts (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    name TEXT NOT NULL,
    email TEXT NOT NULL,
    frequency INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE labels (
    id TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    name TEXT NOT NULL,
    color TEXT NOT NULL
);

CREATE TABLE message_labels (
    message_id TEXT NOT NULL REFERENCES messages(id),
    label_id TEXT NOT NULL REFERENCES labels(id),
    PRIMARY KEY (message_id, label_id)
);

CREATE TABLE tasks (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL,
    payload TEXT NOT NULL,
    run_at INTEGER NOT NULL,
    status TEXT NOT NULL
);

CREATE VIRTUAL TABLE messages_fts USING fts5(
    subject,
    sender,
    recipients,
    snippet,
    body,
    content='messages',
    content_rowid='rowid'
);
