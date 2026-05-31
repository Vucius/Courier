use std::path::{Path, PathBuf};

use courier_domain::SyncCursor;
use courier_proto::{
    AccountId, AccountSummary, AuthType, DraftId, DraftMessage, MailboxId, MailboxRole,
    MailboxSummary, MessageBody, MessageId, ProviderKind, ThreadId, ThreadSummary,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::json;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone)]
pub struct Storage {
    data_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedOp {
    pub id: i64,
    pub account_id: AccountId,
    pub op_type: String,
    pub payload: String,
    pub status: String,
    pub retry_count: u32,
    pub last_error: Option<String>,
}

impl Storage {
    pub fn open(data_dir: impl Into<PathBuf>) -> Result<Self> {
        let data_dir = data_dir.into();
        std::fs::create_dir_all(data_dir.join("attachments"))?;
        std::fs::create_dir_all(data_dir.join("raw"))?;

        Ok(Self { data_dir })
    }

    pub fn initialize(&self) -> Result<()> {
        let db_path = self.db_path();
        let connection = rusqlite::Connection::open(&db_path)?;
        connection.execute_batch(include_str!("../../../migrations/001_init.sql"))?;
        connection.execute_batch(include_str!("../../../migrations/002_search.sql"))?;

        tracing::info!(path = %db_path.display(), "courier storage initialized");
        Ok(())
    }

