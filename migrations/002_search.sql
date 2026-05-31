CREATE VIRTUAL TABLE IF NOT EXISTS message_search_fts USING fts5(
    message_id UNINDEXED,
    account_id UNINDEXED,
    mailbox_id UNINDEXED,
    subject,
    from_text,
    to_text,
    snippet,
    body
);
