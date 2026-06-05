#![allow(clippy::manual_async_fn)]

use courier_adapter::{
    MailRemote, NoopRemote, OutgoingMessage, RemoteAttachment, RemoteDelta, RemoteMailbox,
    RemoteMessage, RemoteOp,
};
use courier_proto::{
    AccountId, AttachmentSummary, DraftId, DraftMessage, MailboxId, MailboxSummary, MessageBody,
    MessageId, TaskId, ThreadSummary,
};
use courier_storage::{QueuedOp, Storage};
use serde::Deserialize;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("sync worker is not running for account {0}")]
    WorkerNotRunning(String),
    #[error("storage error: {0}")]
    Storage(#[from] courier_storage::Error),
    #[error("adapter error: {0}")]
    Adapter(#[from] courier_adapter::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported queued op type: {0}")]
    UnsupportedQueuedOp(String),
    #[error("draft not found: {0}")]
    DraftNotFound(String),
}

#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub base_delay_ms: u64,
}

#[derive(Debug, Clone)]
pub struct SyncScheduler<R = NoopRemote> {
    storage: Storage,
    remote: R,
    retry_policy: RetryPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncReport {
    pub account_id: AccountId,
    pub pending_ops: usize,
    pub applied_ops: usize,
    pub remote_messages: usize,
    pub remote_deleted: usize,
    pub remote_moved: usize,
    pub reset_mailboxes: usize,
    pub conflicts: usize,
    pub mailbox_updates: Vec<MailboxSyncReport>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailboxSyncReport {
    pub mailbox_id: MailboxId,
    pub message_ids: Vec<MessageId>,
    pub deleted_messages: usize,
    pub moved_messages: usize,
    pub uidvalidity_reset: bool,
    pub conflicts: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendReport {
    pub task_id: TaskId,
    pub draft_id: DraftId,
    pub message_id: MessageId,
    pub remote_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendQueueRunReport {
    pub attempted: usize,
    pub sent: Vec<SendReport>,
    pub failed: Vec<(DraftId, String)>,
}

impl SyncScheduler<NoopRemote> {
    pub fn new(storage: Storage) -> Self {
        Self::with_remote(storage, NoopRemote::default())
    }
}

impl<R> SyncScheduler<R>
where
    R: MailRemote + Clone,
{
    pub fn with_remote(storage: Storage, remote: R) -> Self {
        Self {
            storage,
            remote,
            retry_policy: RetryPolicy {
                max_retries: 5,
                base_delay_ms: 500,
            },
        }
    }

    pub async fn sync_now(&self, account_id: AccountId) -> Result<SyncReport> {
        let pending_ops = self.storage.pending_ops_for_account(Some(&account_id))?;
        let remote_ops = pending_ops
            .iter()
            .map(remote_op_from_queued_op)
            .collect::<Result<Vec<_>>>()?;

        if !remote_ops.is_empty()
            && let Err(error) = self.remote.apply_ops(remote_ops).await
        {
            for op in &pending_ops {
                self.storage.mark_op_failed(op.id, &error.to_string())?;
            }

            return Err(error.into());
        }

        for op in &pending_ops {
            tracing::info!(
                op_id = op.id,
                op_type = %op.op_type,
                account_id = %op.account_id.0,
                "remote adapter acknowledged queued op"
            );
            self.storage.mark_op_completed(op.id)?;
        }

        let mailbox_updates = self.pull_remote_mailbox_deltas(&account_id).await?;
        let remote_messages = mailbox_updates
            .iter()
            .map(|mailbox| mailbox.message_ids.len())
            .sum::<usize>();
        let remote_deleted = mailbox_updates
            .iter()
            .map(|mailbox| mailbox.deleted_messages)
            .sum::<usize>();
        let remote_moved = mailbox_updates
            .iter()
            .map(|mailbox| mailbox.moved_messages)
            .sum::<usize>();
        let reset_mailboxes = mailbox_updates
            .iter()
            .filter(|mailbox| mailbox.uidvalidity_reset)
            .count();
        let conflicts = mailbox_updates
            .iter()
            .map(|mailbox| mailbox.conflicts)
            .sum::<usize>();

        tracing::info!(
            ?account_id,
            pending_ops = pending_ops.len(),
            applied_ops = pending_ops.len(),
            remote_messages,
            remote_deleted,
            remote_moved,
            reset_mailboxes,
            conflicts,
            db = %self.storage.db_path().display(),
            "sync requested"
        );

        Ok(SyncReport {
            account_id,
            pending_ops: pending_ops.len(),
            applied_ops: pending_ops.len(),
            remote_messages,
            remote_deleted,
            remote_moved,
            reset_mailboxes,
            conflicts,
            mailbox_updates,
        })
    }

    pub fn retry_policy(&self) -> &RetryPolicy {
        &self.retry_policy
    }

    pub async fn send_draft(&self, draft_id: DraftId) -> Result<SendReport> {
        let draft = self
            .storage
            .load_draft(&draft_id)?
            .ok_or_else(|| Error::DraftNotFound(draft_id.0.clone()))?;

        self.storage.mark_draft_sending(&draft_id)?;
        let sender = self
            .storage
            .list_accounts()?
            .into_iter()
            .find(|account| account.id == draft.account_id)
            .map(|account| account.email)
            .ok_or_else(|| Error::WorkerNotRunning(draft.account_id.0.clone()))?;
        let outgoing = outgoing_message_from_draft(&draft, sender);

        match self.remote.send_message(outgoing).await {
            Ok(result) => {
                let message_id = self
                    .storage
                    .persist_sent_draft(&draft, result.remote_id.as_deref())?;
                self.storage.mark_draft_sent(&draft_id)?;

                Ok(SendReport {
                    task_id: TaskId(format!("send:{}", draft_id.0)),
                    draft_id,
                    message_id,
                    remote_id: result.remote_id,
                })
            }
            Err(error) => {
                let retry_count = self
                    .storage
                    .draft_retry_count(&draft_id)?
                    .unwrap_or_default();
                let error_message = error.to_string();
                if retry_count + 1 >= self.retry_policy.max_retries {
                    self.storage.mark_draft_failed(&draft_id, &error_message)?;
                } else {
                    self.storage.mark_draft_retry(
                        &draft_id,
                        &error_message,
                        unix_timestamp() + self.retry_delay_seconds(retry_count),
                    )?;
                }
                Err(error.into())
            }
        }
    }

    pub async fn send_due_drafts(&self, now: i64, limit: usize) -> Result<SendQueueRunReport> {
        let draft_ids = self.storage.due_draft_ids(now, limit)?;
        let mut sent = Vec::new();
        let mut failed = Vec::new();

        for draft_id in draft_ids {
            match self.send_draft(draft_id.clone()).await {
                Ok(report) => sent.push(report),
                Err(error) => failed.push((draft_id, error.to_string())),
            }
        }

        Ok(SendQueueRunReport {
            attempted: sent.len() + failed.len(),
            sent,
            failed,
        })
    }

    async fn pull_remote_mailbox_deltas(
        &self,
        account_id: &AccountId,
    ) -> Result<Vec<MailboxSyncReport>> {
        let remote_mailboxes = self.remote.list_mailboxes().await?;
        let remote_summaries = remote_mailboxes
            .iter()
            .map(|mailbox| remote_mailbox_summary(account_id, mailbox))
            .collect::<Vec<_>>();
        let reconciled = self
            .storage
            .reconcile_remote_mailboxes(account_id, &remote_summaries)?;
        if reconciled > 0 {
            tracing::info!(
                account_id = %account_id.0,
                mailboxes = reconciled,
                "remote mailbox discovery reconciled into local storage"
            );
        }

        let mailboxes = if remote_summaries.is_empty() {
            self.storage
                .list_mailboxes()?
                .into_iter()
                .filter(|mailbox| mailbox.account_id == *account_id)
                .collect::<Vec<_>>()
        } else {
            remote_summaries
        };
        let mut mailbox_updates = Vec::new();

        for mailbox in mailboxes {
            let cursor = self.storage.mailbox_sync_cursor(&mailbox.id)?;
            let delta = self
                .remote
                .fetch_delta(mailbox.id.clone(), cursor.clone())
                .await?;
            let report = self.persist_remote_delta(account_id, &mailbox.id, cursor, delta)?;
            if report.has_changes() {
                mailbox_updates.push(MailboxSyncReport {
                    mailbox_id: mailbox.id,
                    message_ids: report.message_ids,
                    deleted_messages: report.deleted_messages,
                    moved_messages: report.moved_messages,
                    uidvalidity_reset: report.uidvalidity_reset,
                    conflicts: report.conflicts,
                });
            }
        }

        Ok(mailbox_updates)
    }

    fn persist_remote_delta(
        &self,
        account_id: &AccountId,
        mailbox_id: &MailboxId,
        previous_cursor: courier_domain::SyncCursor,
        delta: RemoteDelta,
    ) -> Result<DeltaPersistReport> {
        let mut report = DeltaPersistReport::default();
        if previous_cursor.validity_changed(delta.cursor.uid_validity) {
            let conflicted = self
                .storage
                .reset_mailbox_for_uidvalidity_change(mailbox_id)?;
            report.uidvalidity_reset = true;
            report.conflicts += conflicted.len();
            tracing::warn!(
                mailbox_id = %mailbox_id.0,
                old_uid_validity = previous_cursor.uid_validity,
                new_uid_validity = delta.cursor.uid_validity,
                conflicts = conflicted.len(),
                "mailbox uidvalidity changed; local mailbox view reset"
            );
        }

        for message_id in &delta.deleted_messages {
            if self.storage.apply_remote_delete(mailbox_id, message_id)? {
                report.deleted_messages += 1;
            } else {
                report.conflicts += 1;
            }
        }

        for moved in &delta.moved_messages {
            if self.storage.apply_remote_move(
                mailbox_id,
                &moved.target_mailbox_id,
                &moved.message_id,
            )? {
                report.moved_messages += 1;
            } else {
                report.conflicts += 1;
            }
        }

        let mut message_ids = Vec::with_capacity(delta.new_message_count());

        for message in delta.messages {
            let message_id = message.id.clone();
            if self.persist_remote_message(account_id, mailbox_id, message)? {
                message_ids.push(message_id);
            }
        }

        self.storage
            .update_mailbox_sync_cursor(mailbox_id, &delta.cursor)?;

        report.message_ids = message_ids;
        Ok(report)
    }

    fn persist_remote_message(
        &self,
        account_id: &AccountId,
        mailbox_id: &MailboxId,
        message: RemoteMessage,
    ) -> Result<bool> {
        if self
            .storage
            .message_conflict_state(&message.id)?
            .is_some_and(|state| state == "local_pending")
        {
            tracing::warn!(
                message_id = %message.id.0,
                account_id = %account_id.0,
                "remote delta conflicted with pending local change"
            );
            self.storage.mark_message_conflicted(&message.id)?;
            return Ok(false);
        }

        let message_id = message.id.clone();
        let attachments = remote_attachment_summaries(message.attachments);
        let thread = ThreadSummary {
            id: message.thread_id.clone(),
            account_id: account_id.clone(),
            subject: message.subject.clone(),
            sender: message.from.clone(),
            snippet: message.snippet.clone(),
            unread: !message.read,
            last_message_ts: message.timestamp,
        };
        let body = MessageBody {
            id: message.id,
            thread_id: message.thread_id,
            subject: message.subject,
            from: message.from,
            to: message.to,
            content_type: message.content_type,
            body: message.body,
            attachments: Vec::new(),
        };

        self.storage.upsert_thread(&thread)?;
        self.storage.upsert_message(mailbox_id, &thread, &body)?;
        if let Some(raw) = message.raw.as_deref() {
            self.storage
                .persist_raw_message_for_existing(&body.id, raw)?;
        }
        self.storage
            .persist_attachment_metadata(&message_id, &attachments)?;

        Ok(true)
    }

    fn retry_delay_seconds(&self, retry_count: u32) -> i64 {
        let multiplier = 2_u64.saturating_pow(retry_count.min(16));
        let delay_ms = self.retry_policy.base_delay_ms.saturating_mul(multiplier);
        ((delay_ms.saturating_add(999)) / 1000).min(i64::MAX as u64) as i64
    }
}

#[derive(Debug, Default)]
struct DeltaPersistReport {
    message_ids: Vec<MessageId>,
    deleted_messages: usize,
    moved_messages: usize,
    uidvalidity_reset: bool,
    conflicts: usize,
}

impl DeltaPersistReport {
    fn has_changes(&self) -> bool {
        !self.message_ids.is_empty()
            || self.deleted_messages > 0
            || self.moved_messages > 0
            || self.uidvalidity_reset
            || self.conflicts > 0
    }
}

fn remote_attachment_summaries(attachments: Vec<RemoteAttachment>) -> Vec<AttachmentSummary> {
    attachments
        .into_iter()
        .map(|attachment| AttachmentSummary {
            id: attachment.id,
            filename: attachment.filename,
            mime_type: attachment.mime_type,
            size: attachment.size,
            blob_path: None,
            content_id: attachment.content_id,
            inline: attachment.inline,
        })
        .collect()
}

fn remote_mailbox_summary(account_id: &AccountId, mailbox: &RemoteMailbox) -> MailboxSummary {
    MailboxSummary {
        id: mailbox.id.clone(),
        account_id: account_id.clone(),
        name: mailbox.name.clone(),
        role: mailbox.role.clone(),
        unread_count: 0,
    }
}

#[derive(Debug, Deserialize)]
struct MarkReadPayload {
    message_id: String,
    read: bool,
}

#[derive(Debug, Deserialize)]
struct MovePayload {
    message_id: String,
    target_mailbox_id: String,
}

fn remote_op_from_queued_op(op: &QueuedOp) -> Result<RemoteOp> {
    match op.op_type.as_str() {
        "mark_read" => {
            let payload: MarkReadPayload = serde_json::from_str(&op.payload)?;
            Ok(RemoteOp::MarkRead {
                message_id: payload.message_id,
                read: payload.read,
            })
        }
        "move" => {
            let payload: MovePayload = serde_json::from_str(&op.payload)?;
            Ok(RemoteOp::Move {
                message_id: payload.message_id,
                mailbox_id: payload.target_mailbox_id,
            })
        }
        other => Err(Error::UnsupportedQueuedOp(other.to_string())),
    }
}

fn unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
        .min(i64::MAX as u64) as i64
}

fn outgoing_message_from_draft(draft: &DraftMessage, from: String) -> OutgoingMessage {
    let mut headers = vec![
        format!("From: {}", sanitize_header(&from)),
        format!("To: {}", sanitize_header_list(&draft.to)),
        format!("Subject: {}", sanitize_header(&draft.subject)),
        "MIME-Version: 1.0".to_string(),
        "Content-Type: text/plain; charset=utf-8".to_string(),
    ];

    if !draft.cc.is_empty() {
        headers.insert(2, format!("Cc: {}", sanitize_header_list(&draft.cc)));
    }

    let recipients = draft
        .to
        .iter()
        .chain(draft.cc.iter())
        .chain(draft.bcc.iter())
        .map(|recipient| sanitize_header(recipient))
        .filter(|recipient| !recipient.is_empty())
        .collect();
    let rfc822 = format!("{}\r\n\r\n{}", headers.join("\r\n"), draft.body).into_bytes();
    OutgoingMessage {
        rfc822,
        from,
        recipients,
    }
}

fn sanitize_header(value: &str) -> String {
    value.replace(['\r', '\n'], " ").trim().to_string()
}

fn sanitize_header_list(values: &[String]) -> String {
    values
        .iter()
        .map(|value| sanitize_header(value))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Debug, Clone)]
pub struct SyncWorker {
    pub account_id: AccountId,
}

#[derive(Debug, Clone)]
pub struct SendQueue;

#[derive(Debug, Clone)]
pub struct OpQueue;

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use courier_adapter::{
        AttachmentFetchRequest, AttachmentFetchResult, MailRemote, NoopRemote, OutgoingMessage,
        RemoteDelta, RemoteMessage, RemoteOp, SendResult,
    };
    use courier_domain::SyncCursor;
    use courier_proto::{
        AccountId, AccountSummary, DraftId, DraftMessage, MailboxId, MailboxRole, MailboxSummary,
        MessageBody, MessageId, ProviderKind, ThreadId, ThreadSummary,
    };

    use super::*;

    #[tokio::test]
    async fn sync_now_acknowledges_pending_local_ops() {
        let data_dir = test_data_dir("ack-pending-ops");
        let storage = Storage::open(&data_dir).expect("open storage");
        storage.initialize().expect("initialize storage");

        let account = AccountSummary {
            id: AccountId("account:sync".to_string()),
            email: "sync@example.test".to_string(),
            provider: ProviderKind::GenericImap,
        };
        let mailbox = MailboxSummary {
            id: MailboxId("account:sync:inbox".to_string()),
            account_id: account.id.clone(),
            name: "Inbox".to_string(),
            role: MailboxRole::Inbox,
            unread_count: 1,
        };
        let thread = ThreadSummary {
            id: ThreadId("thread:sync".to_string()),
            account_id: account.id.clone(),
            subject: "Pending op".to_string(),
            sender: "sender@example.test".to_string(),
            snippet: "A local op waits for sync.".to_string(),
            unread: true,
            last_message_ts: 7,
        };
        let body = MessageBody {
            id: MessageId("message:sync".to_string()),
            thread_id: thread.id.clone(),
            subject: thread.subject.clone(),
            from: thread.sender.clone(),
            to: vec!["sync@example.test".to_string()],
            content_type: "text/plain".to_string(),
            body: "Local operation acknowledgement test.".to_string(),
            attachments: Vec::new(),
        };

        storage.upsert_account(&account).expect("upsert account");
        storage.upsert_mailbox(&mailbox).expect("upsert mailbox");
        storage.upsert_thread(&thread).expect("upsert thread");
        storage
            .upsert_message(&mailbox.id, &thread, &body)
            .expect("upsert message");
        storage
            .mark_message_read(&body.id, true)
            .expect("queue mark-read op");

        assert_eq!(storage.pending_ops().expect("pending before sync").len(), 1);

        let remote = NoopRemote::default();
        let scheduler = SyncScheduler::with_remote(storage.clone(), remote.clone());
        let report = scheduler
            .sync_now(account.id.clone())
            .await
            .expect("sync now");

        assert_eq!(report.pending_ops, 1);
        assert_eq!(report.applied_ops, 1);
        assert_eq!(report.remote_messages, 0);
        assert!(report.mailbox_updates.is_empty());
        assert!(
            storage
                .pending_ops()
                .expect("pending after sync")
                .is_empty()
        );
        assert_eq!(
            storage
                .message_conflict_state(&body.id)
                .expect("conflict state after sync"),
            Some("none".to_string())
        );
        assert_eq!(
            remote.applied_ops(),
            vec![RemoteOp::MarkRead {
                message_id: body.id.0.clone(),
                read: true,
            }]
        );

        std::fs::remove_dir_all(data_dir).expect("remove test data");
    }

    #[tokio::test]
    async fn sync_now_persists_remote_delta() {
        let data_dir = test_data_dir("remote-delta");
        let storage = Storage::open(&data_dir).expect("open storage");
        storage.initialize().expect("initialize storage");

        let account = AccountSummary {
            id: AccountId("account:remote".to_string()),
            email: "remote@example.test".to_string(),
            provider: ProviderKind::GenericImap,
        };
        let mailbox = MailboxSummary {
            id: MailboxId("account:remote:inbox".to_string()),
            account_id: account.id.clone(),
            name: "Inbox".to_string(),
            role: MailboxRole::Inbox,
            unread_count: 0,
        };

        storage.upsert_account(&account).expect("upsert account");
        storage.upsert_mailbox(&mailbox).expect("upsert mailbox");

        let remote = NoopRemote::with_delta(RemoteDelta {
            cursor: SyncCursor {
                uid_validity: 11,
                last_uid: 42,
                highest_modseq: Some(9001),
            },
            messages: vec![RemoteMessage {
                id: MessageId("message:remote".to_string()),
                thread_id: ThreadId("thread:remote".to_string()),
                subject: "Remote delta".to_string(),
                from: "delta@example.test".to_string(),
                to: vec!["remote@example.test".to_string()],
                snippet: "A server message arrived.".to_string(),
                body: "This message was pulled through the adapter delta path.".to_string(),
                content_type: "text/plain".to_string(),
                timestamp: 1234,
                read: false,
                raw: None,
                attachments: Vec::new(),
            }],
            deleted_messages: Vec::new(),
            moved_messages: Vec::new(),
        });
        let scheduler = SyncScheduler::with_remote(storage.clone(), remote);
        let report = scheduler
            .sync_now(account.id.clone())
            .await
            .expect("sync now");

        assert_eq!(report.pending_ops, 0);
        assert_eq!(report.applied_ops, 0);
        assert_eq!(report.remote_messages, 1);
        assert_eq!(report.mailbox_updates.len(), 1);
        assert_eq!(report.mailbox_updates[0].mailbox_id, mailbox.id);
        assert_eq!(
            report.mailbox_updates[0].message_ids,
            vec![MessageId("message:remote".to_string())]
        );

        let threads = storage.list_threads().expect("list threads");
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].subject, "Remote delta");
        assert!(threads[0].unread);

