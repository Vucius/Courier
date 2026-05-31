use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AccountId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FolderId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ThreadId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DraftId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthType {
    Password,
    OAuth2,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FolderRole {
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
    pub provider: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderSummary {
    pub id: FolderId,
    pub account_id: AccountId,
    pub name: String,
    pub role: FolderRole,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftMessage {
    pub id: DraftId,
    pub account_id: AccountId,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EngineCommand {
    SyncNow(AccountId),
    MarkRead(MessageId, bool),
    SendMessage(DraftId),
    SaveDraft(DraftMessage),
    Snooze(MessageId, i64),
    Search(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EngineEvent {
    Ready,
    SyncProgress {
        account_id: AccountId,
        progress: f32,
    },
    NewMessages {
        folder_id: FolderId,
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
