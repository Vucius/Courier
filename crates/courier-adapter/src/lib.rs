use std::collections::VecDeque;
use std::future::Future;
use std::sync::{Arc, Mutex};

use courier_domain::SyncCursor;
use courier_proto::{AccountConfig, MailboxId, MessageId, ProviderKind, ThreadId};

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

#[derive(Debug, Clone)]
pub struct ImapSmtpRemote {
    account: AccountConfig,
}

#[derive(Debug, Clone)]
pub struct GmailRemote {
    account: AccountConfig,
}

#[derive(Debug, Clone)]
pub struct OutlookRemote {
    account: AccountConfig,
}

#[derive(Debug, Clone)]
pub struct JmapRemote {
    account: AccountConfig,
}

#[derive(Debug, Clone)]
pub enum ConfiguredRemote {
    LocalNoop(NoopRemote),
    ImapSmtp(ImapSmtpRemote),
    Gmail(GmailRemote),
    Outlook(OutlookRemote),
    Jmap(JmapRemote),
}

impl ConfiguredRemote {
    pub fn local_noop() -> Self {
        Self::LocalNoop(NoopRemote::default())
    }

    pub fn from_account_config(account: AccountConfig) -> Self {
        match account.provider {
            ProviderKind::GenericImap => Self::ImapSmtp(ImapSmtpRemote::new(account)),
            ProviderKind::Gmail => Self::Gmail(GmailRemote::new(account)),
            ProviderKind::Outlook => Self::Outlook(OutlookRemote::new(account)),
            ProviderKind::Jmap => Self::Jmap(JmapRemote::new(account)),
        }
    }
}

impl ImapSmtpRemote {
    pub fn new(account: AccountConfig) -> Self {
        Self { account }
    }

    pub fn account(&self) -> &AccountConfig {
        &self.account
    }
}

impl GmailRemote {
    pub fn new(account: AccountConfig) -> Self {
        Self { account }
    }

    pub fn account(&self) -> &AccountConfig {
        &self.account
    }
}

impl OutlookRemote {
    pub fn new(account: AccountConfig) -> Self {
        Self { account }
    }

    pub fn account(&self) -> &AccountConfig {
        &self.account
    }
}

impl JmapRemote {
    pub fn new(account: AccountConfig) -> Self {
        Self { account }
    }

    pub fn account(&self) -> &AccountConfig {
        &self.account
    }
}

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

impl MailRemote for ConfiguredRemote {
    fn list_mailboxes(&self) -> impl Future<Output = Result<Vec<RemoteMailbox>>> + Send {
        let remote = self.clone();
        async move {
            match remote {
                ConfiguredRemote::LocalNoop(remote) => remote.list_mailboxes().await,
                ConfiguredRemote::ImapSmtp(remote) => remote.list_mailboxes().await,
                ConfiguredRemote::Gmail(remote) => remote.list_mailboxes().await,
                ConfiguredRemote::Outlook(remote) => remote.list_mailboxes().await,
                ConfiguredRemote::Jmap(remote) => remote.list_mailboxes().await,
            }
        }
    }

    fn fetch_delta(
        &self,
        mailbox: MailboxId,
        cursor: SyncCursor,
    ) -> impl Future<Output = Result<RemoteDelta>> + Send {
        let remote = self.clone();
        async move {
            match remote {
                ConfiguredRemote::LocalNoop(remote) => remote.fetch_delta(mailbox, cursor).await,
                ConfiguredRemote::ImapSmtp(remote) => remote.fetch_delta(mailbox, cursor).await,
                ConfiguredRemote::Gmail(remote) => remote.fetch_delta(mailbox, cursor).await,
                ConfiguredRemote::Outlook(remote) => remote.fetch_delta(mailbox, cursor).await,
                ConfiguredRemote::Jmap(remote) => remote.fetch_delta(mailbox, cursor).await,
            }
        }
    }

    fn apply_ops(&self, ops: Vec<RemoteOp>) -> impl Future<Output = Result<()>> + Send {
        let remote = self.clone();
        async move {
            match remote {
                ConfiguredRemote::LocalNoop(remote) => remote.apply_ops(ops).await,
                ConfiguredRemote::ImapSmtp(remote) => remote.apply_ops(ops).await,
                ConfiguredRemote::Gmail(remote) => remote.apply_ops(ops).await,
                ConfiguredRemote::Outlook(remote) => remote.apply_ops(ops).await,
                ConfiguredRemote::Jmap(remote) => remote.apply_ops(ops).await,
            }
        }
    }

    fn send_message(
        &self,
        message: OutgoingMessage,
    ) -> impl Future<Output = Result<SendResult>> + Send {
        let remote = self.clone();
        async move {
            match remote {
                ConfiguredRemote::LocalNoop(remote) => remote.send_message(message).await,
                ConfiguredRemote::ImapSmtp(remote) => remote.send_message(message).await,
                ConfiguredRemote::Gmail(remote) => remote.send_message(message).await,
                ConfiguredRemote::Outlook(remote) => remote.send_message(message).await,
                ConfiguredRemote::Jmap(remote) => remote.send_message(message).await,
            }
        }
    }
}

impl MailRemote for ImapSmtpRemote {
    fn list_mailboxes(&self) -> impl Future<Output = Result<Vec<RemoteMailbox>>> + Send {
        async { Err(Error::NotImplemented("imap/smtp list_mailboxes")) }
    }