        let body = storage
            .load_message_for_thread(&ThreadId("thread:remote".to_string()))
            .expect("load message")
            .expect("message body exists");
        assert_eq!(
            body.body,
            "This message was pulled through the adapter delta path."
        );

        let cursor = storage
            .mailbox_sync_cursor(&mailbox.id)
            .expect("mailbox cursor");
        assert_eq!(cursor.uid_validity, 11);
        assert_eq!(cursor.last_uid, 42);
        assert_eq!(cursor.highest_modseq, Some(9001));

        std::fs::remove_dir_all(data_dir).expect("remove test data");
    }

    #[tokio::test]
    async fn sync_now_marks_conflict_without_overwriting_local_pending_message() {
        let data_dir = test_data_dir("remote-conflict");
        let storage = Storage::open(&data_dir).expect("open storage");
        storage.initialize().expect("initialize storage");

        let account = AccountSummary {
            id: AccountId("account:conflict".to_string()),
            email: "conflict@example.test".to_string(),
            provider: ProviderKind::GenericImap,
        };
        let mailbox = MailboxSummary {
            id: MailboxId("account:conflict:inbox".to_string()),
            account_id: account.id.clone(),
            name: "Inbox".to_string(),
            role: MailboxRole::Inbox,
            unread_count: 1,
        };
        let thread = ThreadSummary {
            id: ThreadId("thread:conflict".to_string()),
            account_id: account.id.clone(),
            subject: "Local subject".to_string(),
            sender: "sender@example.test".to_string(),
            snippet: "Local snippet".to_string(),
            unread: true,
            last_message_ts: 7,
        };
        let body = MessageBody {
            id: MessageId("message:conflict".to_string()),
            thread_id: thread.id.clone(),
            subject: thread.subject.clone(),
            from: thread.sender.clone(),
            to: vec!["conflict@example.test".to_string()],
            content_type: "text/plain".to_string(),
            body: "Local body should remain.".to_string(),
            attachments: Vec::new(),
        };

        storage.upsert_account(&account).expect("upsert account");
        storage.upsert_mailbox(&mailbox).expect("upsert mailbox");
        storage.upsert_thread(&thread).expect("upsert thread");
        storage
            .upsert_message(&mailbox.id, &thread, &body)
            .expect("upsert message");
        storage
            .mark_message_read(&body.id, true)
            .expect("mark local pending");

        let scheduler = SyncScheduler::with_remote(storage.clone(), NoopRemote::default());
        let persisted = scheduler
            .persist_remote_message(
                &account.id,
                &mailbox.id,
                RemoteMessage {
                    id: body.id.clone(),
                    thread_id: thread.id.clone(),
                    subject: "Remote subject".to_string(),
                    from: "remote@example.test".to_string(),
                    to: vec!["conflict@example.test".to_string()],
                    snippet: "Remote snippet".to_string(),
                    body: "Remote body should not overwrite.".to_string(),
                    content_type: "text/plain".to_string(),
                    timestamp: 9,
                    read: false,
                    raw: None,
                    attachments: Vec::new(),
                },
            )
            .expect("persist remote message");

        assert!(!persisted);
        assert_eq!(
            storage
                .message_conflict_state(&body.id)
                .expect("conflict state"),
            Some("conflicted".to_string())
        );
        let loaded = storage
            .load_message_for_thread(&thread.id)
            .expect("load message")
            .expect("message exists");
        assert_eq!(loaded.body, "Local body should remain.");

        std::fs::remove_dir_all(data_dir).expect("remove test data");
    }

    #[tokio::test]
    async fn sync_now_applies_confirmed_remote_delta_after_clearing_local_pending() {
        let data_dir = test_data_dir("remote-confirmed-delta");
        let storage = Storage::open(&data_dir).expect("open storage");
        storage.initialize().expect("initialize storage");

        let account = AccountSummary {
            id: AccountId("account:confirmed".to_string()),
            email: "confirmed@example.test".to_string(),
            provider: ProviderKind::GenericImap,
        };
        let mailbox = MailboxSummary {
            id: MailboxId("account:confirmed:inbox".to_string()),
            account_id: account.id.clone(),
            name: "Inbox".to_string(),
            role: MailboxRole::Inbox,
            unread_count: 1,
        };
        let thread = ThreadSummary {
            id: ThreadId("thread:confirmed".to_string()),
            account_id: account.id.clone(),
            subject: "Local subject".to_string(),
            sender: "sender@example.test".to_string(),
            snippet: "Local snippet".to_string(),
            unread: true,
            last_message_ts: 7,
        };
        let body = MessageBody {
            id: MessageId("message:confirmed".to_string()),
            thread_id: thread.id.clone(),
            subject: thread.subject.clone(),
            from: thread.sender.clone(),
            to: vec!["confirmed@example.test".to_string()],
            content_type: "text/plain".to_string(),
            body: "Local body should be replaced after writeback confirmation.".to_string(),
            attachments: Vec::new(),
        };

        storage.upsert_account(&account).expect("upsert account");
        storage.upsert_mailbox(&mailbox).expect("upsert mailbox");
        storage.upsert_thread(&thread).expect("upsert thread");
        storage
            .upsert_message(&mailbox.id, &thread, &body)
            .expect("upsert message");
        storage
            .mark_message_read(&body.id, true)
            .expect("mark local pending");

        let remote = NoopRemote::with_delta(RemoteDelta {
            cursor: SyncCursor {
                uid_validity: 1,
                last_uid: 2,
                highest_modseq: None,
            },
            messages: vec![RemoteMessage {
                id: body.id.clone(),
                thread_id: thread.id.clone(),
                subject: "Remote subject".to_string(),
                from: "remote@example.test".to_string(),
                to: vec!["confirmed@example.test".to_string()],
                snippet: "Remote snippet".to_string(),
                body: "Remote body can update after writeback.".to_string(),
                content_type: "text/plain".to_string(),
                timestamp: 9,
                read: false,
                raw: None,
                attachments: Vec::new(),
            }],
            deleted_messages: Vec::new(),
            moved_messages: Vec::new(),
        });
        let scheduler = SyncScheduler::with_remote(storage.clone(), remote);
        let report = scheduler
            .sync_now(account.id.clone())
            .await
            .expect("sync now");

        assert_eq!(report.remote_messages, 1);
        assert_eq!(report.mailbox_updates.len(), 1);
        assert_eq!(
            storage
                .message_conflict_state(&body.id)
                .expect("conflict state"),
            Some("none".to_string())
        );
        let loaded = storage
            .load_message_for_thread(&thread.id)
            .expect("load message")
            .expect("message exists");
        assert_eq!(loaded.body, "Remote body can update after writeback.");

        std::fs::remove_dir_all(data_dir).expect("remove test data");
    }

    #[tokio::test]
    async fn send_draft_uses_remote_and_persists_sent_copy() {
        let data_dir = test_data_dir("send-draft");
        let storage = Storage::open(&data_dir).expect("open storage");
        storage.initialize().expect("initialize storage");

        let account = AccountSummary {
            id: AccountId("account:send".to_string()),
            email: "sender@example.test".to_string(),
            provider: ProviderKind::GenericImap,
        };
        let draft = DraftMessage {
            id: DraftId("draft:send".to_string()),
            account_id: account.id.clone(),
            to: vec!["receiver@example.test".to_string()],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "Send pipeline".to_string(),
            body: "Sending should go through MailRemote.".to_string(),
            attachments: Vec::new(),
        };

        storage.upsert_account(&account).expect("upsert account");
        storage.save_draft(&draft).expect("save draft");

        let remote = NoopRemote::default();
        let scheduler = SyncScheduler::with_remote(storage.clone(), remote.clone());
        let report = scheduler
            .send_draft(draft.id.clone())
            .await
            .expect("send draft");

        assert_eq!(report.draft_id, draft.id);
        assert_eq!(report.message_id, MessageId("sent:draft:send".to_string()));
        assert_eq!(report.remote_id, Some("noop-sent-1".to_string()));
        assert_eq!(
            storage.draft_status(&draft.id).expect("draft status"),
            Some("done".to_string())
        );

        let sent_messages = remote.sent_messages();
        assert_eq!(sent_messages.len(), 1);
        let raw = String::from_utf8(sent_messages[0].rfc822.clone()).expect("utf8 rfc822");
        assert!(raw.contains("To: receiver@example.test"));
        assert!(raw.contains("Subject: Send pipeline"));
        assert!(raw.contains("Sending should go through MailRemote."));

        let sent_mailbox = MailboxId("account:send:sent".to_string());
        let sent_threads = storage
            .list_threads_for_mailbox(Some(&sent_mailbox))
            .expect("list sent threads");
        assert_eq!(sent_threads.len(), 1);
        assert_eq!(sent_threads[0].subject, "Send pipeline");
        assert!(!sent_threads[0].unread);

        std::fs::remove_dir_all(data_dir).expect("remove test data");
    }

    #[tokio::test]
    async fn send_draft_requeues_failures_until_retry_limit() {
        let data_dir = test_data_dir("send-retry");
        let storage = Storage::open(&data_dir).expect("open storage");
        storage.initialize().expect("initialize storage");

        let account = AccountSummary {
            id: AccountId("account:retry".to_string()),
            email: "retry@example.test".to_string(),
            provider: ProviderKind::GenericImap,
        };
        let draft = DraftMessage {
            id: DraftId("draft:retry".to_string()),
            account_id: account.id.clone(),
            to: vec!["receiver@example.test".to_string()],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "Retry pipeline".to_string(),
            body: "This send should be retried.".to_string(),
            attachments: Vec::new(),
        };

        storage.upsert_account(&account).expect("upsert account");
        storage.save_draft(&draft).expect("save draft");

        let scheduler = SyncScheduler::with_remote(storage.clone(), FailingRemote);

        for attempt in 1..=scheduler.retry_policy().max_retries {
            let result = scheduler.send_draft(draft.id.clone()).await;
            assert!(result.is_err());

            let status = storage
                .draft_status(&draft.id)
                .expect("draft status")
                .expect("draft status exists");
            let retry_count = storage
                .draft_retry_count(&draft.id)
                .expect("draft retry count")
                .expect("draft retry count exists");

            if attempt < scheduler.retry_policy().max_retries {
                assert_eq!(status, "pending");
                assert_eq!(retry_count, attempt);
            } else {
                assert_eq!(status, "failed");
                assert_eq!(retry_count, scheduler.retry_policy().max_retries);
            }
        }

        std::fs::remove_dir_all(data_dir).expect("remove test data");
    }

    #[derive(Debug, Clone)]
    struct FailingRemote;

    impl MailRemote for FailingRemote {
        fn list_mailboxes(
            &self,
        ) -> impl std::future::Future<
            Output = courier_adapter::Result<Vec<courier_adapter::RemoteMailbox>>,
        > + Send {
            async { Ok(Vec::new()) }
        }

        fn fetch_delta(
            &self,
            _mailbox: MailboxId,
            cursor: SyncCursor,
        ) -> impl std::future::Future<Output = courier_adapter::Result<RemoteDelta>> + Send
        {
            async move { Ok(RemoteDelta::empty(cursor)) }
        }

        fn apply_ops(
            &self,
            _ops: Vec<RemoteOp>,
        ) -> impl std::future::Future<Output = courier_adapter::Result<()>> + Send {
            async { Ok(()) }
        }

        fn send_message(
            &self,
            _message: OutgoingMessage,
        ) -> impl std::future::Future<Output = courier_adapter::Result<SendResult>> + Send {
            async { Err(courier_adapter::Error::NotImplemented("smtp send_message")) }
        }

        fn fetch_attachment(
            &self,
            _request: AttachmentFetchRequest,
        ) -> impl std::future::Future<Output = courier_adapter::Result<AttachmentFetchResult>> + Send
        {
            async { Err(courier_adapter::Error::NotImplemented("attachment fetch")) }
        }
    }

    fn test_data_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();

        std::env::current_dir()
            .expect("current dir")
            .join("target")
            .join("courier-sync-tests")
            .join(format!("{name}-{nonce}"))
    }
}