    pub fn upsert_account(&self, account: &AccountSummary) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            r#"
            INSERT INTO accounts (
                id, email, provider, imap_host, imap_port, smtp_host, smtp_port, auth_type, created_at
            )
            VALUES (?1, ?2, ?3, '', 993, '', 587, ?4, unixepoch())
            ON CONFLICT(id) DO UPDATE SET
                email = excluded.email,
                provider = excluded.provider
            "#,
            params![
                account.id.0,
                account.email,
                provider_to_str(&account.provider),
                auth_type_to_str(&AuthType::Password),
            ],
        )?;
        Ok(())
    }

    pub fn upsert_mailbox(&self, mailbox: &MailboxSummary) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            r#"
            INSERT INTO mailboxes (
                id, account_id, name, role, unread_count, total_count
            )
            VALUES (?1, ?2, ?3, ?4, ?5, 0)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                role = excluded.role,
                unread_count = excluded.unread_count
            "#,
            params![
                mailbox.id.0,
                mailbox.account_id.0,
                mailbox.name,
                mailbox_role_to_str(&mailbox.role),
                mailbox.unread_count,
            ],
        )?;
        Ok(())
    }

    pub fn list_mailboxes(&self) -> Result<Vec<MailboxSummary>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            r#"
            SELECT id, account_id, name, role, unread_count
            FROM mailboxes
            ORDER BY
                CASE role
                    WHEN 'inbox' THEN 0
                    WHEN 'drafts' THEN 1
                    WHEN 'sent' THEN 2
                    WHEN 'archive' THEN 3
                    WHEN 'spam' THEN 4
                    WHEN 'trash' THEN 5
                    ELSE 6
                END,
                name COLLATE NOCASE
            "#,
        )?;

        let rows = statement.query_map([], |row| {
            let role: String = row.get(3)?;
            Ok(MailboxSummary {
                id: MailboxId(row.get(0)?),
                account_id: AccountId(row.get(1)?),
                name: row.get(2)?,
                role: mailbox_role_from_str(&role),
                unread_count: row.get::<_, i64>(4)?.max(0) as u32,
            })
        })?;

        collect_rows(rows)
    }

    pub fn mailbox_sync_cursor(&self, mailbox_id: &MailboxId) -> Result<SyncCursor> {
        let connection = self.connection()?;
        let cursor = connection
            .query_row(
                r#"
                SELECT uid_validity, last_uid, highest_modseq
                FROM mailboxes
                WHERE id = ?1
                "#,
                params![mailbox_id.0],
                |row| {
                    let uid_validity = row.get::<_, Option<i64>>(0)?.unwrap_or_default();
                    let last_uid = row.get::<_, Option<i64>>(1)?.unwrap_or_default();
                    let highest_modseq = row.get::<_, Option<i64>>(2)?;
                    Ok(SyncCursor {
                        uid_validity: non_negative_u32(uid_validity),
                        last_uid: non_negative_u32(last_uid),
                        highest_modseq: highest_modseq.map(non_negative_u64),
                    })
                },
            )
            .optional()?;

        Ok(cursor.unwrap_or_default())
    }

    pub fn update_mailbox_sync_cursor(
        &self,
        mailbox_id: &MailboxId,
        cursor: &SyncCursor,
    ) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            r#"
            UPDATE mailboxes
            SET uid_validity = ?2,
                last_uid = ?3,
                highest_modseq = ?4
            WHERE id = ?1
            "#,
            params![
                mailbox_id.0,
                i64::from(cursor.uid_validity),
                i64::from(cursor.last_uid),
                cursor.highest_modseq.map(u64_to_i64),
            ],
        )?;
        Ok(())
    }

    pub fn upsert_thread(&self, thread: &ThreadSummary) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            r#"
            INSERT INTO threads (
                id, account_id, subject, last_message_ts, unread_count, message_count
            )
            VALUES (?1, ?2, ?3, ?4, ?5, 1)
            ON CONFLICT(id) DO UPDATE SET
                subject = excluded.subject,
                last_message_ts = MAX(threads.last_message_ts, excluded.last_message_ts),
                unread_count = excluded.unread_count
            "#,
            params![
                thread.id.0,
                thread.account_id.0,
                thread.subject,
                thread.last_message_ts,
                if thread.unread { 1 } else { 0 },
            ],
        )?;
        Ok(())
    }

    pub fn upsert_message(
        &self,
        mailbox_id: &MailboxId,
        thread: &ThreadSummary,
        body: &MessageBody,
    ) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let to = body.to.join(", ");
        let flags = if thread.unread { 0 } else { 1 };

        transaction.execute(
            r#"
            INSERT INTO messages (
                id, account_id, thread_id, message_id_header, "references", "from", "to",
                subject, snippet, timestamp, flags, has_attachments, conflict_state
            )
            VALUES (?1, ?2, ?3, ?4, '', ?5, ?6, ?7, ?8, ?9, ?10, 0, 'none')
            ON CONFLICT(id) DO UPDATE SET
                thread_id = excluded.thread_id,
                "from" = excluded."from",
                "to" = excluded."to",
                subject = excluded.subject,
                snippet = excluded.snippet,
                timestamp = excluded.timestamp,
                flags = excluded.flags
            "#,
            params![
                body.id.0,
                thread.account_id.0,
                thread.id.0,
                body.id.0,
                body.from,
                to,
                body.subject,
                thread.snippet,
                thread.last_message_ts,
                flags,
            ],
        )?;

        transaction.execute(
            r#"
            INSERT INTO message_bodies (message_id, content_type, body)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(message_id) DO UPDATE SET
                content_type = excluded.content_type,
                body = excluded.body
            "#,
            params![body.id.0, body.content_type, body.body],
        )?;

        transaction.execute(
            r#"
            INSERT OR IGNORE INTO message_mailboxes (message_id, mailbox_id, remote_uid)
            VALUES (?1, ?2, NULL)
            "#,
            params![body.id.0, mailbox_id.0],
        )?;

        transaction.execute(
            "DELETE FROM message_search_fts WHERE message_id = ?1",
            params![body.id.0],
        )?;

        transaction.execute(
            r#"
            INSERT INTO message_search_fts (
                message_id, account_id, mailbox_id, subject, from_text, to_text, snippet, body
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                body.id.0,
                thread.account_id.0,
                mailbox_id.0,
                body.subject,
                body.from,
                to,
                thread.snippet,
                body.body,
            ],
        )?;

        transaction.commit()?;
        self.recount_threads()?;
        Ok(())
    }

    pub fn list_threads(&self) -> Result<Vec<ThreadSummary>> {
        self.list_threads_for_mailbox(None)
    }

    pub fn list_threads_for_mailbox(
        &self,
        mailbox_id: Option<&MailboxId>,
    ) -> Result<Vec<ThreadSummary>> {
        let connection = self.connection()?;

        match mailbox_id {
            Some(mailbox_id) => {
                let mut statement = connection.prepare(THREAD_SUMMARY_FOR_MAILBOX_QUERY)?;
                let rows = statement.query_map(params![&mailbox_id.0], thread_summary_from_row)?;
                collect_rows(rows)
            }
            None => {
                let mut statement = connection.prepare(THREAD_SUMMARY_FOR_INBOX_QUERY)?;
                let rows = statement.query_map([], thread_summary_from_row)?;
                collect_rows(rows)
            }
        }
    }

    pub fn search_threads(&self, query: &str) -> Result<Vec<ThreadSummary>> {
        self.search_threads_for_mailbox(query, None)
    }

    pub fn search_threads_for_mailbox(
        &self,
        query: &str,
        mailbox_id: Option<&MailboxId>,
    ) -> Result<Vec<ThreadSummary>> {
        let query = build_fts_query(query);
        if query.is_empty() {
            return self.list_threads_for_mailbox(mailbox_id);
        }

        let connection = self.connection()?;
        match mailbox_id {
            Some(mailbox_id) => {
                let mut statement = connection.prepare(SEARCH_FOR_MAILBOX_QUERY)?;
                let rows =
                    statement.query_map(params![&query, &mailbox_id.0], thread_summary_from_row)?;
                collect_rows(rows)
            }
            None => {
                let mut statement = connection.prepare(SEARCH_FOR_INBOX_QUERY)?;
                let rows = statement.query_map(params![&query], thread_summary_from_row)?;
                collect_rows(rows)
            }
        }
    }

    pub fn load_message_for_thread(&self, thread_id: &ThreadId) -> Result<Option<MessageBody>> {
        let connection = self.connection()?;
        connection
            .query_row(
                r#"
                SELECT
                    m.id,
                    m.thread_id,
                    m.subject,
                    m."from",
                    m."to",
                    b.content_type,
                    b.body
                FROM messages m
                JOIN message_bodies b ON b.message_id = m.id
                WHERE m.thread_id = ?1
                ORDER BY m.timestamp DESC, m.id DESC
                LIMIT 1
                "#,
                params![thread_id.0],
                |row| {
                    let to: String = row.get(4)?;
                    Ok(MessageBody {
                        id: MessageId(row.get(0)?),
                        thread_id: ThreadId(row.get(1)?),
                        subject: row.get(2)?,
                        from: row.get(3)?,
                        to: split_recipients(&to),
                        content_type: row.get(5)?,
                        body: row.get(6)?,
                    })
                },
            )
            .optional()
            .map_err(Error::from)
    }

    pub fn mark_message_read(&self, message_id: &MessageId, read: bool) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let account_id = message_account_id(&transaction, message_id)?;
        let read_flag = if read { 1 } else { 0 };
        transaction.execute(
            r#"
            UPDATE messages
            SET flags = CASE
                    WHEN ?2 = 1 THEN flags | 1
                    ELSE flags & ~1
                END,
                conflict_state = 'local_pending'
            WHERE id = ?1
            "#,
            params![message_id.0, read_flag],
        )?;

        queue_op(
            &transaction,
            &account_id,
            "mark_read",
            json!({
                "message_id": message_id.0,
                "read": read,
            }),
        )?;

        transaction.commit()?;
        self.recount_threads()
    }

    pub fn move_message_to_mailbox_role(
        &self,
        message_id: &MessageId,
        role: MailboxRole,
    ) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let account_id = message_account_id(&transaction, message_id)?;
        let mailbox_id = ensure_mailbox_for_role(&transaction, &account_id, &role)?;
        let flag_mask = match role {
            MailboxRole::Archive => FLAG_ARCHIVED,
            MailboxRole::Trash => FLAG_DELETED,
            _ => 0,
        };

        transaction.execute(
            r#"
            UPDATE messages
            SET flags = flags | ?2,
                conflict_state = 'local_pending'
            WHERE id = ?1
            "#,
            params![message_id.0, flag_mask],
        )?;

        transaction.execute(
            "DELETE FROM message_mailboxes WHERE message_id = ?1",
            params![message_id.0],
        )?;

        transaction.execute(
            r#"
            INSERT INTO message_mailboxes (message_id, mailbox_id, remote_uid)
            VALUES (?1, ?2, NULL)
            "#,
            params![message_id.0, mailbox_id.0],
        )?;

        queue_op(
            &transaction,
            &account_id,
            "move",
            json!({
                "message_id": message_id.0,
                "target_mailbox_id": mailbox_id.0,
                "target_role": mailbox_role_to_str(&role),
            }),
        )?;

        transaction.commit()?;
        self.recount_threads()
    }

    pub fn pending_ops(&self) -> Result<Vec<QueuedOp>> {
        self.pending_ops_for_account(None)
    }

    pub fn pending_ops_for_account(&self, account_id: Option<&AccountId>) -> Result<Vec<QueuedOp>> {
        let connection = self.connection()?;
        match account_id {
            Some(account_id) => {
                let mut statement = connection.prepare(PENDING_OPS_FOR_ACCOUNT_QUERY)?;
                let rows = statement.query_map(params![&account_id.0], queued_op_from_row)?;
                collect_rows(rows)
            }
            None => {
                let mut statement = connection.prepare(PENDING_OPS_QUERY)?;
                let rows = statement.query_map([], queued_op_from_row)?;
                collect_rows(rows)
            }
        }
    }

    pub fn mark_op_completed(&self, op_id: i64) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "UPDATE op_queue SET status = 'done', last_error = NULL WHERE id = ?1",
            params![op_id],
        )?;
        Ok(())
    }

    pub fn mark_op_failed(&self, op_id: i64, error: &str) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            r#"
            UPDATE op_queue
            SET status = 'pending',
                retry_count = retry_count + 1,
                last_error = ?2
            WHERE id = ?1
            "#,
            params![op_id, error],
        )?;
        Ok(())
    }

    pub fn save_draft(&self, draft: &DraftMessage) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            r#"
            INSERT INTO tasks (id, task_type, payload, run_at, status)
            VALUES (?1, 'draft', ?2, unixepoch(), 'pending')
            ON CONFLICT(id) DO UPDATE SET
                payload = excluded.payload,
                run_at = excluded.run_at,
                status = excluded.status
            "#,
            params![draft.id.0, draft_payload(draft)?],
        )?;
        Ok(())
    }

    pub fn load_draft(&self, draft_id: &DraftId) -> Result<Option<DraftMessage>> {
        let connection = self.connection()?;
        let payload = connection
            .query_row(
                r#"
                SELECT payload
                FROM tasks
                WHERE id = ?1
                  AND task_type = 'draft'
                "#,
                params![draft_id.0],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        payload
            .map(|payload| serde_json::from_str(&payload))
            .transpose()
            .map_err(Error::from)
    }

    pub fn draft_status(&self, draft_id: &DraftId) -> Result<Option<String>> {
        let connection = self.connection()?;
        connection
            .query_row(
                r#"
                SELECT status
                FROM tasks
                WHERE id = ?1
                  AND task_type = 'draft'
                "#,
                params![draft_id.0],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(Error::from)
    }

    pub fn mark_draft_sending(&self, draft_id: &DraftId) -> Result<()> {
        self.mark_draft_status(draft_id, "sending")
    }

    pub fn mark_draft_sent(&self, draft_id: &DraftId) -> Result<()> {
        self.mark_draft_status(draft_id, "done")
    }

    pub fn mark_draft_failed(&self, draft_id: &DraftId) -> Result<()> {
        self.mark_draft_status(draft_id, "failed")
    }

    pub fn persist_sent_draft(
        &self,
        draft: &DraftMessage,
        remote_id: Option<&str>,
    ) -> Result<MessageId> {
        tracing::debug!(
            draft_id = %draft.id.0,
            remote_id,
            "persisting sent draft copy"
        );

        let connection = self.connection()?;
        let from = account_email(&connection, &draft.account_id)?;
        let sent_mailbox_id =
            ensure_mailbox_for_role(&connection, &draft.account_id, &MailboxRole::Sent)?;
        drop(connection);

        let subject = draft_subject(&draft.subject);
        let snippet = draft_snippet(&draft.body);
        let timestamp = unix_timestamp();
        let message_id = MessageId(format!("sent:{}", draft.id.0));
        let thread = ThreadSummary {
            id: ThreadId(format!("thread:sent:{}", draft.id.0)),
            account_id: draft.account_id.clone(),
            subject: subject.clone(),
            sender: from.clone(),
            snippet,
            unread: false,
            last_message_ts: timestamp,
        };
        let body = MessageBody {
            id: message_id.clone(),
            thread_id: thread.id.clone(),
            subject,
            from,
            to: draft.to.clone(),
            content_type: "text/plain".to_string(),
            body: draft.body.clone(),
        };

        self.upsert_thread(&thread)?;
        self.upsert_message(&sent_mailbox_id, &thread, &body)?;

        Ok(message_id)
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("courier.db")
    }

    fn connection(&self) -> Result<Connection> {
        Ok(Connection::open(self.db_path())?)
    }

    fn recount_threads(&self) -> Result<()> {
        let connection = self.connection()?;
        connection.execute_batch(
            r#"
            UPDATE threads
            SET unread_count = (
                SELECT COUNT(*)
                FROM messages
                WHERE messages.thread_id = threads.id
                  AND (messages.flags & 1) = 0
            ),
            message_count = (
                SELECT COUNT(*)
                FROM messages
                WHERE messages.thread_id = threads.id
            );
            UPDATE mailboxes
            SET unread_count = (
                SELECT COUNT(*)
                FROM message_mailboxes mm
                JOIN messages m ON m.id = mm.message_id
                WHERE mm.mailbox_id = mailboxes.id
                  AND (m.flags & 1) = 0
            ),
            total_count = (
                SELECT COUNT(*)
                FROM message_mailboxes mm
                WHERE mm.mailbox_id = mailboxes.id
            );
            "#,
        )?;
        Ok(())
    }

    fn mark_draft_status(&self, draft_id: &DraftId, status: &str) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            r#"
            UPDATE tasks
            SET status = ?2
            WHERE id = ?1
              AND task_type = 'draft'
            "#,
            params![draft_id.0, status],
        )?;
        Ok(())
    }
}

