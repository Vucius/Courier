use courier_adapter::{
    MailRemote, NoopRemote, OutgoingMessage, RemoteDelta, RemoteMessage, RemoteOp,
};
use courier_proto::{
    AccountId, DraftId, DraftMessage, MailboxId, MessageBody, MessageId, TaskId, ThreadSummary,
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
    pub mailbox_updates: Vec<MailboxSyncReport>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailboxSyncReport {
    pub mailbox_id: MailboxId,
    pub message_ids: Vec<MessageId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendReport {
    pub task_id: TaskId,
    pub draft_id: DraftId,
    pub message_id: MessageId,
    pub remote_id: Option<String>,
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

        if !remote_ops.is_empty() {
            if let Err(error) = self.remote.apply_ops(remote_ops).await {
                for op in &pending_ops {
                    self.storage.mark_op_failed(op.id, &error.to_string())?;
                }

                return Err(error.into());
            }
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

        tracing::info!(
            ?account_id,
            pending_ops = pending_ops.len(),
            applied_ops = pending_ops.len(),
            remote_messages,
            db = %self.storage.db_path().display(),
            "sync requested"
        );

        Ok(SyncReport {
            account_id,
            pending_ops: pending_ops.len(),
            applied_ops: pending_ops.len(),
            remote_messages,
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
        let outgoing = outgoing_message_from_draft(&draft);

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
                self.storage.mark_draft_failed(&draft_id)?;
                Err(error.into())
            }
        }
    }

    async fn pull_remote_mailbox_deltas(
        &self,
        account_id: &AccountId,
    ) -> Result<Vec<MailboxSyncReport>> {
        let mailboxes = self.storage.list_mailboxes()?;
        let mut mailbox_updates = Vec::new();

        for mailbox in mailboxes
            .into_iter()
            .filter(|mailbox| mailbox.account_id == *account_id)
        {
            let cursor = self.storage.mailbox_sync_cursor(&mailbox.id)?;
            let delta = self.remote.fetch_delta(mailbox.id.clone(), cursor).await?;
            let message_ids = self.persist_remote_delta(account_id, &mailbox.id, delta)?;
            if !message_ids.is_empty() {
                mailbox_updates.push(MailboxSyncReport {
                    mailbox_id: mailbox.id,
                    message_ids,
                });
            }
        }

        Ok(mailbox_updates)
    }

    fn persist_remote_delta(
        &self,
        account_id: &AccountId,
        mailbox_id: &MailboxId,
        delta: RemoteDelta,
    ) -> Result<Vec<MessageId>> {
        let mut message_ids = Vec::with_capacity(delta.new_message_count());

        for message in delta.messages {
            message_ids.push(message.id.clone());
            self.persist_remote_message(account_id, mailbox_id, message)?;
        }

        self.storage
            .update_mailbox_sync_cursor(mailbox_id, &delta.cursor)?;

        Ok(message_ids)
    }

    fn persist_remote_message(
        &self,
        account_id: &AccountId,
        mailbox_id: &MailboxId,
        message: RemoteMessage,
    ) -> Result<()> {
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
        };

        self.storage.upsert_thread(&thread)?;
        self.storage.upsert_message(mailbox_id, &thread, &body)?;

        Ok(())
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

fn outgoing_message_from_draft(draft: &DraftMessage) -> OutgoingMessage {
    let mut headers = vec![
        format!("To: {}", sanitize_header_list(&draft.to)),
        format!("Subject: {}", sanitize_header(&draft.subject)),
        "MIME-Version: 1.0".to_string(),
        "Content-Type: text/plain; charset=utf-8".to_string(),
    ];

    if !draft.cc.is_empty() {
        headers.insert(1, format!("Cc: {}", sanitize_header_list(&draft.cc)));
    }
    if !draft.bcc.is_empty() {
        headers.insert(2, format!("Bcc: {}", sanitize_header_list(&draft.bcc)));
    }

    let rfc822 = format!("{}\r\n\r\n{}", headers.join("\r\n"), draft.body).into_bytes();
    OutgoingMessage { rfc822 }
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

    use courier_adapter::{NoopRemote, RemoteDelta, RemoteMessage, RemoteOp};
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
            }],
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
