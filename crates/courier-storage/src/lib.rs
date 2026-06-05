use std::collections::{HashSet, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use courier_domain::SyncCursor;
use courier_mime::{BodyKind, ParsedAttachment, parse_rfc822};
use courier_proto::{
    AccountConfig, AccountId, AccountState, AccountSummary, AttachmentId, AttachmentPreview,
    AttachmentPreviewKind, AttachmentSummary, AttachmentTransfer, AttachmentTransferStatus,
    AuthType, ConflictResolution, ConflictSummary, DraftId, DraftMessage, IdentityConfig,
    IdentityId, IdentitySummary, MailboxId, MailboxRole, MailboxSummary, MessageBody, MessageId,
    ProviderKind, SendQueueItem, TaskId, ThreadId, ThreadSummary,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::json;

pub type Result<T> = std::result::Result<T, Error>;

const UNDO_SEND_SECONDS: i64 = 5;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("mime error: {0}")]
    Mime(#[from] courier_mime::Error),
}

#[derive(Debug, Clone)]
pub struct Storage {
    data_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationReport {
    pub db_path: PathBuf,
    pub sql_migrations: Vec<String>,
    pub compatibility_steps: Vec<String>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredAttachment {
    pub id: AttachmentId,
    pub filename: String,
    pub mime_type: String,
    pub size: u64,
    pub blob_path: Option<PathBuf>,
    pub content_id: Option<String>,
    pub inline: bool,
}

impl From<StoredAttachment> for AttachmentSummary {
    fn from(attachment: StoredAttachment) -> Self {
        Self {
            id: attachment.id,
            filename: attachment.filename,
            mime_type: attachment.mime_type,
            size: attachment.size,
            blob_path: attachment
                .blob_path
                .map(|path| path.to_string_lossy().into_owned()),
            content_id: attachment.content_id,
            inline: attachment.inline,
        }
    }
}

impl Storage {
    pub fn open(data_dir: impl Into<PathBuf>) -> Result<Self> {
        let data_dir = data_dir.into();
        std::fs::create_dir_all(data_dir.join("attachments"))?;
        std::fs::create_dir_all(data_dir.join("raw"))?;

        Ok(Self { data_dir })
    }

    pub fn initialize(&self) -> Result<()> {
        self.initialize_with_report().map(|_| ())
    }

    pub fn initialize_with_report(&self) -> Result<MigrationReport> {
        let db_path = self.db_path();
        let connection = rusqlite::Connection::open(&db_path)?;
        connection.execute_batch(include_str!("../../../migrations/001_init.sql"))?;
        connection.execute_batch(include_str!("../../../migrations/002_search.sql"))?;
        let mut compatibility_steps = Vec::new();
        if ensure_accounts_enabled_column(&connection)? {
            compatibility_steps.push("accounts.enabled".to_string());
        }
        compatibility_steps.extend(ensure_attachment_metadata_columns(&connection)?);
        compatibility_steps.extend(ensure_tasks_retry_columns(&connection)?);

        let report = MigrationReport {
            db_path,
            sql_migrations: vec!["001_init.sql".to_string(), "002_search.sql".to_string()],
            compatibility_steps,
        };

        tracing::info!(
            path = %report.db_path.display(),
            migrations = ?report.sql_migrations,
            compatibility_steps = ?report.compatibility_steps,
            "courier storage initialized"
        );
        Ok(report)
    }

    pub fn upsert_account(&self, account: &AccountSummary) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            r#"
            INSERT INTO accounts (
                id, email, provider, imap_host, imap_port, smtp_host, smtp_port, auth_type, enabled, created_at
            )
            VALUES (?1, ?2, ?3, '', 993, '', 587, ?4, 1, unixepoch())
            ON CONFLICT(id) DO UPDATE SET
                email = excluded.email,
                provider = excluded.provider,
                enabled = 1
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

    pub fn upsert_account_config(&self, account: &AccountConfig) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            r#"
            INSERT INTO accounts (
                id, email, provider, imap_host, imap_port, smtp_host, smtp_port, auth_type, enabled, created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, unixepoch())
            ON CONFLICT(id) DO UPDATE SET
                email = excluded.email,
                provider = excluded.provider,
                imap_host = excluded.imap_host,
                imap_port = excluded.imap_port,
                smtp_host = excluded.smtp_host,
                smtp_port = excluded.smtp_port,
                auth_type = excluded.auth_type
            "#,
            params![
                account.id.0,
                account.email,
                provider_to_str(&account.provider),
                account.imap_host,
                i64::from(account.imap_port),
                account.smtp_host,
                i64::from(account.smtp_port),
                auth_type_to_str(&account.auth_type),
            ],
        )?;
        self.ensure_standard_mailboxes(&account.id)
    }

    pub fn upsert_identity(&self, identity: &IdentityConfig) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            r#"
            INSERT INTO identities (id, account_id, name, email, reply_to)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(id) DO UPDATE SET
                account_id = excluded.account_id,
                name = excluded.name,
                email = excluded.email,
                reply_to = excluded.reply_to
            "#,
            params![
                identity.id.0,
                identity.account_id.0,
                identity.name,
                identity.email,
                identity.reply_to,
            ],
        )?;
        Ok(())
    }

    pub fn list_identities(&self) -> Result<Vec<IdentitySummary>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            r#"
            SELECT id, account_id, name, email, reply_to
            FROM identities
            ORDER BY account_id, name COLLATE NOCASE, email COLLATE NOCASE, id
            "#,
        )?;
        let rows = statement.query_map([], |row| {
            Ok(IdentitySummary {
                id: IdentityId(row.get(0)?),
                account_id: AccountId(row.get(1)?),
                name: row.get(2)?,
                email: row.get(3)?,
                reply_to: row.get(4)?,
            })
        })?;

        collect_rows(rows)
    }

    pub fn delete_identity(&self, identity_id: &IdentityId) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "DELETE FROM identities WHERE id = ?1",
            params![identity_id.0],
        )?;
        Ok(())
    }

    pub fn list_accounts(&self) -> Result<Vec<AccountState>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            r#"
            SELECT id, email, provider, imap_host, imap_port, smtp_host, smtp_port, auth_type, enabled
            FROM accounts
            ORDER BY email COLLATE NOCASE, id
            "#,
        )?;
        let rows = statement.query_map([], |row| {
            let provider: String = row.get(2)?;
            let auth_type: String = row.get(7)?;
            Ok(AccountState {
                id: AccountId(row.get(0)?),
                email: row.get(1)?,
                provider: provider_from_str(&provider),
                imap_host: row.get(3)?,
                imap_port: non_negative_u32(row.get::<_, i64>(4)?).min(u16::MAX.into()) as u16,
                smtp_host: row.get(5)?,
                smtp_port: non_negative_u32(row.get::<_, i64>(6)?).min(u16::MAX.into()) as u16,
                auth_type: auth_type_from_str(&auth_type),
                enabled: row.get::<_, i64>(8)? != 0,
            })
        })?;

        collect_rows(rows)
    }

    pub fn set_account_enabled(&self, account_id: &AccountId, enabled: bool) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "UPDATE accounts SET enabled = ?2 WHERE id = ?1",
            params![account_id.0, if enabled { 1 } else { 0 }],
        )?;
        Ok(())
    }

    pub fn delete_account(&self, account_id: &AccountId) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;

        transaction.execute(
            "DELETE FROM message_search_fts WHERE account_id = ?1",
            params![account_id.0],
        )?;
        transaction.execute(
            r#"
            DELETE FROM message_labels
            WHERE message_id IN (
                SELECT id FROM messages WHERE account_id = ?1
            )
            "#,
            params![account_id.0],
        )?;
        transaction.execute(
            r#"
            DELETE FROM attachments
            WHERE message_id IN (
                SELECT id FROM messages WHERE account_id = ?1
            )
            "#,
            params![account_id.0],
        )?;
        transaction.execute(
            r#"
            DELETE FROM message_bodies
            WHERE message_id IN (
                SELECT id FROM messages WHERE account_id = ?1
            )
            "#,
            params![account_id.0],
        )?;
        transaction.execute(
            r#"
            DELETE FROM message_mailboxes
            WHERE message_id IN (
                SELECT id FROM messages WHERE account_id = ?1
            )
            "#,
            params![account_id.0],
        )?;
        transaction.execute(
            "DELETE FROM messages WHERE account_id = ?1",
            params![account_id.0],
        )?;
        transaction.execute(
            "DELETE FROM threads WHERE account_id = ?1",
            params![account_id.0],
        )?;
        transaction.execute(
            "DELETE FROM contacts WHERE account_id = ?1",
            params![account_id.0],
        )?;
        transaction.execute(
            "DELETE FROM identities WHERE account_id = ?1",
            params![account_id.0],
        )?;
        transaction.execute(
            "DELETE FROM message_labels WHERE label_id IN (SELECT id FROM labels WHERE account_id = ?1)",
            params![account_id.0],
        )?;
        transaction.execute(
            "DELETE FROM labels WHERE account_id = ?1",
            params![account_id.0],
        )?;
        transaction.execute(
            "DELETE FROM op_queue WHERE account_id = ?1",
            params![account_id.0],
        )?;
        transaction.execute(
            r#"
            DELETE FROM tasks
            WHERE payload LIKE ?1
            "#,
            params![format!("%{}%", account_id.0)],
        )?;
        transaction.execute(
            "DELETE FROM mailboxes WHERE account_id = ?1",
            params![account_id.0],
        )?;
        transaction.execute("DELETE FROM accounts WHERE id = ?1", params![account_id.0])?;

        transaction.commit()?;
        Ok(())
    }

    pub fn ensure_standard_mailboxes(&self, account_id: &AccountId) -> Result<()> {
        let connection = self.connection()?;
        for role in [
            MailboxRole::Inbox,
            MailboxRole::Sent,
            MailboxRole::Drafts,
            MailboxRole::Archive,
            MailboxRole::Trash,
        ] {
            ensure_mailbox_for_role(&connection, account_id, &role)?;
        }
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
            SELECT mb.id, mb.account_id, mb.name, mb.role, mb.unread_count
            FROM mailboxes mb
            JOIN accounts a ON a.id = mb.account_id
            WHERE a.enabled = 1
            ORDER BY
                CASE mb.role
                    WHEN 'inbox' THEN 0
                    WHEN 'drafts' THEN 1
                    WHEN 'sent' THEN 2
                    WHEN 'archive' THEN 3
                    WHEN 'spam' THEN 4
                    WHEN 'trash' THEN 5
                    ELSE 6
                END,
                mb.name COLLATE NOCASE
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

    pub fn reset_mailbox_for_uidvalidity_change(
        &self,
        mailbox_id: &MailboxId,
    ) -> Result<Vec<MessageId>> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let conflicted = local_pending_message_ids_for_mailbox(&transaction, mailbox_id)?;

        for message_id in &conflicted {
            transaction.execute(
                "UPDATE messages SET conflict_state = 'conflicted' WHERE id = ?1",
                params![message_id.0],
            )?;
        }

        transaction.execute(
            r#"
            DELETE FROM message_search_fts
            WHERE mailbox_id = ?1
              AND message_id NOT IN (
                  SELECT m.id
                  FROM messages m
                  WHERE m.conflict_state = 'conflicted'
              )
            "#,
            params![mailbox_id.0],
        )?;
        transaction.execute(
            r#"
            DELETE FROM message_mailboxes
            WHERE mailbox_id = ?1
              AND message_id NOT IN (
                  SELECT m.id
                  FROM messages m
                  WHERE m.conflict_state = 'conflicted'
              )
            "#,
            params![mailbox_id.0],
        )?;
        transaction.execute(
            r#"
            UPDATE mailboxes
            SET last_uid = 0,
                highest_modseq = NULL
            WHERE id = ?1
            "#,
            params![mailbox_id.0],
        )?;

        transaction.commit()?;
        self.recount_threads()?;
        Ok(conflicted)
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

    pub fn import_raw_message(
        &self,
        account_id: &AccountId,
        mailbox_id: &MailboxId,
        raw: &[u8],
    ) -> Result<MessageId> {
        let parsed = parse_rfc822(raw)?;
        let message_id = MessageId(message_id_from_raw(
            parsed.headers.message_id.as_deref(),
            raw,
        ));
        let thread = ThreadSummary {
            id: ThreadId(format!("thread:{}", message_id.0)),
            account_id: account_id.clone(),
            subject: draft_subject(&parsed.headers.subject),
            sender: parsed.headers.from.clone(),
            snippet: draft_snippet(&parsed.body.content),
            unread: true,
            last_message_ts: unix_timestamp(),
        };
        let body = MessageBody {
            id: message_id.clone(),
            thread_id: thread.id.clone(),
            subject: thread.subject.clone(),
            from: thread.sender.clone(),
            to: parsed.headers.to,
            content_type: match parsed.body.kind {
                BodyKind::PlainText => "text/plain".to_string(),
                BodyKind::Html => "text/html".to_string(),
            },
            body: parsed.body.content,
            attachments: Vec::new(),
        };

        self.upsert_thread(&thread)?;
        self.upsert_message(mailbox_id, &thread, &body)?;
        let raw_path = self.persist_raw_message_blob(&message_id, raw)?;
        self.persist_message_attachments(&message_id, &parsed.attachments)?;
        self.update_message_blob_state(
            &message_id,
            Some(&raw_path),
            !parsed.attachments.is_empty(),
        )?;

        Ok(message_id)
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
        let mut body = connection
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
                        attachments: Vec::new(),
                    })
                },
            )
            .optional()
            .map_err(Error::from)?;

        if let Some(body) = body.as_mut() {
            body.attachments = self
                .list_attachments(&body.id)?
                .into_iter()
                .map(AttachmentSummary::from)
                .collect();
        }

        Ok(body)
    }

    pub fn message_conflict_state(&self, message_id: &MessageId) -> Result<Option<String>> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT conflict_state FROM messages WHERE id = ?1",
                params![message_id.0],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(Error::from)
    }

    pub fn mark_message_conflicted(&self, message_id: &MessageId) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "UPDATE messages SET conflict_state = 'conflicted' WHERE id = ?1",
            params![message_id.0],
        )?;
        Ok(())
    }

    pub fn resolve_conflict(
        &self,
        message_id: &MessageId,
        resolution: ConflictResolution,
    ) -> Result<()> {
        let connection = self.connection()?;
        let next_state = match resolution {
            ConflictResolution::KeepLocal | ConflictResolution::RequeueLocal => "local_pending",
            ConflictResolution::AcceptRemote => "none",
        };
        connection.execute(
            r#"
            UPDATE messages
            SET conflict_state = ?2
            WHERE id = ?1
              AND conflict_state = 'conflicted'
            "#,
            params![message_id.0, next_state],
        )?;
        Ok(())
    }

    pub fn list_conflicts(&self) -> Result<Vec<ConflictSummary>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            r#"
            SELECT m.id, m.thread_id, m.subject
            FROM messages m
            JOIN accounts a ON a.id = m.account_id
            WHERE m.conflict_state = 'conflicted'
              AND a.enabled = 1
            ORDER BY m.timestamp DESC, m.id DESC
            "#,
        )?;
        let rows = statement.query_map([], |row| {
            Ok(ConflictSummary {
                message_id: MessageId(row.get(0)?),
                thread_id: ThreadId(row.get(1)?),
                subject: row.get(2)?,
                reason: "Remote change conflicted with pending local work".to_string(),
            })
        })?;

        collect_rows(rows)
    }

    pub fn apply_remote_delete(
        &self,
        mailbox_id: &MailboxId,
        message_id: &MessageId,
    ) -> Result<bool> {
        if self
            .message_conflict_state(message_id)?
            .is_some_and(|state| state == "local_pending")
        {
            self.mark_message_conflicted(message_id)?;
            return Ok(false);
        }

        let connection = self.connection()?;
        connection.execute(
            "DELETE FROM message_mailboxes WHERE mailbox_id = ?1 AND message_id = ?2",
            params![mailbox_id.0, message_id.0],
        )?;
        connection.execute(
            "DELETE FROM message_search_fts WHERE mailbox_id = ?1 AND message_id = ?2",
            params![mailbox_id.0, message_id.0],
        )?;
        self.recount_threads()?;
        Ok(true)
    }

    pub fn apply_remote_move(
        &self,
        source_mailbox_id: &MailboxId,
        target_mailbox_id: &MailboxId,
        message_id: &MessageId,
    ) -> Result<bool> {
        if self
            .message_conflict_state(message_id)?
            .is_some_and(|state| state == "local_pending")
        {
            self.mark_message_conflicted(message_id)?;
            return Ok(false);
        }

        let connection = self.connection()?;
        connection.execute(
            "DELETE FROM message_mailboxes WHERE mailbox_id = ?1 AND message_id = ?2",
            params![source_mailbox_id.0, message_id.0],
        )?;
        connection.execute(
            r#"
            INSERT OR IGNORE INTO message_mailboxes (message_id, mailbox_id, remote_uid)
            VALUES (?1, ?2, NULL)
            "#,
            params![message_id.0, target_mailbox_id.0],
        )?;
        connection.execute(
            "UPDATE message_search_fts SET mailbox_id = ?2 WHERE mailbox_id = ?1 AND message_id = ?3",
            params![source_mailbox_id.0, target_mailbox_id.0, message_id.0],
        )?;
        self.recount_threads()?;
        Ok(true)
    }

    pub fn list_attachments(&self, message_id: &MessageId) -> Result<Vec<StoredAttachment>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            r#"
            SELECT id, filename, mime_type, size, blob_path, content_id, inline
            FROM attachments
            WHERE message_id = ?1
            ORDER BY filename COLLATE NOCASE, id
            "#,
        )?;
        let rows = statement.query_map(params![message_id.0], |row| {
            Ok(StoredAttachment {
                id: AttachmentId(row.get(0)?),
                filename: row.get(1)?,
                mime_type: row.get(2)?,
                size: row.get::<_, i64>(3)?.max(0) as u64,
                blob_path: row.get::<_, Option<String>>(4)?.map(PathBuf::from),
                content_id: row.get(5)?,
                inline: row.get::<_, i64>(6)? != 0,
            })
        })?;

        collect_rows(rows)
    }

    pub fn attachment_by_id(
        &self,
        attachment_id: &AttachmentId,
    ) -> Result<Option<StoredAttachment>> {
        let connection = self.connection()?;
        connection
            .query_row(
                r#"
                SELECT id, filename, mime_type, size, blob_path, content_id, inline
                FROM attachments
                WHERE id = ?1
                "#,
                params![attachment_id.0],
                |row| {
                    Ok(StoredAttachment {
                        id: AttachmentId(row.get(0)?),
                        filename: row.get(1)?,
                        mime_type: row.get(2)?,
                        size: row.get::<_, i64>(3)?.max(0) as u64,
                        blob_path: row.get::<_, Option<String>>(4)?.map(PathBuf::from),
                        content_id: row.get(5)?,
                        inline: row.get::<_, i64>(6)? != 0,
                    })
                },
            )
            .optional()
            .map_err(Error::from)
    }

    pub fn attachment_account_id(&self, attachment_id: &AttachmentId) -> Result<Option<AccountId>> {
        let connection = self.connection()?;
        connection
            .query_row(
                r#"
                SELECT m.account_id
                FROM attachments a
                JOIN messages m ON m.id = a.message_id
                WHERE a.id = ?1
                "#,
                params![attachment_id.0],
                |row| Ok(AccountId(row.get(0)?)),
            )
            .optional()
            .map_err(Error::from)
    }

    pub fn attachment_transfer(
        &self,
        attachment_id: &AttachmentId,
    ) -> Result<Option<AttachmentTransfer>> {
        self.attachment_by_id(attachment_id).map(|attachment| {
            attachment.map(|attachment| self.attachment_transfer_from(attachment))
        })
    }

    pub fn attachment_transfers_for_message(
        &self,
        message_id: &MessageId,
    ) -> Result<Vec<AttachmentTransfer>> {
        self.list_attachments(message_id).map(|attachments| {
            attachments
                .into_iter()
                .map(|attachment| self.attachment_transfer_from(attachment))
                .collect()
        })
    }

    pub fn attachment_preview(
        &self,
        attachment_id: &AttachmentId,
        max_text_bytes: u64,
    ) -> Result<Option<AttachmentPreview>> {
        let Some(attachment) = self.attachment_by_id(attachment_id)? else {
            return Ok(None);
        };
        let summary = AttachmentSummary::from(attachment.clone());
        let Some(relative_path) = attachment.blob_path.as_ref() else {
            return Ok(Some(AttachmentPreview {
                attachment: summary,
                kind: AttachmentPreviewKind::MissingBlob,
                content: None,
                path: None,
                message: "Attachment file is not available locally".to_string(),
            }));
        };

        let absolute_path = self.data_dir.join(relative_path);
        let path = Some(absolute_path.to_string_lossy().into_owned());
        let mime_type = attachment.mime_type.to_ascii_lowercase();

        if mime_type.starts_with("text/") && attachment.size <= max_text_bytes {
            let content = std::fs::read_to_string(&absolute_path)?;
            return Ok(Some(AttachmentPreview {
                attachment: summary,
                kind: AttachmentPreviewKind::Text,
                content: Some(content),
                path,
                message: "Text preview loaded".to_string(),
            }));
        }

        if mime_type.starts_with("image/") {
            return Ok(Some(AttachmentPreview {
                attachment: summary,
                kind: AttachmentPreviewKind::Image,
                content: None,
                path,
                message: "Image attachment is ready for inline preview".to_string(),
            }));
        }

        if mime_type == "application/pdf"
            || attachment.filename.to_ascii_lowercase().ends_with(".pdf")
        {
            return Ok(Some(AttachmentPreview {
                attachment: summary,
                kind: AttachmentPreviewKind::Pdf,
                content: None,
                path,
                message: format!(
                    "PDF preview ready: {}. Open with the system viewer for full reading.",
                    file_size_label(attachment.size)
                ),
            }));
        }

        Ok(Some(AttachmentPreview {
            attachment: summary,
            kind: AttachmentPreviewKind::Unsupported,
            content: None,
            path,
            message: "No inline preview is available for this attachment type".to_string(),
        }))
    }

    fn attachment_transfer_from(&self, attachment: StoredAttachment) -> AttachmentTransfer {
        let summary = AttachmentSummary::from(attachment.clone());
        match attachment.blob_path.as_ref() {
            Some(relative_path) => {
                let absolute_path = self.data_dir.join(relative_path);
                if absolute_path.exists() {
                    AttachmentTransfer {
                        attachment: summary,
                        status: AttachmentTransferStatus::Ready,
                        progress: 1.0,
                        message: "Available locally".to_string(),
                    }
                } else {
                    AttachmentTransfer {
                        attachment: summary,
                        status: AttachmentTransferStatus::Missing,
                        progress: 0.0,
                        message: "Attachment file is missing from local storage".to_string(),
                    }
                }
            }
            None => AttachmentTransfer {
                attachment: summary,
                status: AttachmentTransferStatus::Missing,
                progress: 0.0,
                message: "Attachment has not been downloaded".to_string(),
            },
        }
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
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let message_id = {
            let op_payload = transaction
                .query_row(
                    "SELECT payload FROM op_queue WHERE id = ?1",
                    params![op_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;

            op_payload
                .as_deref()
                .map(message_id_from_op_payload)
                .transpose()?
                .flatten()
        };

        transaction.execute(
            "UPDATE op_queue SET status = 'done', last_error = NULL WHERE id = ?1",
            params![op_id],
        )?;

        if let Some(message_id) = message_id
            && !has_pending_op_for_message(&transaction, &message_id, op_id)?
        {
            transaction.execute(
                r#"
                UPDATE messages
                SET conflict_state = 'none'
                WHERE id = ?1
                  AND conflict_state = 'local_pending'
                "#,
                params![message_id.0],
            )?;
        }

        transaction.commit()?;
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
        let run_at = unix_timestamp().saturating_add(UNDO_SEND_SECONDS);
        connection.execute(
            r#"
            INSERT INTO tasks (id, task_type, payload, run_at, status)
            VALUES (?1, 'draft', ?2, ?3, 'pending')
            ON CONFLICT(id) DO UPDATE SET
                payload = excluded.payload,
                run_at = excluded.run_at,
                status = excluded.status
            "#,
            params![draft.id.0, draft_payload(draft)?, run_at],
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

    pub fn draft_retry_count(&self, draft_id: &DraftId) -> Result<Option<u32>> {
        let connection = self.connection()?;
        connection
            .query_row(
                r#"
                SELECT retry_count
                FROM tasks
                WHERE id = ?1
                  AND task_type = 'draft'
                "#,
                params![draft_id.0],
                |row| Ok(row.get::<_, i64>(0)?.max(0) as u32),
            )
            .optional()
            .map_err(Error::from)
    }

    pub fn list_send_queue(&self) -> Result<Vec<SendQueueItem>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            r#"
            SELECT id, payload, status, retry_count, last_error, run_at
            FROM tasks
            WHERE task_type = 'draft'
              AND status IN ('pending', 'sending', 'failed', 'cancelled')
            ORDER BY
                CASE status
                    WHEN 'sending' THEN 0
                    WHEN 'pending' THEN 1
                    WHEN 'failed' THEN 2
                    ELSE 3
                END,
                run_at ASC,
                id ASC
            "#,
        )?;
        let rows = statement.query_map([], |row| {
            let id: String = row.get(0)?;
            let payload: String = row.get(1)?;
            let draft: DraftMessage = serde_json::from_str(&payload).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    1,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            Ok(SendQueueItem {
                task_id: TaskId(format!("send:{id}")),
                draft_id: DraftId(id),
                account_id: draft.account_id,
                to: draft.to,
                subject: draft.subject,
                status: row.get(2)?,
                retry_count: row.get::<_, i64>(3)?.max(0) as u32,
                last_error: row.get(4)?,
                run_at: row.get(5)?,
            })
        })?;

        collect_rows(rows)
    }

    pub fn due_draft_ids(&self, now: i64, limit: usize) -> Result<Vec<DraftId>> {
        let connection = self.connection()?;
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let enabled_accounts = enabled_account_ids(&connection)?;
        let mut statement = connection.prepare(
            r#"
            SELECT id, payload
            FROM tasks
            WHERE task_type = 'draft'
              AND status = 'pending'
              AND run_at <= ?1
            ORDER BY run_at ASC, id ASC
            LIMIT ?2
            "#,
        )?;
        let mut rows = statement.query(params![now, limit])?;
        let mut draft_ids = Vec::new();

        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let payload: String = row.get(1)?;
            let draft: DraftMessage = serde_json::from_str(&payload)?;
            if enabled_accounts.contains(&draft.account_id.0) {
                draft_ids.push(DraftId(id));
            }
        }

        Ok(draft_ids)
    }

    pub fn mark_draft_sending(&self, draft_id: &DraftId) -> Result<()> {
        self.mark_draft_status(draft_id, "sending")
    }

    pub fn mark_draft_sent(&self, draft_id: &DraftId) -> Result<()> {
        self.mark_draft_status(draft_id, "done")
    }

    pub fn mark_draft_retry(&self, draft_id: &DraftId, error: &str, run_at: i64) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            r#"
            UPDATE tasks
            SET status = 'pending',
                retry_count = retry_count + 1,
                last_error = ?2,
                run_at = ?3
            WHERE id = ?1
              AND task_type = 'draft'
            "#,
            params![draft_id.0, error, run_at],
        )?;
        Ok(())
    }

    pub fn mark_draft_failed(&self, draft_id: &DraftId, error: &str) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            r#"
            UPDATE tasks
            SET status = 'failed',
                retry_count = retry_count + 1,
                last_error = ?2
            WHERE id = ?1
              AND task_type = 'draft'
            "#,
            params![draft_id.0, error],
        )?;
        Ok(())
    }

    pub fn mark_draft_pending_now(&self, draft_id: &DraftId) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            r#"
            UPDATE tasks
            SET status = 'pending',
                run_at = unixepoch(),
                last_error = NULL
            WHERE id = ?1
              AND task_type = 'draft'
            "#,
            params![draft_id.0],
        )?;
        Ok(())
    }

    pub fn cancel_draft_send(&self, draft_id: &DraftId) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            r#"
            UPDATE tasks
            SET status = 'cancelled'
            WHERE id = ?1
              AND task_type = 'draft'
              AND status IN ('pending', 'failed')
            "#,
            params![draft_id.0],
        )?;
        Ok(())
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
            attachments: Vec::new(),
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

    fn persist_raw_message_blob(&self, message_id: &MessageId, raw: &[u8]) -> Result<PathBuf> {
        let relative =
            PathBuf::from("raw").join(format!("{}.eml", safe_path_segment(&message_id.0)));
        let absolute = self.data_dir.join(&relative);
        if let Some(parent) = absolute.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&absolute, raw)?;
        Ok(relative)
    }

    fn persist_message_attachments(
        &self,
        message_id: &MessageId,
        attachments: &[ParsedAttachment],
    ) -> Result<()> {
        let connection = self.connection()?;

        for attachment in attachments {
            let relative = PathBuf::from("attachments")
                .join(safe_path_segment(&message_id.0))
                .join(safe_path_segment(&attachment.id.0));
            let absolute = self.data_dir.join(&relative);
            if let Some(parent) = absolute.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&absolute, &attachment.data)?;

            connection.execute(
                r#"
                INSERT INTO attachments (
                    id, message_id, filename, mime_type, size, blob_path, content_id, inline
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                ON CONFLICT(id) DO UPDATE SET
                    filename = excluded.filename,
                    mime_type = excluded.mime_type,
                    size = excluded.size,
                    blob_path = excluded.blob_path,
                    content_id = excluded.content_id,
                    inline = excluded.inline
                "#,
                params![
                    attachment.id.0,
                    message_id.0,
                    attachment.filename,
                    attachment.mime_type,
                    u64_to_i64(attachment.size),
                    relative.to_string_lossy(),
                    attachment.content_id,
                    if attachment.inline { 1 } else { 0 },
                ],
            )?;
        }

        Ok(())
    }

    fn update_message_blob_state(
        &self,
        message_id: &MessageId,
        raw_path: Option<&Path>,
        has_attachments: bool,
    ) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            r#"
            UPDATE messages
            SET raw_path = ?2,
                has_attachments = ?3
            WHERE id = ?1
            "#,
            params![
                message_id.0,
                raw_path.map(|path| path.to_string_lossy().to_string()),
                if has_attachments { 1 } else { 0 },
            ],
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
JOIN accounts a ON a.id = t.account_id
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
  AND a.enabled = 1
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
JOIN accounts a ON a.id = t.account_id
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
  AND a.enabled = 1
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
JOIN accounts a ON a.id = t.account_id
JOIN message_mailboxes mm ON mm.message_id = m.id
JOIN mailboxes mb ON mb.id = mm.mailbox_id
WHERE message_search_fts MATCH ?1
  AND mb.role = 'inbox'
  AND (m.flags & 4) = 0
  AND a.enabled = 1
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
JOIN accounts a ON a.id = t.account_id
JOIN message_mailboxes mm ON mm.message_id = m.id
WHERE message_search_fts MATCH ?1
  AND mm.mailbox_id = ?2
  AND a.enabled = 1
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