const THREAD_SUMMARY_FOR_INBOX_QUERY: &str = r#"
SELECT
    t.id,
    t.account_id,
    t.subject,
    COALESCE(m."from", ''),
    COALESCE(m.snippet, ''),
    CASE WHEN t.unread_count > 0 THEN 1 ELSE 0 END,
    t.last_message_ts
FROM threads t
LEFT JOIN messages m ON m.id = (
    SELECT latest.id
    FROM messages latest
    JOIN message_mailboxes latest_mm ON latest_mm.message_id = latest.id
    JOIN mailboxes latest_mb ON latest_mb.id = latest_mm.mailbox_id
    WHERE latest.thread_id = t.id
      AND latest_mb.role = 'inbox'
      AND (latest.flags & 4) = 0
    ORDER BY latest.timestamp DESC, latest.id DESC
    LIMIT 1
)
WHERE m.id IS NOT NULL
ORDER BY t.last_message_ts DESC
"#;

const THREAD_SUMMARY_FOR_MAILBOX_QUERY: &str = r#"
SELECT
    t.id,
    t.account_id,
    t.subject,
    COALESCE(m."from", ''),
    COALESCE(m.snippet, ''),
    CASE WHEN t.unread_count > 0 THEN 1 ELSE 0 END,
    t.last_message_ts
