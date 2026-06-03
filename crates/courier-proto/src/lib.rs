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
pub struct EndpointCheckResult {
    pub host: String,
    pub port: u16,
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountConnectionTestResult {
    pub account_id: AccountId,
    pub imap: EndpointCheckResult,
    pub smtp: EndpointCheckResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2ClientConfig {
    pub provider: ProviderKind,
    pub client_id: String,
    pub auth_url: String,
    pub token_url: String,
    pub scopes: Vec<String>,
    pub redirect_uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2AuthorizationRequest {
    pub account_id: AccountId,
    pub provider: ProviderKind,
    pub auth_url: String,
    pub redirect_uri: String,
    pub state: String,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2Callback {
    pub account_id: AccountId,
    pub code: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CredentialKind {
    Password,
    OAuthAccessToken,
    OAuthRefreshToken,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialRef {
    pub account_id: AccountId,
    pub kind: CredentialKind,
    pub service: String,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialStoreStatus {
    pub available: bool,
    pub backend: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialSecret {
    pub reference: CredentialRef,
    pub secret: String,
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
    pub content_id: Option<String>,
    pub inline: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AttachmentPreviewKind {
    Text,
    Image,
    Unsupported,
    MissingBlob,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentPreview {
    pub attachment: AttachmentSummary,
    pub kind: AttachmentPreviewKind,
    pub content: Option<String>,
    pub path: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentOpenRequest {
    pub attachment: AttachmentSummary,
    pub path: Option<String>,
    pub allowed: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttachmentTransferStatus {
    Ready,
    Missing,
    Downloading,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentTransfer {
    pub attachment: AttachmentSummary,
    pub status: AttachmentTransferStatus,
    pub progress: f32,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendQueueItem {
    pub task_id: TaskId,
    pub draft_id: DraftId,
    pub account_id: AccountId,
    pub to: Vec<String>,
    pub subject: String,
    pub status: String,
    pub retry_count: u32,
    pub last_error: Option<String>,
    pub run_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictResolution {
    KeepLocal,
    AcceptRemote,
    RequeueLocal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictSummary {
    pub message_id: MessageId,
    pub thread_id: ThreadId,
    pub subject: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotificationKind {
    NewMail,
    Sync,
    Send,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopNotification {
    pub id: String,
    pub kind: NotificationKind,
    pub title: String,
    pub body: String,
    pub account_id: Option<AccountId>,
    pub message_ids: Vec<MessageId>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStatus {
    pub online: bool,
    pub reason: String,
    pub checked_at: i64,
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
    TestAccountConnection(AccountConfig),
    BeginOAuth2(AccountId),
    CompleteOAuth2(OAuth2Callback),
    CredentialStatus,
    SaveCredentialSecret(CredentialSecret),
    SaveIdentity(IdentityConfig),
    DeleteIdentity(IdentityId),
    SendMessage(DraftId),
    SaveDraft(DraftMessage),
    ListSendQueue,
    RetrySend(DraftId),
    CancelSend(DraftId),
    RunDueSendQueue,
    PreviewAttachment(AttachmentId),
    OpenAttachment(AttachmentId),
    ConfirmOpenAttachment(AttachmentId),
    DownloadAttachment(AttachmentId),
    CancelAttachmentDownload(AttachmentId),
    RetryAttachmentDownload(AttachmentId),
    SetNetworkOnline(bool),
    ListConflicts,
    ResolveConflict(MessageId, ConflictResolution),
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
    AccountConnectionTested(AccountConnectionTestResult),
    OAuth2AuthorizationStarted(Result<OAuth2AuthorizationRequest, String>),
    OAuth2Completed(Result<CredentialRef, String>),
    CredentialStoreChecked(CredentialStoreStatus),
    CredentialSaved(Result<CredentialRef, String>),
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
    AttachmentPreviewLoaded(Result<AttachmentPreview, String>),
    AttachmentOpenPrepared(AttachmentOpenRequest),
    AttachmentOpenExecuted(Result<AttachmentOpenRequest, String>),
    AttachmentTransfersUpdated(Vec<AttachmentTransfer>),
    SendQueueUpdated(Vec<SendQueueItem>),
    NetworkStatusChanged(NetworkStatus),
    ConflictsUpdated(Vec<ConflictSummary>),
    NotificationRaised(DesktopNotification),
    SendResult {
        task_id: TaskId,
        result: Result<(), String>,
    },
    Error(String),
}
