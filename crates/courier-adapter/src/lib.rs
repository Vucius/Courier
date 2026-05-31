use std::collections::VecDeque;
use std::future::Future;
use std::sync::{Arc, Mutex};

use courier_domain::SyncCursor;
use courier_proto::{MailboxId, MessageId, ThreadId};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("remote capability is not implemented: {0}")]
    NotImplemented(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteMailbox {
    pub id: MailboxId,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct RemoteDelta {
    pub cursor: SyncCursor,
    pub messages: Vec<RemoteMessage>,
}

impl RemoteDelta {
    pub fn empty(cursor: SyncCursor) -> Self {
        Self {
            cursor,
            messages: Vec::new(),
        }
    }

    pub fn new_message_count(&self) -> usize {
        self.messages.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteMessage {
    pub id: MessageId,
    pub thread_id: ThreadId,
    pub subject: String,
    pub from: String,
    pub to: Vec<String>,
    pub snippet: String,
    pub body: String,
    pub content_type: String,
    pub timestamp: i64,
    pub read: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteOp {
    MarkRead {
        message_id: String,
        read: bool,
    },
    Move {
        message_id: String,
        mailbox_id: String,
    },
    Delete {
        message_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutgoingMessage {
    pub rfc822: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendResult {
    pub remote_id: Option<String>,
}

pub trait MailRemote {
    fn list_mailboxes(&self) -> impl Future<Output = Result<Vec<RemoteMailbox>>> + Send;
    fn fetch_delta(
        &self,
        mailbox: MailboxId,
        cursor: SyncCursor,
    ) -> impl Future<Output = Result<RemoteDelta>> + Send;
    fn apply_ops(&self, ops: Vec<RemoteOp>) -> impl Future<Output = Result<()>> + Send;
    fn send_message(
        &self,
        message: OutgoingMessage,
    ) -> impl Future<Output = Result<SendResult>> + Send;
}

pub struct ImapSmtpRemote;
pub struct GmailRemote;
pub struct OutlookRemote;
pub struct JmapRemote;

#[derive(Debug, Clone, Default)]
pub struct NoopRemote {
    applied_ops: Arc<Mutex<Vec<RemoteOp>>>,
    deltas: Arc<Mutex<VecDeque<RemoteDelta>>>,
    sent_messages: Arc<Mutex<Vec<OutgoingMessage>>>,
}

impl NoopRemote {
    pub fn with_delta(delta: RemoteDelta) -> Self {
        let remote = Self::default();
        remote.push_delta(delta);
        remote
    }

    pub fn push_delta(&self, delta: RemoteDelta) {
        if let Ok(mut deltas) = self.deltas.lock() {
            deltas.push_back(delta);
        }
    }

    pub fn applied_ops(&self) -> Vec<RemoteOp> {
        self.applied_ops
            .lock()
            .map(|ops| ops.clone())
            .unwrap_or_default()
    }

    pub fn sent_messages(&self) -> Vec<OutgoingMessage> {
        self.sent_messages
            .lock()
            .map(|messages| messages.clone())
            .unwrap_or_default()
    }
}

impl MailRemote for NoopRemote {
    fn list_mailboxes(&self) -> impl Future<Output = Result<Vec<RemoteMailbox>>> + Send {
        async { Ok(Vec::new()) }
    }

    fn fetch_delta(
        &self,
        _mailbox: MailboxId,
        cursor: SyncCursor,
    ) -> impl Future<Output = Result<RemoteDelta>> + Send {
        let deltas = self.deltas.clone();

        async move {
            if let Ok(mut deltas) = deltas.lock() {
                if let Some(delta) = deltas.pop_front() {
                    return Ok(delta);
                }
            }

            Ok(RemoteDelta::empty(cursor))
        }
    }

    fn apply_ops(&self, ops: Vec<RemoteOp>) -> impl Future<Output = Result<()>> + Send {
        let applied_ops = self.applied_ops.clone();

        async move {
            if let Ok(mut applied) = applied_ops.lock() {
                applied.extend(ops);
            }
            Ok(())
        }
    }

    fn send_message(
        &self,
        message: OutgoingMessage,
    ) -> impl Future<Output = Result<SendResult>> + Send {
        let sent_messages = self.sent_messages.clone();

        async move {
            let remote_id = if let Ok(mut sent) = sent_messages.lock() {
                sent.push(message);
                Some(format!("noop-sent-{}", sent.len()))
            } else {
                None
            };

            Ok(SendResult { remote_id })
        }
    }
}