FROM threads t
LEFT JOIN messages m ON m.id = (
    SELECT latest.id
    FROM messages latest
    JOIN message_mailboxes latest_mm ON latest_mm.message_id = latest.id
    WHERE latest.thread_id = t.id
      AND latest_mm.mailbox_id = ?1
    ORDER BY latest.timestamp DESC, latest.id DESC
    LIMIT 1
)
WHERE m.id IS NOT NULL
ORDER BY t.last_message_ts DESC
"#;

const SEARCH_FOR_INBOX_QUERY: &str = r#"
SELECT DISTINCT
    t.id,
    t.account_id,
    t.subject,
    COALESCE(m."from", ''),
    COALESCE(m.snippet, ''),
    CASE WHEN t.unread_count > 0 THEN 1 ELSE 0 END,
    t.last_message_ts
FROM message_search_fts fts
JOIN messages m ON m.id = fts.message_id
JOIN threads t ON t.id = m.thread_id
JOIN message_mailboxes mm ON mm.message_id = m.id
JOIN mailboxes mb ON mb.id = mm.mailbox_id
WHERE message_search_fts MATCH ?1
  AND mb.role = 'inbox'
  AND (m.flags & 4) = 0
ORDER BY t.last_message_ts DESC
"#;

const SEARCH_FOR_MAILBOX_QUERY: &str = r#"
SELECT DISTINCT
    t.id,
    t.account_id,
    t.subject,
    COALESCE(m."from", ''),
    COALESCE(m.snippet, ''),
    CASE WHEN t.unread_count > 0 THEN 1 ELSE 0 END,
    t.last_message_ts
