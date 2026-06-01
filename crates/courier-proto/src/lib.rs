use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AccountId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MailboxId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ThreadId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UnifiedThreadId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DraftId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AttachmentId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LabelId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IdentityId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthType {
    Password,
    OAuth2,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProviderKind {
    GenericImap,
    Gmail,
    Outlook,
    Jmap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MailboxRole {
    Inbox,
    Sent,
    Drafts,
    Archive,
    Trash,
    Spam,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSummary {
    pub id: AccountId,
    pub email: String,
    pub provider: ProviderKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountConfig {
    pub id: AccountId,
    pub email: String,
    pub provider: ProviderKind,
    pub imap_host: String,
    pub imap_port: u16,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub auth_type: AuthType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountState {
    pub id: AccountId,
    pub email: String,
    pub provider: ProviderKind,
    pub imap_host: String,
    pub imap_port: u16,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub auth_type: AuthType,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityConfig {
    pub id: IdentityId,
    pub account_id: AccountId,
    pub name: String,
    pub email: String,
    pub reply_to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentitySummary {
    pub id: IdentityId,
    pub account_id: AccountId,
    pub name: String,
    pub email: String,
    pub reply_to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailboxSummary {
    pub id: MailboxId,
    pub account_id: AccountId,
    pub name: String,
    pub role: MailboxRole,
    pub unread_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadSummary {
    pub id: ThreadId,
    pub account_id: AccountId,
    pub subject: String,
    pub sender: String,
    pub snippet: String,
    pub unread: bool,
    pub last_message_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageBody {
    pub id: MessageId,
    pub thread_id: ThreadId,
    pub subject: String,
    pub from: String,
    pub to: Vec<String>,
    pub content_type: String,
    pub body: String,
    pub attachments: Vec<AttachmentSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentSummary {
    pub id: AttachmentId,
    pub filename: String,
    pub mime_type: String,
    pub size: u64,
    pub blob_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftMessage {
    pub id: DraftId,
    pub account_id: AccountId,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub body: String,
    pub attachments: Vec<AttachmentId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EngineCommand {
    SyncNow(AccountId),
    ListThreads {
        mailbox_id: Option<MailboxId>,
        query: String,
    },
    LoadThread(ThreadId),
    MarkRead(MessageId, bool),
    ArchiveMessage(MessageId),
    MoveToTrash(MessageId),
    SaveAccount(AccountConfig),
    SetAccountEnabled(AccountId, bool),
    DeleteAccount(AccountId),
    SaveIdentity(IdentityConfig),
    DeleteIdentity(IdentityId),
    SendMessage(DraftId),
    SaveDraft(DraftMessage),
    Snooze(MessageId, i64),
    Search(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EngineEvent {
    Ready,
    AccountsUpdated(Vec<AccountState>),
    IdentitiesUpdated(Vec<IdentitySummary>),
    IdentitySaved(IdentitySummary),
    MailboxesUpdated(Vec<MailboxSummary>),
    AccountSaved(AccountSummary),
    SyncProgress {
        account_id: AccountId,
        progress: f32,
    },
    NewMessages {
        mailbox_id: MailboxId,
        messages: Vec<MessageId>,
    },
    ThreadsUpdated(Vec<ThreadSummary>),
    MessageLoaded(MessageBody),
    SendResult {
        task_id: TaskId,
        result: Result<(), String>,
    },
    Error(String),
}
