#![allow(clippy::manual_async_fn)]

use std::collections::VecDeque;
use std::future::Future;
use std::sync::{Arc, Mutex};

use courier_domain::SyncCursor;
use courier_proto::{AccountConfig, AuthType, MailboxId, MessageId, ProviderKind, ThreadId};
use courier_provider::{
    AuthMechanism, ProviderCapabilities, ProviderEndpoint, TransportSecurity, capabilities_for,
};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("remote capability is not implemented: {0}")]
    NotImplemented(&'static str),
    #[error("provider endpoint is missing: {0}")]
    MissingEndpoint(&'static str),
    #[error("account auth is not supported by provider: {0}")]
    UnsupportedAuth(&'static str),
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
    pub deleted_messages: Vec<MessageId>,
    pub moved_messages: Vec<RemoteMove>,
}

impl RemoteDelta {
    pub fn empty(cursor: SyncCursor) -> Self {
        Self {
            cursor,
            messages: Vec::new(),
            deleted_messages: Vec::new(),
            moved_messages: Vec::new(),
        }
    }

    pub fn new_message_count(&self) -> usize {
        self.messages.len()
    }

    pub fn deleted_message_count(&self) -> usize {
        self.deleted_messages.len()
    }

    pub fn moved_message_count(&self) -> usize {
        self.moved_messages.len()
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
pub struct RemoteMove {
    pub message_id: MessageId,
    pub target_mailbox_id: MailboxId,
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
    plan: ProtocolConnectionPlan,
}

#[derive(Debug, Clone)]
pub struct GmailRemote {
    account: AccountConfig,
    plan: ProtocolConnectionPlan,
}

#[derive(Debug, Clone)]
pub struct OutlookRemote {
    account: AccountConfig,
    plan: ProtocolConnectionPlan,
}

#[derive(Debug, Clone)]
pub struct JmapRemote {
    account: AccountConfig,
    plan: ProtocolConnectionPlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolTransport {
    ImapSmtp,
    GmailImapSmtp,
    OutlookImapSmtp,
    Jmap,
}

#[derive(Debug, Clone)]
pub struct ProtocolConnectionPlan {
    pub provider: ProviderKind,
    pub transport: ProtocolTransport,
    pub capabilities: ProviderCapabilities,
    pub imap_endpoint: Option<ProviderEndpoint>,
    pub smtp_endpoint: Option<ProviderEndpoint>,
    pub jmap_endpoint: Option<ProviderEndpoint>,
    pub auth_mechanisms: Vec<AuthMechanism>,
}

fn auth_supported(account: &AccountConfig, auth_mechanisms: &[AuthMechanism]) -> bool {
    match &account.auth_type {
        AuthType::Password => auth_mechanisms.contains(&AuthMechanism::Password),
        AuthType::OAuth2 => auth_mechanisms
            .iter()
            .any(|mechanism| !matches!(mechanism, AuthMechanism::Password)),
    }
}

fn endpoint_from_account_imap(account: &AccountConfig) -> Result<Option<ProviderEndpoint>> {
    if account.imap_host.trim().is_empty() {
        return Err(Error::MissingEndpoint("imap"));
    }

    Ok(Some(ProviderEndpoint {
        host: account.imap_host.trim().to_string(),
        port: account.imap_port,
        security: if account.imap_port == 143 {
            TransportSecurity::StartTls
        } else {
            TransportSecurity::Tls
        },
    }))
}

fn endpoint_from_account_smtp(account: &AccountConfig) -> Result<Option<ProviderEndpoint>> {
    if account.smtp_host.trim().is_empty() {
        return Err(Error::MissingEndpoint("smtp"));
    }

    Ok(Some(ProviderEndpoint {
        host: account.smtp_host.trim().to_string(),
        port: account.smtp_port,
        security: if account.smtp_port == 465 {
            TransportSecurity::Tls
        } else {
            TransportSecurity::StartTls
        },
    }))
}

fn fallback_plan(account: &AccountConfig, transport: ProtocolTransport) -> ProtocolConnectionPlan {
    let capabilities = capabilities_for(&account.provider);
    ProtocolConnectionPlan {
        provider: account.provider.clone(),
        transport,
        imap_endpoint: capabilities.imap_endpoint.clone(),
        smtp_endpoint: capabilities.smtp_endpoint.clone(),
        jmap_endpoint: capabilities.jmap_endpoint.clone(),
        auth_mechanisms: capabilities.auth_mechanisms.clone(),
        capabilities,
    }
}

impl ProtocolConnectionPlan {
    pub fn from_account(account: &AccountConfig) -> Result<Self> {
        let capabilities = capabilities_for(&account.provider);
        let auth_mechanisms = capabilities.auth_mechanisms.clone();
        if !auth_supported(account, &auth_mechanisms) {
            return Err(Error::UnsupportedAuth(match account.provider {
                ProviderKind::GenericImap => "generic imap",
                ProviderKind::Gmail => "gmail",
                ProviderKind::Outlook => "outlook",
                ProviderKind::Jmap => "jmap",
            }));
        }

        let imap_endpoint = match account.provider {
            ProviderKind::GenericImap => endpoint_from_account_imap(account)?,
            _ => capabilities.imap_endpoint.clone(),
        };
        let smtp_endpoint = match account.provider {
            ProviderKind::GenericImap => endpoint_from_account_smtp(account)?,
            _ => capabilities.smtp_endpoint.clone(),
        };
        let jmap_endpoint = capabilities.jmap_endpoint.clone();
        let transport = match account.provider {
            ProviderKind::GenericImap => ProtocolTransport::ImapSmtp,
            ProviderKind::Gmail => ProtocolTransport::GmailImapSmtp,
            ProviderKind::Outlook => ProtocolTransport::OutlookImapSmtp,
            ProviderKind::Jmap => ProtocolTransport::Jmap,
        };

        Ok(Self {
            provider: account.provider.clone(),
            transport,
            capabilities,
            imap_endpoint,
            smtp_endpoint,
            jmap_endpoint,
            auth_mechanisms,
        })
    }

    pub fn requires_oauth2(&self) -> bool {
        !self.capabilities.supports_password && self.capabilities.supports_oauth2
    }
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
        let plan = ProtocolConnectionPlan::from_account(&account)
            .unwrap_or_else(|_| fallback_plan(&account, ProtocolTransport::ImapSmtp));
        Self { account, plan }
    }

    pub fn account(&self) -> &AccountConfig {
        &self.account
    }

    pub fn plan(&self) -> &ProtocolConnectionPlan {
        &self.plan
    }
}

impl GmailRemote {
    pub fn new(account: AccountConfig) -> Self {
        let plan = ProtocolConnectionPlan::from_account(&account)
            .unwrap_or_else(|_| fallback_plan(&account, ProtocolTransport::GmailImapSmtp));
        Self { account, plan }
    }

    pub fn account(&self) -> &AccountConfig {
        &self.account
    }

    pub fn plan(&self) -> &ProtocolConnectionPlan {
        &self.plan
    }
}

impl OutlookRemote {
    pub fn new(account: AccountConfig) -> Self {
        let plan = ProtocolConnectionPlan::from_account(&account)
            .unwrap_or_else(|_| fallback_plan(&account, ProtocolTransport::OutlookImapSmtp));
        Self { account, plan }
    }

    pub fn account(&self) -> &AccountConfig {
        &self.account
    }

    pub fn plan(&self) -> &ProtocolConnectionPlan {
        &self.plan
    }
}

impl JmapRemote {
    pub fn new(account: AccountConfig) -> Self {
        let plan = ProtocolConnectionPlan::from_account(&account)
            .unwrap_or_else(|_| fallback_plan(&account, ProtocolTransport::Jmap));
        Self { account, plan }
    }

    pub fn account(&self) -> &AccountConfig {
        &self.account
    }

    pub fn plan(&self) -> &ProtocolConnectionPlan {
        &self.plan
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
            if let Ok(mut deltas) = deltas.lock()
                && let Some(delta) = deltas.pop_front()
            {
                return Ok(delta);
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

    #[test]
    fn generic_imap_plan_uses_account_endpoints() {
        let account = account_config(ProviderKind::GenericImap);
        let plan = ProtocolConnectionPlan::from_account(&account).expect("connection plan");

        assert!(matches!(plan.transport, ProtocolTransport::ImapSmtp));
        assert_eq!(
            plan.imap_endpoint.as_ref().expect("imap endpoint").host,
            "imap.example.test"
        );
        assert_eq!(
            plan.smtp_endpoint.as_ref().expect("smtp endpoint").host,
            "smtp.example.test"
        );
        assert!(!plan.requires_oauth2());
    }

    #[test]
    fn gmail_plan_requires_oauth2() {
        let mut account = account_config(ProviderKind::Gmail);
        account.auth_type = AuthType::OAuth2;
        let plan = ProtocolConnectionPlan::from_account(&account).expect("gmail plan");

        assert!(matches!(plan.transport, ProtocolTransport::GmailImapSmtp));
        assert!(plan.requires_oauth2());
        assert_eq!(
            plan.imap_endpoint.as_ref().expect("gmail imap").host,
            "imap.gmail.com"
        );
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