FROM message_search_fts fts
JOIN messages m ON m.id = fts.message_id
JOIN threads t ON t.id = m.thread_id
JOIN message_mailboxes mm ON mm.message_id = m.id
WHERE message_search_fts MATCH ?1
  AND mm.mailbox_id = ?2
ORDER BY t.last_message_ts DESC
"#;

const PENDING_OPS_QUERY: &str = r#"
SELECT id, account_id, op_type, payload, status, retry_count, last_error
FROM op_queue
WHERE status = 'pending'
ORDER BY id ASC
"#;

const PENDING_OPS_FOR_ACCOUNT_QUERY: &str = r#"
SELECT id, account_id, op_type, payload, status, retry_count, last_error
FROM op_queue
WHERE status = 'pending'
  AND account_id = ?1
ORDER BY id ASC
"#;

const FLAG_ARCHIVED: i64 = 2;
const FLAG_DELETED: i64 = 4;

fn thread_summary_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ThreadSummary> {
    Ok(ThreadSummary {
        id: ThreadId(row.get(0)?),
        account_id: AccountId(row.get(1)?),
        subject: row.get(2)?,
        sender: row.get(3)?,
        snippet: row.get(4)?,
        unread: row.get::<_, i64>(5)? != 0,
        last_message_ts: row.get(6)?,
    })
}

