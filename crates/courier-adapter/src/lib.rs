use std::future::Future;

use courier_domain::SyncCursor;
use courier_proto::MailboxId;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("remote capability is not implemented: {0}")]
    NotImplemented(&'static str),
}

#[derive(Debug, Clone)]
pub struct RemoteMailbox {
    pub id: MailboxId,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct RemoteDelta {
    pub cursor: SyncCursor,
    pub new_message_count: usize,
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct OutgoingMessage {
    pub rfc822: Vec<u8>,
}

#[derive(Debug, Clone)]
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
