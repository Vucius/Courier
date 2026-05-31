use std::path::PathBuf;

use courier_proto::{
    AccountId, AttachmentId, DraftId, IdentityId, LabelId, MailboxId, MessageId, ProviderKind,
    ThreadId, UnifiedThreadId,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Address {
    pub name: Option<String>,
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub id: IdentityId,
    pub account_id: AccountId,
    pub name: String,
    pub email: String,
    pub reply_to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSyncState {
    pub enabled: bool,
    pub last_sync_at: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: AccountId,
    pub email: String,
    pub provider: ProviderKind,
    pub identities: Vec<Identity>,
    pub sync_state: AccountSyncState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
pub struct Mailbox {
    pub id: MailboxId,
    pub account_id: AccountId,
    pub name: String,
    pub role: MailboxRole,
    pub unread_count: u32,
    pub total_count: u32,
    pub sync_cursor: SyncCursor,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageFlags {
    pub read: bool,
    pub starred: bool,
    pub archived: bool,
    pub deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictState {
    None,
    LocalPending,
    RemoteRejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: MessageId,
    pub account_id: AccountId,
    pub thread_id: ThreadId,
    pub message_id_header: String,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub from: Address,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub subject: String,
    pub snippet: String,
    pub timestamp: i64,
    pub flags: MessageFlags,
    pub has_attachments: bool,
    pub labels: Vec<LabelId>,
    pub conflict_state: ConflictState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountThread {
    pub id: ThreadId,
    pub account_id: AccountId,
    pub provider_thread_id: Option<String>,
    pub message_ids: Vec<MessageId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedThread {
    pub id: UnifiedThreadId,
    pub accounts: Vec<AccountId>,
    pub message_ids: Vec<MessageId>,
    pub subject: String,
    pub participants: Vec<Address>,
    pub last_message_ts: i64,
    pub unread_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub id: AttachmentId,
    pub message_id: MessageId,
    pub filename: String,
    pub mime_type: String,
    pub size: u64,
    pub blob_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DraftState {
    Local,
    Queued,
    Sending,
    Sent,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Draft {
    pub id: DraftId,
    pub account_id: AccountId,
    pub state: DraftState,
    pub reply_to: Option<MessageId>,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub bcc: Vec<Address>,
    pub subject: String,
    pub body: String,
    pub attachments: Vec<AttachmentId>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncCursor {
    pub uid_validity: u32,
    pub last_uid: u32,
    pub highest_modseq: Option<u64>,
}

impl AccountThread {
    pub fn merge_message(&mut self, msg: &Message) -> bool {
        if self.message_ids.contains(&msg.id) {
            return false;
        }

        self.message_ids.push(msg.id.clone());
        true
    }

    pub fn recount_unread(&self, messages: &[Message]) -> u32 {
        messages
            .iter()
            .filter(|message| self.message_ids.contains(&message.id) && !message.flags.read)
            .count() as u32
    }
}

impl Message {
    pub fn mark_read(&mut self) -> bool {
        if self.flags.read {
            return false;
        }

        self.flags.read = true;
        self.conflict_state = ConflictState::LocalPending;
        true
    }

    pub fn toggle_star(&mut self) -> bool {
        self.flags.starred = !self.flags.starred;
        self.conflict_state = ConflictState::LocalPending;
        true
    }
}

impl Mailbox {
    pub fn map_role_from_imap(imap_attrs: &[&str]) -> MailboxRole {
        if imap_attrs.iter().any(|attr| attr.eq_ignore_ascii_case("\\Inbox")) {
            MailboxRole::Inbox
        } else if imap_attrs
            .iter()
            .any(|attr| attr.eq_ignore_ascii_case("\\Sent"))
        {
            MailboxRole::Sent
        } else if imap_attrs
            .iter()
            .any(|attr| attr.eq_ignore_ascii_case("\\Drafts"))
        {
            MailboxRole::Drafts
        } else if imap_attrs
            .iter()
            .any(|attr| attr.eq_ignore_ascii_case("\\Trash"))
        {
            MailboxRole::Trash
        } else if imap_attrs.iter().any(|attr| {
            attr.eq_ignore_ascii_case("\\Junk") || attr.eq_ignore_ascii_case("\\Spam")
        }) {
            MailboxRole::Spam
        } else if imap_attrs
            .iter()
            .any(|attr| attr.eq_ignore_ascii_case("\\Archive"))
        {
            MailboxRole::Archive
        } else {
            MailboxRole::Custom
        }
    }
}

impl SyncCursor {
    pub fn advance(&mut self, new_uid: u32, modseq: Option<u64>) {
        self.last_uid = self.last_uid.max(new_uid);
        self.highest_modseq = match (self.highest_modseq, modseq) {
            (Some(current), Some(next)) => Some(current.max(next)),
            (None, Some(next)) => Some(next),
            (current, None) => current,
        };
    }

    pub fn validity_changed(&self, server_validity: u32) -> bool {
        self.uid_validity != 0 && self.uid_validity != server_validity
    }
}