fn queued_op_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<QueuedOp> {
    Ok(QueuedOp {
        id: row.get(0)?,
        account_id: AccountId(row.get(1)?),
        op_type: row.get(2)?,
        payload: row.get(3)?,
        status: row.get(4)?,
        retry_count: row.get::<_, i64>(5)?.max(0) as u32,
        last_error: row.get(6)?,
    })
}

fn collect_rows<T>(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>>,
) -> Result<Vec<T>> {
    let mut values = Vec::new();
    for row in rows {
        values.push(row?);
    }
    Ok(values)
}

fn provider_to_str(provider: &ProviderKind) -> &'static str {
    match provider {
        ProviderKind::GenericImap => "generic_imap",
        ProviderKind::Gmail => "gmail",
        ProviderKind::Outlook => "outlook",
        ProviderKind::Jmap => "jmap",
    }
}

fn auth_type_to_str(auth_type: &AuthType) -> &'static str {
    match auth_type {
        AuthType::Password => "password",
        AuthType::OAuth2 => "oauth2",
    }
}

fn mailbox_role_to_str(role: &MailboxRole) -> &'static str {
    match role {
        MailboxRole::Inbox => "inbox",
        MailboxRole::Sent => "sent",
        MailboxRole::Drafts => "drafts",
        MailboxRole::Archive => "archive",
        MailboxRole::Trash => "trash",
        MailboxRole::Spam => "spam",
        MailboxRole::Custom => "custom",
    }
}

fn mailbox_role_from_str(role: &str) -> MailboxRole {
    match role {
        "inbox" => MailboxRole::Inbox,
        "sent" => MailboxRole::Sent,
        "drafts" => MailboxRole::Drafts,
        "archive" => MailboxRole::Archive,
        "trash" => MailboxRole::Trash,
        "spam" => MailboxRole::Spam,
        _ => MailboxRole::Custom,
    }
}

fn mailbox_role_label(role: &MailboxRole) -> &'static str {
    match role {
        MailboxRole::Inbox => "Inbox",
        MailboxRole::Sent => "Sent",
        MailboxRole::Drafts => "Drafts",
        MailboxRole::Archive => "Archive",
        MailboxRole::Trash => "Trash",
        MailboxRole::Spam => "Spam",
        MailboxRole::Custom => "Custom",
    }
}

fn split_recipients(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|recipient| !recipient.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn draft_subject(subject: &str) -> String {
    let subject = subject.trim();
    if subject.is_empty() {
        "(no subject)".to_string()
    } else {
        subject.to_string()
    }
}

fn draft_snippet(body: &str) -> String {
    let snippet = body
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default()
        .trim();

    if snippet.chars().count() > 140 {
        format!("{}...", snippet.chars().take(140).collect::<String>())
    } else {
        snippet.to_string()
    }
}

fn unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
        .min(i64::MAX as u64) as i64
}

fn non_negative_u32(value: i64) -> u32 {
    value.max(0).min(i64::from(u32::MAX)) as u32
}

fn non_negative_u64(value: i64) -> u64 {
    value.max(0) as u64
}