    fn fetch_delta(
        &self,
        mailbox: MailboxId,
        cursor: SyncCursor,
    ) -> impl Future<Output = Result<RemoteDelta>> + Send {
        async move {
            let _ = (mailbox, cursor);
            Err(Error::NotImplemented("imap fetch_delta"))
        }
    }

    fn apply_ops(&self, ops: Vec<RemoteOp>) -> impl Future<Output = Result<()>> + Send {
        async move {
            let _ = ops;
            Err(Error::NotImplemented("imap/smtp apply_ops"))
        }
    }

    fn send_message(
        &self,
        message: OutgoingMessage,
    ) -> impl Future<Output = Result<SendResult>> + Send {
        async move {
            let _ = message;
            Err(Error::NotImplemented("smtp send_message"))
        }
    }
}

impl MailRemote for GmailRemote {
    fn list_mailboxes(&self) -> impl Future<Output = Result<Vec<RemoteMailbox>>> + Send {
        async { Err(Error::NotImplemented("gmail list_mailboxes")) }
    }

    fn fetch_delta(
        &self,
        mailbox: MailboxId,
        cursor: SyncCursor,
    ) -> impl Future<Output = Result<RemoteDelta>> + Send {
        async move {
            let _ = (mailbox, cursor);
            Err(Error::NotImplemented("gmail fetch_delta"))
        }
    }

    fn apply_ops(&self, ops: Vec<RemoteOp>) -> impl Future<Output = Result<()>> + Send {
        async move {
            let _ = ops;
            Err(Error::NotImplemented("gmail apply_ops"))
        }
    }

    fn send_message(
        &self,
        message: OutgoingMessage,
    ) -> impl Future<Output = Result<SendResult>> + Send {
        async move {
            let _ = message;
            Err(Error::NotImplemented("gmail send_message"))
        }
    }
}

impl MailRemote for OutlookRemote {
    fn list_mailboxes(&self) -> impl Future<Output = Result<Vec<RemoteMailbox>>> + Send {
        async { Err(Error::NotImplemented("outlook list_mailboxes")) }
    }

    fn fetch_delta(
        &self,
        mailbox: MailboxId,
        cursor: SyncCursor,
    ) -> impl Future<Output = Result<RemoteDelta>> + Send {
        async move {
            let _ = (mailbox, cursor);
            Err(Error::NotImplemented("outlook fetch_delta"))
        }
    }

    fn apply_ops(&self, ops: Vec<RemoteOp>) -> impl Future<Output = Result<()>> + Send {
        async move {
            let _ = ops;
            Err(Error::NotImplemented("outlook apply_ops"))
        }
    }

    fn send_message(
        &self,
        message: OutgoingMessage,
    ) -> impl Future<Output = Result<SendResult>> + Send {
        async move {
            let _ = message;
            Err(Error::NotImplemented("outlook send_message"))
        }
    }
}

impl MailRemote for JmapRemote {
    fn list_mailboxes(&self) -> impl Future<Output = Result<Vec<RemoteMailbox>>> + Send {
        async { Err(Error::NotImplemented("jmap list_mailboxes")) }
    }

    fn fetch_delta(
        &self,
        mailbox: MailboxId,
        cursor: SyncCursor,
    ) -> impl Future<Output = Result<RemoteDelta>> + Send {
        async move {
            let _ = (mailbox, cursor);
            Err(Error::NotImplemented("jmap fetch_delta"))
        }
    }

    fn apply_ops(&self, ops: Vec<RemoteOp>) -> impl Future<Output = Result<()>> + Send {
        async move {
            let _ = ops;
            Err(Error::NotImplemented("jmap apply_ops"))
        }
    }

    fn send_message(
        &self,
        message: OutgoingMessage,
    ) -> impl Future<Output = Result<SendResult>> + Send {
        async move {
            let _ = message;
            Err(Error::NotImplemented("jmap send_message"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use courier_proto::{AccountId, AuthType};

    #[test]
    fn configured_remote_selects_generic_imap() {
        let account = account_config(ProviderKind::GenericImap);
        let remote = ConfiguredRemote::from_account_config(account.clone());

        match remote {
            ConfiguredRemote::ImapSmtp(remote) => {
                assert_eq!(remote.account().id, account.id);
                assert_eq!(remote.account().imap_host, "imap.example.test");
                assert_eq!(remote.account().smtp_host, "smtp.example.test");
            }
            other => panic!("expected imap/smtp remote, got {other:?}"),
        }
    }

    #[test]
    fn configured_remote_selects_provider_specific_variants() {
        assert!(matches!(
            ConfiguredRemote::from_account_config(account_config(ProviderKind::Gmail)),
            ConfiguredRemote::Gmail(_)
        ));
        assert!(matches!(
            ConfiguredRemote::from_account_config(account_config(ProviderKind::Outlook)),
            ConfiguredRemote::Outlook(_)
        ));
        assert!(matches!(
            ConfiguredRemote::from_account_config(account_config(ProviderKind::Jmap)),
            ConfiguredRemote::Jmap(_)
        ));
    }

    fn account_config(provider: ProviderKind) -> AccountConfig {
        AccountConfig {
            id: AccountId("account:test".to_string()),
            email: "test@example.test".to_string(),
            provider,
            imap_host: "imap.example.test".to_string(),
            imap_port: 993,
            smtp_host: "smtp.example.test".to_string(),
            smtp_port: 587,
            auth_type: AuthType::Password,
        }
    }
}