fn provider_from_str(provider: &str) -> ProviderKind {
    match provider {
        "gmail" => ProviderKind::Gmail,
        "outlook" => ProviderKind::Outlook,
        "jmap" => ProviderKind::Jmap,
        _ => ProviderKind::GenericImap,
    }
}

fn auth_type_to_str(auth_type: &AuthType) -> &'static str {
    match auth_type {
        AuthType::Password => "password",
        AuthType::OAuth2 => "oauth2",
    }
}

fn auth_type_from_str(auth_type: &str) -> AuthType {
    match auth_type {
        "oauth2" => AuthType::OAuth2,
        _ => AuthType::Password,
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

fn message_id_from_raw(header_message_id: Option<&str>, raw: &[u8]) -> String {
    if let Some(message_id) = header_message_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return format!("message:{}", safe_identifier(message_id));
    }

    let mut hasher = DefaultHasher::new();
    raw.hash(&mut hasher);
    format!("message:raw:{:016x}", hasher.finish())
}

fn safe_identifier(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '@' | ':') {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn safe_path_segment(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn file_size_label(size: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    if size >= MB {
        format!("{:.1} MB", size as f64 / MB as f64)
    } else if size >= KB {
        format!("{:.1} KB", size as f64 / KB as f64)
    } else {
        format!("{size} B")
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

fn message_id_from_op_payload(payload: &str) -> Result<Option<MessageId>> {
    let value: serde_json::Value = serde_json::from_str(payload)?;
    Ok(value
        .get("message_id")
        .and_then(|value| value.as_str())
        .map(|value| MessageId(value.to_string())))
}

fn local_pending_message_ids_for_mailbox(
    connection: &Connection,
    mailbox_id: &MailboxId,
) -> Result<Vec<MessageId>> {
    let mut statement = connection.prepare(
        r#"
        SELECT m.id
        FROM messages m
        JOIN message_mailboxes mm ON mm.message_id = m.id
        WHERE mm.mailbox_id = ?1
          AND m.conflict_state = 'local_pending'
        ORDER BY m.timestamp DESC, m.id DESC
        "#,
    )?;
    let rows = statement.query_map(params![mailbox_id.0], |row| {
        Ok(MessageId(row.get::<_, String>(0)?))
    })?;

    collect_rows(rows)
}

fn has_pending_op_for_message(
    connection: &Connection,
    message_id: &MessageId,
    excluded_op_id: i64,
) -> Result<bool> {
    let mut statement = connection.prepare(
        r#"
        SELECT payload
        FROM op_queue
        WHERE status = 'pending'
          AND id <> ?1
        "#,
    )?;
    let rows = statement.query_map(params![excluded_op_id], |row| row.get::<_, String>(0))?;

    for row in rows {
        if message_id_from_op_payload(&row?)?.as_ref() == Some(message_id) {
            return Ok(true);
        }
    }

    Ok(false)
}

fn ensure_accounts_enabled_column(connection: &Connection) -> Result<bool> {
    if !table_has_column(connection, "accounts", "enabled")? {
        connection.execute(
            "ALTER TABLE accounts ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1",
            [],
        )?;
        return Ok(true);
    }
    Ok(false)
}

fn ensure_attachment_metadata_columns(connection: &Connection) -> Result<Vec<String>> {
    let mut added = Vec::new();
    if !table_has_column(connection, "attachments", "content_id")? {
        connection.execute("ALTER TABLE attachments ADD COLUMN content_id TEXT", [])?;
        added.push("attachments.content_id".to_string());
    }
    if !table_has_column(connection, "attachments", "inline")? {
        connection.execute(
            "ALTER TABLE attachments ADD COLUMN inline INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
        added.push("attachments.inline".to_string());
    }
    Ok(added)
}

fn ensure_tasks_retry_columns(connection: &Connection) -> Result<Vec<String>> {
    let mut added = Vec::new();
    if !table_has_column(connection, "tasks", "retry_count")? {
        connection.execute(
            "ALTER TABLE tasks ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
        added.push("tasks.retry_count".to_string());
    }
    if !table_has_column(connection, "tasks", "last_error")? {
        connection.execute("ALTER TABLE tasks ADD COLUMN last_error TEXT", [])?;
        added.push("tasks.last_error".to_string());
    }
    Ok(added)
}

fn table_has_column(connection: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
    for row in rows {
        if row? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn enabled_account_ids(connection: &Connection) -> Result<HashSet<String>> {
    let mut statement = connection.prepare("SELECT id FROM accounts WHERE enabled = 1")?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    let mut account_ids = HashSet::new();
    for row in rows {
        account_ids.insert(row?);
    }
    Ok(account_ids)
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
    fn initialize_upgrades_older_runtime_schema() {
        let data_dir = test_data_dir("schema-upgrade");
        let storage = Storage::open(&data_dir).expect("open storage");

        {
            let connection = Connection::open(storage.db_path()).expect("open old database");
            connection
                .execute_batch(
                    r#"
                    CREATE TABLE accounts (
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
                    CREATE TABLE tasks (
                        id          TEXT PRIMARY KEY,
                        task_type   TEXT NOT NULL,
                        payload     TEXT NOT NULL,
                        run_at      INTEGER NOT NULL,
                        status      TEXT DEFAULT 'pending'
                    );
                    INSERT INTO accounts (
                        id, email, provider, imap_host, smtp_host, auth_type, created_at
                    )
                    VALUES (
                        'account:old', 'old@example.test', 'generic-imap',
                        'imap.example.test', 'smtp.example.test', 'password', 1
                    );
                    "#,
                )
                .expect("seed old schema");
        }

        storage.initialize().expect("upgrade storage");

        let connection = Connection::open(storage.db_path()).expect("reopen upgraded database");
        assert!(table_has_column(&connection, "accounts", "enabled").expect("accounts schema"));
        assert!(table_has_column(&connection, "tasks", "retry_count").expect("tasks schema"));
        assert!(table_has_column(&connection, "tasks", "last_error").expect("tasks schema"));
        assert_eq!(
            connection
                .query_row(
                    "SELECT enabled FROM accounts WHERE id = 'account:old'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("old account enabled default"),
            1
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'message_search_fts'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("fts table exists"),
            1
        );

        drop(connection);
        std::fs::remove_dir_all(data_dir).expect("remove test data");
    }

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
            attachments: Vec::new(),
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

    #[test]
    fn account_config_creates_standard_mailboxes() {
        let data_dir = test_data_dir("account-config");
        let storage = Storage::open(&data_dir).expect("open storage");
        storage.initialize().expect("initialize storage");

        let account = AccountConfig {
            id: AccountId("account:configured".to_string()),
            email: "configured@example.test".to_string(),
            provider: ProviderKind::GenericImap,
            imap_host: "imap.example.test".to_string(),
            imap_port: 993,
            smtp_host: "smtp.example.test".to_string(),
            smtp_port: 587,
            auth_type: AuthType::Password,
        };

        storage
            .upsert_account_config(&account)
            .expect("upsert account config");
        let identity = IdentityConfig {
            id: IdentityId("identity:configured:primary".to_string()),
            account_id: account.id.clone(),
            name: "Configured Sender".to_string(),
            email: "alias@example.test".to_string(),
            reply_to: Some("reply@example.test".to_string()),
        };
        storage.upsert_identity(&identity).expect("upsert identity");
        let identities = storage.list_identities().expect("list identities");
        assert_eq!(identities.len(), 1);
        assert_eq!(identities[0].name, "Configured Sender");
        assert_eq!(identities[0].email, "alias@example.test");

        let accounts = storage.list_accounts().expect("list accounts");
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].email, "configured@example.test");
        assert_eq!(accounts[0].imap_host, "imap.example.test");
        assert_eq!(accounts[0].imap_port, 993);
        assert_eq!(accounts[0].smtp_host, "smtp.example.test");
        assert_eq!(accounts[0].smtp_port, 587);
        assert!(accounts[0].enabled);

        let mailboxes = storage.list_mailboxes().expect("list mailboxes");
        assert_eq!(mailboxes.len(), 5);
        assert!(
            mailboxes
                .iter()
                .any(|mailbox| matches!(mailbox.role, MailboxRole::Inbox))
        );
        assert!(
            mailboxes
                .iter()
                .any(|mailbox| matches!(mailbox.role, MailboxRole::Sent))
        );
        assert!(
            mailboxes
                .iter()
                .any(|mailbox| matches!(mailbox.role, MailboxRole::Drafts))
        );
        assert!(
            mailboxes
                .iter()
                .any(|mailbox| matches!(mailbox.role, MailboxRole::Archive))
        );
        assert!(
            mailboxes
                .iter()
                .any(|mailbox| matches!(mailbox.role, MailboxRole::Trash))
        );

        storage
            .set_account_enabled(&account.id, false)
            .expect("disable account");
        assert!(!storage.list_accounts().expect("list disabled account")[0].enabled);
        assert!(
            storage
                .list_mailboxes()
                .expect("disabled account hides mailboxes")
                .is_empty()
        );

        let edited_account = AccountConfig {
            email: "edited@example.test".to_string(),
            imap_host: "imap.edited.example.test".to_string(),
            ..account.clone()
        };
        storage
            .upsert_account_config(&edited_account)
            .expect("edit account config");
        let edited = storage
            .list_accounts()
            .expect("list edited account")
            .pop()
            .expect("edited account exists");
        assert_eq!(edited.email, "edited@example.test");
        assert_eq!(edited.imap_host, "imap.edited.example.test");
        assert!(!edited.enabled);

        storage
            .set_account_enabled(&account.id, true)
            .expect("enable account");
        assert_eq!(
            storage
                .list_mailboxes()
                .expect("enabled account shows mailboxes")
                .len(),
            5
        );

        storage.delete_account(&account.id).expect("delete account");
        assert!(storage.list_accounts().expect("list accounts").is_empty());
        assert!(
            storage
                .list_identities()
                .expect("list identities")
                .is_empty()
        );
        assert!(storage.list_mailboxes().expect("list mailboxes").is_empty());

        std::fs::remove_dir_all(data_dir).expect("remove test data");
    }

    #[test]
    fn raw_message_import_persists_body_raw_and_attachments() {
        let data_dir = test_data_dir("raw-import");
        let storage = Storage::open(&data_dir).expect("open storage");
        storage.initialize().expect("initialize storage");

        let account = AccountSummary {
            id: AccountId("account:raw".to_string()),
            email: "raw@example.test".to_string(),
            provider: ProviderKind::GenericImap,
        };
        let mailbox = MailboxSummary {
            id: MailboxId("account:raw:inbox".to_string()),
            account_id: account.id.clone(),
            name: "Inbox".to_string(),
            role: MailboxRole::Inbox,
            unread_count: 0,
        };
        let raw = br#"Subject: Raw import
From: sender@example.test
To: raw@example.test
Message-ID: <raw-import@example.test>
Content-Type: multipart/mixed; boundary="mix"

--mix
Content-Type: text/html

<p>Imported HTML body</p>
--mix
Content-Type: text/plain; name="note.txt"
Content-Disposition: attachment; filename="note.txt"
Content-Transfer-Encoding: base64

SGVsbG8=
--mix--
"#;

        storage.upsert_account(&account).expect("upsert account");
        storage.upsert_mailbox(&mailbox).expect("upsert mailbox");

        let message_id = storage
            .import_raw_message(&account.id, &mailbox.id, raw)
            .expect("import raw message");
        assert_eq!(
            message_id,
            MessageId("message:raw-import@example.test".to_string())
        );

        let threads = storage.list_threads().expect("list threads");
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].subject, "Raw import");
        assert_eq!(threads[0].sender, "sender@example.test");

        let body = storage
            .load_message_for_thread(&threads[0].id)
            .expect("load imported message")
            .expect("message exists");
        assert_eq!(body.content_type, "text/html");
        assert_eq!(body.body, "<p>Imported HTML body</p>");
        assert_eq!(body.attachments.len(), 1);
        assert_eq!(body.attachments[0].filename, "note.txt");
        assert_eq!(body.attachments[0].mime_type, "text/plain");
        assert_eq!(body.attachments[0].size, 5);

        let attachments = storage
            .list_attachments(&message_id)
            .expect("list attachments");
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].filename, "note.txt");
        assert_eq!(attachments[0].mime_type, "text/plain");
        assert_eq!(attachments[0].size, 5);
        let attachment_path = data_dir.join(attachments[0].blob_path.as_ref().expect("blob path"));
        assert_eq!(
            std::fs::read(attachment_path).expect("read attachment"),
            b"Hello"
        );
        assert!(
            data_dir
                .join("raw")
                .join("message-raw-import-example.test.eml")
                .exists()
        );

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