fn u64_to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn message_account_id(connection: &Connection, message_id: &MessageId) -> Result<AccountId> {
    let account_id = connection.query_row(
        "SELECT account_id FROM messages WHERE id = ?1",
        params![message_id.0],
        |row| row.get::<_, String>(0),
    )?;
    Ok(AccountId(account_id))
}

fn account_email(connection: &Connection, account_id: &AccountId) -> Result<String> {
    connection
        .query_row(
            "SELECT email FROM accounts WHERE id = ?1",
            params![account_id.0],
            |row| row.get::<_, String>(0),
        )
        .map_err(Error::from)
}

fn ensure_mailbox_for_role(
    connection: &Connection,
    account_id: &AccountId,
    role: &MailboxRole,
) -> Result<MailboxId> {
    let role_name = mailbox_role_to_str(role);
    let existing = connection
        .query_row(
            "SELECT id FROM mailboxes WHERE account_id = ?1 AND role = ?2 LIMIT 1",
            params![account_id.0, role_name],
            |row| row.get::<_, String>(0),
        )
        .optional()?;

    if let Some(id) = existing {
        return Ok(MailboxId(id));
    }

    let id = MailboxId(format!("{}:{role_name}", account_id.0));
    connection.execute(
        r#"
        INSERT INTO mailboxes (id, account_id, name, role, unread_count, total_count)
        VALUES (?1, ?2, ?3, ?4, 0, 0)
        "#,
        params![&id.0, &account_id.0, mailbox_role_label(role), role_name],
    )?;
    Ok(id)
}

fn queue_op(
    connection: &Connection,
    account_id: &AccountId,
    op_type: &str,
    payload: serde_json::Value,
) -> Result<i64> {
    connection.execute(
        r#"
        INSERT INTO op_queue (account_id, op_type, payload, created_at, status)
        VALUES (?1, ?2, ?3, unixepoch(), 'pending')
        "#,
        params![account_id.0, op_type, serde_json::to_string(&payload)?],
    )?;
    Ok(connection.last_insert_rowid())
}

fn build_fts_query(query: &str) -> String {
    query
        .split_whitespace()
        .filter_map(|token| {
            let token = token
                .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '@' && ch != '.');
            if token.is_empty() {
                None
            } else {
                Some(format!("\"{}\"", token.replace('"', "\"\"")))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn draft_payload(draft: &DraftMessage) -> Result<String> {
    Ok(serde_json::to_string(draft)?)
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn storage_roundtrip_and_search() {
        let data_dir = test_data_dir("roundtrip");
        let storage = Storage::open(&data_dir).expect("open storage");
        storage.initialize().expect("initialize storage");

        let account = AccountSummary {
            id: AccountId("account:test".to_string()),
            email: "tester@example.test".to_string(),
            provider: ProviderKind::GenericImap,
        };
        let mailbox = MailboxSummary {
            id: MailboxId("account:test:inbox".to_string()),
            account_id: account.id.clone(),
            name: "Inbox".to_string(),
            role: MailboxRole::Inbox,
            unread_count: 1,
        };
        let thread = ThreadSummary {
            id: ThreadId("thread:test".to_string()),
            account_id: account.id.clone(),
            subject: "Roadmap review".to_string(),
            sender: "planner@example.test".to_string(),
            snippet: "Storage search should find this thread.".to_string(),
            unread: true,
            last_message_ts: 42,
        };
        let body = MessageBody {
            id: MessageId("message:test".to_string()),
            thread_id: thread.id.clone(),
            subject: thread.subject.clone(),
            from: thread.sender.clone(),
            to: vec!["tester@example.test".to_string()],
            content_type: "text/plain".to_string(),
            body: "The local storage roadmap is searchable.".to_string(),
        };

        storage.upsert_account(&account).expect("upsert account");
        storage.upsert_mailbox(&mailbox).expect("upsert mailbox");
        storage.upsert_thread(&thread).expect("upsert thread");
        storage
            .upsert_message(&mailbox.id, &thread, &body)
            .expect("upsert message");

        let mailboxes = storage.list_mailboxes().expect("list mailboxes");
        assert_eq!(mailboxes.len(), 1);
        assert_eq!(mailboxes[0].name, "Inbox");
        assert_eq!(mailboxes[0].unread_count, 1);

        let threads = storage.list_threads().expect("list threads");
        assert_eq!(threads.len(), 1);
        assert!(threads[0].unread);

        let search = storage.search_threads("roadmap").expect("search threads");
        assert_eq!(search.len(), 1);
        assert_eq!(search[0].id, thread.id);

        let loaded = storage
            .load_message_for_thread(&thread.id)
            .expect("load message")
            .expect("message body exists");
        assert_eq!(loaded.body, body.body);

        storage
            .mark_message_read(&body.id, true)
            .expect("mark message read");
        let threads = storage.list_threads().expect("list threads after read");
        assert!(!threads[0].unread);
        let mailboxes = storage.list_mailboxes().expect("list mailboxes after read");
        assert_eq!(mailboxes[0].unread_count, 0);
        let pending = storage.pending_ops().expect("pending ops after read");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].op_type, "mark_read");

        storage
            .move_message_to_mailbox_role(&body.id, MailboxRole::Archive)
            .expect("archive message");
        let threads = storage.list_threads().expect("list threads after archive");
        assert!(threads.is_empty());
        let archive_mailbox = MailboxId("account:test:archive".to_string());
        let archived_threads = storage
            .list_threads_for_mailbox(Some(&archive_mailbox))
            .expect("list archive threads");
        assert_eq!(archived_threads.len(), 1);
        let archived_search = storage
            .search_threads_for_mailbox("roadmap", Some(&archive_mailbox))
            .expect("search archive threads");
        assert_eq!(archived_search.len(), 1);
        let pending = storage.pending_ops().expect("pending ops after archive");
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[1].op_type, "move");
        storage
            .mark_op_completed(pending[0].id)
            .expect("complete first op");
        let pending = storage.pending_ops().expect("pending ops after completion");
        assert_eq!(pending.len(), 1);

        std::fs::remove_dir_all(data_dir).expect("remove test data");
    }

    #[test]
    fn draft_lifecycle_and_sent_copy() {
        let data_dir = test_data_dir("draft-send");
        let storage = Storage::open(&data_dir).expect("open storage");
        storage.initialize().expect("initialize storage");

        let account = AccountSummary {
            id: AccountId("account:draft".to_string()),
            email: "sender@example.test".to_string(),
            provider: ProviderKind::GenericImap,
        };
        let draft = DraftMessage {
            id: DraftId("draft:one".to_string()),
            account_id: account.id.clone(),
            to: vec!["receiver@example.test".to_string()],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "Draft send".to_string(),
            body: "The send queue should persist a sent copy.".to_string(),
            attachments: Vec::new(),
        };

        storage.upsert_account(&account).expect("upsert account");
        storage.save_draft(&draft).expect("save draft");

        let loaded = storage
            .load_draft(&draft.id)
            .expect("load draft")
            .expect("draft exists");
        assert_eq!(loaded.subject, draft.subject);
        assert_eq!(
            storage.draft_status(&draft.id).expect("draft status"),
            Some("pending".to_string())
        );

        storage
            .mark_draft_sending(&draft.id)
            .expect("mark draft sending");
        assert_eq!(
            storage.draft_status(&draft.id).expect("draft status"),
            Some("sending".to_string())
        );

        let message_id = storage
            .persist_sent_draft(&draft, Some("remote:one"))
            .expect("persist sent draft");
        storage.mark_draft_sent(&draft.id).expect("mark draft sent");

        assert_eq!(message_id, MessageId("sent:draft:one".to_string()));
        assert_eq!(
            storage.draft_status(&draft.id).expect("draft status"),
            Some("done".to_string())
        );

        let sent_mailbox = MailboxId("account:draft:sent".to_string());
        let sent_threads = storage
            .list_threads_for_mailbox(Some(&sent_mailbox))
            .expect("list sent threads");
        assert_eq!(sent_threads.len(), 1);
        assert_eq!(sent_threads[0].subject, "Draft send");
        assert!(!sent_threads[0].unread);

        let sent_body = storage
            .load_message_for_thread(&sent_threads[0].id)
            .expect("load sent body")
            .expect("sent body exists");
        assert_eq!(sent_body.from, "sender@example.test");
        assert_eq!(sent_body.to, vec!["receiver@example.test".to_string()]);

        std::fs::remove_dir_all(data_dir).expect("remove test data");
    }

    fn test_data_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();

        std::env::current_dir()
            .expect("current dir")
            .join("target")
            .join("courier-storage-tests")
            .join(format!("{name}-{nonce}"))
    }
}
