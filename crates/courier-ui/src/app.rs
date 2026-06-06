use std::path::PathBuf;

use courier_app::{EngineConfig, EngineHandle, spawn_engine};
use courier_proto::{
    AccountConfig, AccountId, AccountState, AttachmentId, AttachmentOpenRequest, AttachmentPreview,
    AttachmentTransfer, AuthType, ConflictResolution, ConflictSummary, CredentialKind,
    CredentialRef, CredentialSecret, DesktopNotification, DraftId, DraftMessage, EngineCommand,
    EngineEvent, IdentityConfig, IdentityId, IdentitySummary, MailboxId, MailboxSummary,
    MessageBody, MessageId, NotificationKind, NotificationPolicyState, ProviderKind, SendQueueItem,
    ThreadId, ThreadSummary,
};
use courier_render::{RenderTree, render_tree_from_html, render_tree_from_text};
use iced::futures::SinkExt;
use iced::keyboard::{Key, Modifiers, key};
use iced::widget::{column, container, progress_bar, row, text};
use iced::{Element, Length, Subscription, Task, Theme};
use std::time::Duration;
use crate::components::icon::Icon;

#[derive(Debug, Clone)]
pub enum Message {
    SyncNow,
    SyncQueued,
    EngineEvent(EngineEvent),
    MailboxSelected(Option<MailboxId>, String),
    AddAccount,
    Compose,
    CloseCompose,
    ReplyInline,
    CloseInlineReply,
    CancelActivePanel,
    ArchiveSelected,
    MarkReadSelected,
    TrashSelected,
    SearchChanged(String),
    SelectThread(ThreadId),
    DraftToChanged(String),
    DraftSubjectChanged(String),
    DraftBodyChanged(String),
    SendDraft,
    RetrySend(DraftId),
    CancelSend(DraftId),
    PreviewAttachment(AttachmentId),
    OpenAttachment(AttachmentId),
    ConfirmOpenAttachment(AttachmentId),
    DownloadAttachment(AttachmentId),
    CancelAttachmentDownload(AttachmentId),
    RetryAttachmentDownload(AttachmentId),
    SetNetworkOnline(bool),
    ProbeNetwork,
    SetNotificationsQuiet(bool),
    SetNotificationsQuietFor(i64),
    DismissAttachmentNotice,
    ClearNotifications,
    ResolveConflict(MessageId, ConflictResolution),
    AccountEmailChanged(String),
    AccountImapHostChanged(String),
    AccountImapPortChanged(String),
    AccountSmtpHostChanged(String),
    AccountSmtpPortChanged(String),
    AccountPasswordChanged(String),
    SaveAccount,
    TestAccountConnection,
    BeginOAuth2(AccountId),
    EditAccount(AccountId),
    ToggleAccountEnabled(AccountId, bool),
    DeleteAccount(AccountId),
    IdentityNameChanged(String),
    IdentityEmailChanged(String),
    SaveIdentity,
    DeleteIdentity(IdentityId),
    SelectNextThread,
    SelectPreviousThread,
    OpenThreadContext(ThreadId),
    OpenSelectedThreadContext,
    CloseThreadContext,
    ToggleShortcutsHelp,
    UiTick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    Reader,
    Compose,
}

pub struct App {
    engine: EngineHandle,
    accounts: Vec<AccountState>,
    identities: Vec<IdentitySummary>,
    mailboxes: Vec<MailboxSummary>,
    threads: Vec<ThreadSummary>,
    selected_mailbox_id: Option<MailboxId>,
    selected_mailbox_name: String,
    selected_thread: Option<ThreadId>,
    selected_body: Option<MessageBody>,
    selected_render: Option<RenderTree>,
    attachment_preview: Option<AttachmentPreview>,
    attachment_open: Option<AttachmentOpenRequest>,
    attachment_transfers: Vec<AttachmentTransfer>,
    send_queue: Vec<SendQueueItem>,
    conflicts: Vec<ConflictSummary>,
    notifications: Vec<DesktopNotification>,
    unread_notifications: u32,
    notification_policy: NotificationPolicyState,
    network_online: bool,
    search_query: String,
    draft_to: String,
    draft_subject: String,
    draft_body: String,
    view_mode: ViewMode,
    inline_reply_open: bool,
    context_thread: Option<ThreadId>,
    transition_label: String,
    transition_ticks_remaining: u8,
    account_setup_visible: bool,
    shortcuts_help_visible: bool,
    editing_account_id: Option<AccountId>,
    account_email: String,
    account_imap_host: String,
    account_imap_port: String,
    account_smtp_host: String,
    account_smtp_port: String,
    account_password: String,
    identity_name: String,
    identity_email: String,
    account_connection_status: String,
    status: String,
}

pub fn init() -> (App, Task<Message>) {
    let data_dir = default_data_dir();
    let (engine, _join) = spawn_engine(EngineConfig { data_dir });

    let app = App {
        engine,
        accounts: Vec::new(),
        identities: Vec::new(),
        mailboxes: Vec::new(),
        threads: Vec::new(),
        selected_mailbox_id: None,
        selected_mailbox_name: "Unified Inbox".to_string(),
        selected_thread: None,
        selected_body: None,
        selected_render: None,
        attachment_preview: None,
        attachment_open: None,
        attachment_transfers: Vec::new(),
        send_queue: Vec::new(),
        conflicts: Vec::new(),
        notifications: Vec::new(),
        unread_notifications: 0,
        notification_policy: NotificationPolicyState {
            quiet: false,
            quiet_until: None,
            suppressed_count: 0,
            last_suppressed_at: None,
            reason: "Notifications enabled".to_string(),
        },
        network_online: true,
        search_query: String::new(),
        draft_to: String::new(),
        draft_subject: String::new(),
        draft_body: String::new(),
        view_mode: ViewMode::Reader,
        inline_reply_open: false,
        context_thread: None,
        transition_label: String::new(),
        transition_ticks_remaining: 0,
        account_setup_visible: false,
        shortcuts_help_visible: false,
        editing_account_id: None,
        account_email: String::new(),
        account_imap_host: String::new(),
        account_imap_port: "993".to_string(),
        account_smtp_host: String::new(),
        account_smtp_port: "587".to_string(),
        account_password: String::new(),
        identity_name: String::new(),
        identity_email: String::new(),
        account_connection_status: String::new(),
        status: "Engine starting".to_string(),
    };

    (app, Task::none())
}

pub fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::SyncNow => {
            let engine = app.engine.clone();
            let account_ids = enabled_account_ids(app);
            app.status = "Sync queued".to_string();
            Task::perform(
                async move {
                    for account_id in account_ids {
                        let _ = engine.send(EngineCommand::SyncNow(account_id)).await;
                    }
                },
                |_| Message::SyncQueued,
            )
        }
        Message::SyncQueued => {
            app.status = "Waiting for engine events".to_string();
            Task::none()
        }
        Message::EngineEvent(event) => handle_engine_event(app, event),
        Message::MailboxSelected(mailbox_id, name) => {
            app.account_setup_visible = false;
            app.selected_mailbox_id = mailbox_id.clone();
            app.selected_mailbox_name = name.clone();
            app.selected_body = None;
            app.selected_render = None;
            app.attachment_preview = None;
            app.attachment_open = None;
            app.selected_thread = None;
            app.view_mode = ViewMode::Reader;
            app.inline_reply_open = false;
            app.context_thread = None;
            start_view_transition(app, "Mailbox view");
            app.status = format!("{name} selected");

            let engine = app.engine.clone();
            let query = app.search_query.clone();
            Task::perform(
                async move {
                    let _ = engine
                        .send(EngineCommand::ListThreads { mailbox_id, query })
                        .await;
                },
                |_| Message::SyncQueued,
            )
        }
        Message::AddAccount => {
            app.account_setup_visible = true;
            app.editing_account_id = None;
            reset_account_form(app);
            app.selected_body = None;
            app.selected_render = None;
            app.attachment_preview = None;
            app.attachment_open = None;
            app.selected_thread = None;
            app.view_mode = ViewMode::Reader;
            app.inline_reply_open = false;
            app.context_thread = None;
            start_view_transition(app, "Account setup");
            app.status = "Account setup ready".to_string();
            Task::none()
        }
        Message::Compose => {
            app.account_setup_visible = false;
            app.view_mode = ViewMode::Compose;
            app.inline_reply_open = false;
            app.context_thread = None;
            start_view_transition(app, "Compose");
            app.status = "Draft ready".to_string();
            Task::none()
        }
        Message::CloseCompose => {
            app.view_mode = ViewMode::Reader;
            start_view_transition(app, "Reader");
            app.status = "Reading view ready".to_string();
            Task::none()
        }
        Message::ToggleShortcutsHelp => {
            app.shortcuts_help_visible = !app.shortcuts_help_visible;
            app.account_setup_visible = false;
            app.context_thread = None;
            start_view_transition(
                app,
                if app.shortcuts_help_visible { "Shortcuts help" } else { "Reader" },
            );
            Task::none()
        }
        Message::ReplyInline => {
            if let Some(body) = app.selected_body.as_ref() {
                let reply_to = body.from.clone();
                let subject = reply_subject(&body.subject);
                app.account_setup_visible = false;
                app.view_mode = ViewMode::Reader;
                app.inline_reply_open = true;
                app.context_thread = None;
                start_view_transition(app, "Inline reply");
                app.draft_to = reply_to;
                app.draft_subject = subject;
                app.status = "Reply ready".to_string();
            } else {
                app.status = "Select a message to reply".to_string();
            }
            Task::none()
        }
        Message::CloseInlineReply => {
            app.inline_reply_open = false;
            start_view_transition(app, "Reader");
            app.status = "Reply closed".to_string();
            Task::none()
        }
        Message::CancelActivePanel => {
            if app.shortcuts_help_visible {
                app.shortcuts_help_visible = false;
                start_view_transition(app, "Reader");
                app.status = "Shortcuts help closed".to_string();
            } else if app.account_setup_visible {
                app.account_setup_visible = false;
                start_view_transition(app, "Reader");
                app.status = "Account panel closed".to_string();
            } else if app.context_thread.is_some() {
                app.context_thread = None;
                start_view_transition(app, "Thread list");
                app.status = "Thread actions closed".to_string();
            } else if app.inline_reply_open {
                app.inline_reply_open = false;
                start_view_transition(app, "Reader");
                app.status = "Reply closed".to_string();
            } else if app.view_mode == ViewMode::Compose {
                app.view_mode = ViewMode::Reader;
                start_view_transition(app, "Reader");
                app.status = "Reading view ready".to_string();
            }
            Task::none()
        }
        Message::ArchiveSelected => {
            if app.view_mode == ViewMode::Reader && !app.account_setup_visible {
                if let Some(body) = app.selected_body.as_ref() {
                    let engine = app.engine.clone();
                    let message_id = body.id.clone();
                    app.selected_body = None;
                    app.selected_render = None;
                    app.attachment_preview = None;
                    app.attachment_open = None;
                    app.selected_thread = None;
                    app.inline_reply_open = false;
                    app.context_thread = None;
                    app.status = "Archive queued".to_string();
                    Task::perform(
                        async move {
                            let _ = engine.send(EngineCommand::ArchiveMessage(message_id)).await;
                        },
                        |_| Message::SyncQueued,
                    )
                } else {
                    app.status = selected_action_status(app, "Archive");
                    Task::none()
                }
            } else {
                Task::none()
            }
        }
        Message::MarkReadSelected => {
            if app.view_mode == ViewMode::Reader && !app.account_setup_visible {
                if let Some(body) = app.selected_body.as_ref() {
                    let engine = app.engine.clone();
                    let message_id = body.id.clone();
                    app.status = "Mark read queued".to_string();
                    Task::perform(
                        async move {
                            let _ = engine.send(EngineCommand::MarkRead(message_id, true)).await;
                        },
                        |_| Message::SyncQueued,
                    )
                } else {
                    app.status = selected_action_status(app, "Mark read");
                    Task::none()
                }
            } else {
                Task::none()
            }
        }
        Message::TrashSelected => {
            if app.view_mode == ViewMode::Reader && !app.account_setup_visible {
                if let Some(body) = app.selected_body.as_ref() {
                    let engine = app.engine.clone();
                    let message_id = body.id.clone();
                    app.selected_body = None;
                    app.selected_render = None;
                    app.attachment_preview = None;
                    app.attachment_open = None;
                    app.selected_thread = None;
                    app.inline_reply_open = false;
                    app.context_thread = None;
                    app.status = "Move to trash queued".to_string();
                    Task::perform(
                        async move {
                            let _ = engine.send(EngineCommand::MoveToTrash(message_id)).await;
                        },
                        |_| Message::SyncQueued,
                    )
                } else {
                    app.status = selected_action_status(app, "Move to trash");
                    Task::none()
                }
            } else {
                Task::none()
            }
        }
        Message::SearchChanged(query) => {
            app.search_query = query.clone();
            let engine = app.engine.clone();
            let mailbox_id = app.selected_mailbox_id.clone();
            Task::perform(
                async move {
                    let _ = engine
                        .send(EngineCommand::ListThreads { mailbox_id, query })
                        .await;
                },
                |_| Message::SyncQueued,
            )
        }
        Message::SelectThread(thread_id) => {
            app.account_setup_visible = false;
            app.selected_thread = Some(thread_id.clone());
            app.view_mode = ViewMode::Reader;
            app.inline_reply_open = false;
            app.context_thread = None;
            start_view_transition(app, "Reader");
            app.attachment_preview = None;
            app.attachment_open = None;
            app.status = "Loading message".to_string();
            let engine = app.engine.clone();
            Task::perform(
                async move {
                    let _ = engine.send(EngineCommand::LoadThread(thread_id)).await;
                },
                |_| Message::SyncQueued,
            )
        }
        Message::DraftToChanged(value) => {
            app.draft_to = value;
            Task::none()
        }
        Message::DraftSubjectChanged(value) => {
            app.draft_subject = value;
            Task::none()
        }
        Message::DraftBodyChanged(value) => {
            app.draft_body = value;
            Task::none()
        }
        Message::SendDraft => {
            if app.draft_to.trim().is_empty() {
                app.status = "Add a recipient before sending".to_string();
                Task::none()
            } else {
                let draft = DraftMessage {
                    id: DraftId(format!(
                        "draft:{}",
                        app.draft_subject.trim().replace(' ', "-")
                    )),
                    account_id: AccountId("local-demo".to_string()),
                    to: split_csv(&app.draft_to),
                    cc: Vec::new(),
                    bcc: Vec::new(),
                    subject: app.draft_subject.clone(),
                    body: app.draft_body.clone(),
                    attachments: Vec::new(),
                };
                let engine = app.engine.clone();
                app.status = "Draft queued for send".to_string();
                Task::perform(
                    async move {
                        let _ = engine.send(EngineCommand::SaveDraft(draft)).await;
                    },
                    |_| Message::SyncQueued,
                )
            }
        }
        Message::RetrySend(draft_id) => {
            let engine = app.engine.clone();
            app.status = "Retrying send".to_string();
            Task::perform(
                async move {
                    let _ = engine.send(EngineCommand::RetrySend(draft_id)).await;
                },
                |_| Message::SyncQueued,
            )
        }
        Message::CancelSend(draft_id) => {
            let engine = app.engine.clone();
            app.status = "Cancelling send".to_string();
            Task::perform(
                async move {
                    let _ = engine.send(EngineCommand::CancelSend(draft_id)).await;
                },
                |_| Message::SyncQueued,
            )
        }
        Message::PreviewAttachment(attachment_id) => {
            let engine = app.engine.clone();
            app.status = "Loading attachment preview".to_string();
            Task::perform(
                async move {
                    let _ = engine
                        .send(EngineCommand::PreviewAttachment(attachment_id))
                        .await;
                },
                |_| Message::SyncQueued,
            )
        }
        Message::OpenAttachment(attachment_id) => {
            let engine = app.engine.clone();
            app.status = "Checking attachment policy".to_string();
            Task::perform(
                async move {
                    let _ = engine
                        .send(EngineCommand::OpenAttachment(attachment_id))
                        .await;
                },
                |_| Message::SyncQueued,
            )
        }
        Message::ConfirmOpenAttachment(attachment_id) => {
            let engine = app.engine.clone();
            app.status = "Opening attachment".to_string();
            Task::perform(
                async move {
                    let _ = engine
                        .send(EngineCommand::ConfirmOpenAttachment(attachment_id))
                        .await;
                },
                |_| Message::SyncQueued,
            )
        }
        Message::DownloadAttachment(attachment_id) => {
            let engine = app.engine.clone();
            app.status = "Downloading attachment".to_string();
            Task::perform(
                async move {
                    let _ = engine
                        .send(EngineCommand::DownloadAttachment(attachment_id))
                        .await;
                },
                |_| Message::SyncQueued,
            )
        }
        Message::CancelAttachmentDownload(attachment_id) => {
            let engine = app.engine.clone();
            app.status = "Cancelling attachment download".to_string();
            Task::perform(
                async move {
                    let _ = engine
                        .send(EngineCommand::CancelAttachmentDownload(attachment_id))
                        .await;
                },
                |_| Message::SyncQueued,
            )
        }
        Message::RetryAttachmentDownload(attachment_id) => {
            let engine = app.engine.clone();
            app.status = "Retrying attachment download".to_string();
            Task::perform(
                async move {
                    let _ = engine
                        .send(EngineCommand::RetryAttachmentDownload(attachment_id))
                        .await;
                },
                |_| Message::SyncQueued,
            )
        }
        Message::SetNetworkOnline(online) => {
            let engine = app.engine.clone();
            let account_ids = enabled_account_ids(app);
            app.network_online = online;
            app.status = if online {
                "Network sends and sync enabled".to_string()
            } else {
                "Network sends and sync paused".to_string()
            };
            Task::perform(
                async move {
                    let _ = engine.send(EngineCommand::SetNetworkOnline(online)).await;
                    if online {
                        let _ = engine.send(EngineCommand::RunDueSendQueue).await;
                        for account_id in account_ids {
                            let _ = engine.send(EngineCommand::SyncNow(account_id)).await;
                        }
                    }
                },
                |_| Message::SyncQueued,
            )
        }
        Message::ProbeNetwork => {
            let engine = app.engine.clone();
            Task::perform(
                async move {
                    let _ = engine.send(EngineCommand::ProbeNetwork).await;
                },
                |_| Message::SyncQueued,
            )
        }
        Message::SetNotificationsQuiet(quiet) => {
            let engine = app.engine.clone();
            app.notification_policy.quiet = quiet;
            app.notification_policy.quiet_until = None;
            app.status = if quiet {
                "Quiet notifications enabled".to_string()
            } else {
                "Notifications enabled".to_string()
            };
            Task::perform(
                async move {
                    let _ = engine
                        .send(EngineCommand::SetNotificationsQuiet(quiet))
                        .await;
                },
                |_| Message::SyncQueued,
            )
        }
        Message::SetNotificationsQuietFor(seconds) => {
            let engine = app.engine.clone();
            app.notification_policy.quiet = true;
            app.status = format!(
                "Quiet notifications enabled for {}",
                quiet_duration_label(seconds)
            );
            Task::perform(
                async move {
                    let _ = engine
                        .send(EngineCommand::SetNotificationsQuietFor(seconds))
                        .await;
                },
                |_| Message::SyncQueued,
            )
        }
        Message::DismissAttachmentNotice => {
            app.attachment_preview = None;
            app.attachment_open = None;
            Task::none()
        }
        Message::ClearNotifications => {
            app.notifications.clear();
            app.unread_notifications = 0;
            app.status = "Notifications cleared".to_string();
            Task::none()
        }
        Message::ResolveConflict(message_id, resolution) => {
            let engine = app.engine.clone();
            app.status = "Resolving conflict".to_string();
            Task::perform(
                async move {
                    let _ = engine
                        .send(EngineCommand::ResolveConflict(message_id, resolution))
                        .await;
                },
                |_| Message::SyncQueued,
            )
        }
        Message::AccountEmailChanged(value) => {
            app.account_email = value;
            if let Some(domain) = account_domain(&app.account_email) {
                if app.account_imap_host.trim().is_empty() {
                    app.account_imap_host = format!("imap.{domain}");
                }
                if app.account_smtp_host.trim().is_empty() {
                    app.account_smtp_host = format!("smtp.{domain}");
                }
            }
            Task::none()
        }
        Message::AccountImapHostChanged(value) => {
            app.account_imap_host = value;
            Task::none()
        }
        Message::AccountImapPortChanged(value) => {
            app.account_imap_port = value;
            Task::none()
        }
        Message::AccountSmtpHostChanged(value) => {
            app.account_smtp_host = value;
            Task::none()
        }
        Message::AccountSmtpPortChanged(value) => {
            app.account_smtp_port = value;
            Task::none()
        }
        Message::AccountPasswordChanged(value) => {
            app.account_password = value;
            Task::none()
        }
        Message::SaveAccount => match account_config_from_form(app) {
            Ok(account) => {
                let engine = app.engine.clone();
                let password_secret = account_password_secret(app, &account);
                app.status = if app.editing_account_id.is_some() {
                    "Updating account".to_string()
                } else {
                    "Saving account".to_string()
                };
                Task::perform(
                    async move {
                        let _ = engine.send(EngineCommand::SaveAccount(account)).await;
                        if let Some(secret) = password_secret {
                            let _ = engine
                                .send(EngineCommand::SaveCredentialSecret(secret))
                                .await;
                        }
                    },
                    |_| Message::SyncQueued,
                )
            }
            Err(error) => {
                app.status = error;
                Task::none()
            }
        },
        Message::TestAccountConnection => match account_config_from_form(app) {
            Ok(account) => {
                let engine = app.engine.clone();
                app.account_connection_status = "Testing IMAP and SMTP reachability".to_string();
                app.status = "Testing account connection".to_string();
                Task::perform(
                    async move {
                        let _ = engine
                            .send(EngineCommand::TestAccountConnection(account))
                            .await;
                    },
                    |_| Message::SyncQueued,
                )
            }
            Err(error) => {
                app.account_connection_status = error.clone();
                app.status = error;
                Task::none()
            }
        },
        Message::BeginOAuth2(account_id) => {
            let engine = app.engine.clone();
            app.status = "Preparing OAuth2 authorization".to_string();
            Task::perform(
                async move {
                    let _ = engine.send(EngineCommand::BeginOAuth2(account_id)).await;
                },
                |_| Message::SyncQueued,
            )
        }
        Message::EditAccount(account_id) => {
            if let Some(account) = app
                .accounts
                .iter()
                .find(|account| account.id == account_id)
                .cloned()
            {
                app.account_setup_visible = true;
                app.editing_account_id = Some(account.id);
                app.account_email = account.email;
                app.account_imap_host = account.imap_host;
                app.account_imap_port = account.imap_port.to_string();
                app.account_smtp_host = account.smtp_host;
                app.account_smtp_port = account.smtp_port.to_string();
                app.account_password.clear();
                app.identity_name.clear();
                app.identity_email.clear();
                app.account_connection_status.clear();
                app.status = "Editing account".to_string();
            } else {
                app.status = "Account no longer exists".to_string();
            }
            Task::none()
        }
        Message::ToggleAccountEnabled(account_id, enabled) => {
            let engine = app.engine.clone();
            app.status = if enabled {
                "Enabling account".to_string()
            } else {
                "Disabling account".to_string()
            };
            Task::perform(
                async move {
                    let _ = engine
                        .send(EngineCommand::SetAccountEnabled(account_id, enabled))
                        .await;
                },
                |_| Message::SyncQueued,
            )
        }
        Message::DeleteAccount(account_id) => {
            let engine = app.engine.clone();
            app.status = "Deleting account".to_string();
            Task::perform(
                async move {
                    let _ = engine.send(EngineCommand::DeleteAccount(account_id)).await;
                },
                |_| Message::SyncQueued,
            )
        }
        Message::IdentityNameChanged(value) => {
            app.identity_name = value;
            Task::none()
        }
        Message::IdentityEmailChanged(value) => {
            app.identity_email = value;
            Task::none()
        }
        Message::SaveIdentity => match identity_config_from_form(app) {
            Ok(identity) => {
                let engine = app.engine.clone();
                app.status = "Saving identity".to_string();
                Task::perform(
                    async move {
                        let _ = engine.send(EngineCommand::SaveIdentity(identity)).await;
                    },
                    |_| Message::SyncQueued,
                )
            }
            Err(error) => {
                app.status = error;
                Task::none()
            }
        },
        Message::DeleteIdentity(identity_id) => {
            let engine = app.engine.clone();
            app.status = "Deleting identity".to_string();
            Task::perform(
                async move {
                    let _ = engine
                        .send(EngineCommand::DeleteIdentity(identity_id))
                        .await;
                },
                |_| Message::SyncQueued,
            )
        }
        Message::SelectNextThread => select_relative_thread(app, 1),
        Message::SelectPreviousThread => select_relative_thread(app, -1),
        Message::OpenSelectedThreadContext => {
            if let Some(thread_id) = app.selected_thread.clone() {
                app.context_thread = Some(thread_id);
                start_view_transition(app, "Quick actions");
                app.status = "Thread actions ready".to_string();
            } else {
                app.status = "Select a thread for actions".to_string();
            }
            Task::none()
        }
        Message::OpenThreadContext(thread_id) => {
            app.context_thread = Some(thread_id.clone());
            app.account_setup_visible = false;
            app.view_mode = ViewMode::Reader;
            app.inline_reply_open = false;
            app.selected_thread = Some(thread_id.clone());
            app.attachment_preview = None;
            app.attachment_open = None;
            start_view_transition(app, "Quick actions");
            app.status = "Thread actions ready".to_string();
            let engine = app.engine.clone();
            Task::perform(
                async move {
                    let _ = engine.send(EngineCommand::LoadThread(thread_id)).await;
                },
                |_| Message::SyncQueued,
            )
        }
        Message::CloseThreadContext => {
            app.context_thread = None;
            start_view_transition(app, "Thread list");
            app.status = "Thread actions closed".to_string();
            Task::none()
        }
        Message::UiTick => {
            app.transition_ticks_remaining = app.transition_ticks_remaining.saturating_sub(1);
            Task::none()
        }
    }
}

pub fn subscription(app: &App) -> Subscription<Message> {
    let mut receiver = app.engine.subscribe();

    let engine_events = Subscription::run_with_id(
        "courier-engine-events",
        iced::stream::channel(100, move |mut output| async move {
            loop {
                match receiver.recv().await {
                    Ok(event) => {
                        let _ = output.send(Message::EngineEvent(event)).await;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }),
    );

    let mut subscriptions = vec![
        engine_events,
        iced::keyboard::on_key_press(keyboard_shortcut),
        iced::time::every(Duration::from_secs(60)).map(|_| Message::ProbeNetwork),
    ];
    if app.transition_ticks_remaining > 0 {
        subscriptions.push(iced::time::every(Duration::from_millis(50)).map(|_| Message::UiTick));
    }

    Subscription::batch(subscriptions)
}

pub fn view(app: &App) -> Element<'_, Message> {
    let mailboxes =
        crate::views::mailbox_list::view(&app.mailboxes, app.selected_mailbox_id.as_ref());
    let visible_threads = app.threads.iter().collect::<Vec<_>>();
    let threads = crate::views::thread_list::view(
        &visible_threads,
        app.selected_thread.as_ref(),
        &app.selected_mailbox_name,
    );
    let mut reader = if app.shortcuts_help_visible {
        shortcuts_help_modal()
    } else if app.account_setup_visible {
        column![crate::views::account_setup::view(
            crate::views::account_setup::AccountSetupViewState {
                accounts: &app.accounts,
                identities: &app.identities,
                editing_account_id: app.editing_account_id.as_ref(),
                email: &app.account_email,
                imap_host: &app.account_imap_host,
                imap_port: &app.account_imap_port,
                smtp_host: &app.account_smtp_host,
                smtp_port: &app.account_smtp_port,
                password: &app.account_password,
                identity_name: &app.identity_name,
                identity_email: &app.identity_email,
                connection_status: &app.account_connection_status,
            },
        )]
        .height(Length::Fill)
        .spacing(10)
    } else if app.view_mode == ViewMode::Compose {
        column![crate::views::composer::view(
            &app.draft_to,
            &app.draft_subject,
            &app.draft_body,
            &app.send_queue,
        )]
        .height(Length::Fill)
        .spacing(0)
    } else {
        let mut reader_stack = column![reader_action_bar(app)]
            .height(Length::Fill)
            .spacing(10);
        if !app.conflicts.is_empty() {
            reader_stack = reader_stack.push(conflicts_view(&app.conflicts));
        }
        if !app.notifications.is_empty() {
            reader_stack = reader_stack.push(notifications_view(
                &app.notifications,
                app.unread_notifications,
                &app.notification_policy,
            ));
        }
        reader_stack = reader_stack.push(crate::views::reader::view(
            crate::views::reader::ReaderViewState {
                body: app.selected_body.as_ref(),
                render_tree: app.selected_render.as_ref(),
                attachment_preview: app.attachment_preview.as_ref(),
                attachment_open: app.attachment_open.as_ref(),
                attachment_transfers: &app.attachment_transfers,
                inline_reply_open: app.inline_reply_open,
                draft_to: &app.draft_to,
                draft_subject: &app.draft_subject,
                draft_body: &app.draft_body,
            },
        ));
        reader_stack
    };
    if app.transition_ticks_remaining > 0 {
        reader = column![transition_strip(app), reader]
            .height(Length::Fill)
            .spacing(crate::theme::SPACE_SM);
    }

    let sidebar = column![
        crate::components::surface::header(
            "Courier",
            row![
                if app.network_online {
                    Icon::Wifi.view_styled(14.0, crate::theme::SUCCESS)
                } else {
                    Icon::WifiOff.view_styled(14.0, crate::theme::DANGER)
                },
                text(if app.network_online {
                    "Online"
                } else {
                    "Offline"
                })
                .size(12)
                .color(crate::theme::TEXT_MUTED),
            ]
            .spacing(4)
            .align_y(iced::Alignment::Center)
        ),
        sidebar_summary(app),
        crate::components::list::section_label("PRIMARY"),
        crate::components::action_bar::button_primary_with_icon("Compose", Icon::Compose, Message::Compose),
        crate::components::action_bar::button_toolbar_with_icon(add_account_label(), Icon::AccountAdd, crate::theme::TEXT_MUTED, Message::AddAccount),
        crate::components::surface::divider(),
        mailboxes,
        iced::widget::vertical_space(),
        crate::components::surface::divider(),
        crate::components::list::section_label("CONTROLS"),
        crate::components::action_bar::button_toolbar_with_icon("Accounts", Icon::AccountManage, crate::theme::TEXT_MUTED, Message::AddAccount),
        notification_sidebar_controls(app),
        row![
            crate::components::action_bar::button_toolbar_with_icon("Sync", Icon::Sync, crate::theme::TEXT_MUTED, Message::SyncNow),
            crate::components::action_bar::button_toolbar_with_icon(
                if app.network_online {
                    "Go Offline"
                } else {
                    "Go Online"
                },
                if app.network_online {
                    Icon::WifiOff
                } else {
                    Icon::Wifi
                },
                crate::theme::TEXT_MUTED,
                Message::SetNetworkOnline(!app.network_online),
            ),
        ]
        .spacing(crate::theme::SPACE_SM),
    ]
    .spacing(crate::theme::SPACE_SM)
    .padding(crate::theme::SPACE_SM);

    let mut thread_column = column![crate::components::search::view(&app.search_query)]
        .spacing(crate::theme::SPACE_SM)
        .padding(crate::theme::SPACE_SM)
        .height(Length::Fill);
    if let Some(context_thread) = app.context_thread.as_ref() {
        thread_column = thread_column.push(thread_context_menu(app, context_thread));
    }
    thread_column = thread_column.push(threads);

    let content = row![
        crate::components::surface::pane(sidebar).width(Length::Fixed(crate::theme::SIDEBAR_WIDTH)),
        crate::components::surface::pane(thread_column)
            .width(Length::Fixed(crate::theme::THREAD_LIST_WIDTH)),
        crate::components::surface::pane(reader).width(Length::Fill),
    ]
    .height(Length::Fill)
    .spacing(crate::theme::SPACE_SM);

    crate::components::surface::app_background(
        column![
            content,
            crate::components::status_bar::view(&app.status, shortcut_hint(app))
        ]
        .spacing(crate::theme::SPACE_SM)
        .padding(crate::theme::APP_PADDING),
    )
    .into()
}

pub fn theme(_app: &App) -> Theme {
    Theme::Light
}

fn add_account_label() -> &'static str {
    "\u{6dfb}\u{52a0}\u{8d26}\u{6237}"
}

fn start_view_transition(app: &mut App, label: &str) {
    app.transition_label = label.to_string();
    app.transition_ticks_remaining = 6;
}

fn transition_progress(app: &App) -> f32 {
    const TOTAL_TICKS: f32 = 6.0;
    ((TOTAL_TICKS - app.transition_ticks_remaining as f32) / TOTAL_TICKS).clamp(0.0, 1.0)
}

fn transition_strip(app: &App) -> Element<'_, Message> {
    let progress = transition_progress(app);
    container(
        column![
            row![
                text(&app.transition_label)
                    .size(crate::theme::FONT_CAPTION)
                    .color(crate::theme::ACCENT),
                iced::widget::horizontal_space(),
                text("view transition")
                    .size(crate::theme::FONT_CAPTION)
                    .color(crate::theme::TEXT_MUTED),
            ]
            .align_y(iced::Alignment::Center),
            progress_bar(0.0..=1.0, progress),
        ]
        .spacing(crate::theme::SPACE_XS),
    )
    .padding([6, 10])
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(crate::theme::SURFACE_ALT)),
        border: iced::Border {
            width: 1.0,
            radius: crate::theme::RADIUS_LG.into(),
            color: crate::theme::ACCENT_MUTED,
        },
        ..container::Style::default()
    })
    .into()
}

fn sidebar_summary(app: &App) -> Element<'_, Message> {
    let enabled_accounts = app
        .accounts
        .iter()
        .filter(|account| account.enabled)
        .count();
    container(
        column![
            row![
                text(format!("{} account(s)", enabled_accounts))
                    .size(crate::theme::FONT_CAPTION)
                    .color(crate::theme::TEXT),
                iced::widget::horizontal_space(),
                text(notification_policy_label(&app.notification_policy))
                    .size(crate::theme::FONT_CAPTION)
                    .color(if app.notification_policy.quiet {
                        crate::theme::WARNING
                    } else {
                        crate::theme::SUCCESS
                    }),
            ]
            .align_y(iced::Alignment::Center),
            text("J/K move · R reply · M actions · D trash")
                .size(crate::theme::FONT_CAPTION)
                .color(crate::theme::TEXT_MUTED),
        ]
        .spacing(crate::theme::SPACE_XS),
    )
    .padding(crate::theme::SPACE_SM)
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(crate::theme::SURFACE_ALT)),
        border: iced::Border {
            width: 1.0,
            radius: crate::theme::RADIUS_LG.into(),
            color: crate::theme::BORDER,
        },
        ..container::Style::default()
    })
    .into()
}

fn shortcut_hint(app: &App) -> &'static str {
    if app.shortcuts_help_visible {
        "Press Esc or click Close to return"
    } else {
        "Press ? for keyboard shortcuts"
    }
}

fn reader_action_bar(app: &App) -> Element<'_, Message> {
    let mut actions = row![].spacing(crate::theme::SPACE_XS);
    if app.selected_body.is_some() {
        actions = actions
            .push(crate::components::action_bar::button_text_with_icon(
                "Archive",
                Icon::Archive,
                crate::theme::TEXT_MUTED,
                Message::ArchiveSelected,
            ))
            .push(crate::components::action_bar::button_text_with_icon(
                "Mark read",
                Icon::CheckCircle,
                crate::theme::TEXT_MUTED,
                Message::MarkReadSelected,
            ))
            .push(crate::components::action_bar::button_text_with_icon(
                "Trash",
                Icon::Delete,
                crate::theme::TEXT_MUTED,
                Message::TrashSelected,
            ));
    }

    let title = app
        .selected_body
        .as_ref()
        .map(|body| body.subject.as_str())
        .unwrap_or("Reading pane");

    let mut bar = row![
        text(title).size(13).color(crate::theme::TEXT),
        iced::widget::horizontal_space(),
        actions,
    ]
    .align_y(iced::Alignment::Center)
    .spacing(crate::theme::SPACE_SM)
    .padding(crate::theme::SPACE_SM);

    if app.unread_notifications > 0 {
        bar = bar.push(crate::components::badge::count(app.unread_notifications));
    }

    crate::components::surface::toolbar_surface(bar).into()
}

fn thread_context_menu<'a>(app: &'a App, thread_id: &'a ThreadId) -> Element<'a, Message> {
    let title = app
        .threads
        .iter()
        .find(|thread| thread.id == *thread_id)
        .map(|thread| thread.subject.as_str())
        .unwrap_or("Thread actions");

    container(
        column![
            row![
                text("Focused actions")
                    .size(crate::theme::FONT_CAPTION)
                    .color(crate::theme::ACCENT),
                iced::widget::horizontal_space(),
                crate::components::action_bar::button_text("Close", Message::CloseThreadContext),
            ]
            .align_y(iced::Alignment::Center),
            text(title).size(13).color(crate::theme::TEXT),
            row![
                crate::components::action_bar::button_text_with_icon("Reply", Icon::Reply, crate::theme::TEXT_MUTED, Message::ReplyInline),
                crate::components::action_bar::button_text_with_icon("Archive", Icon::Archive, crate::theme::TEXT_MUTED, Message::ArchiveSelected),
                crate::components::action_bar::button_text_with_icon("Mark read", Icon::CheckCircle, crate::theme::TEXT_MUTED, Message::MarkReadSelected),
                crate::components::action_bar::button_text_with_icon("Trash", Icon::Delete, crate::theme::TEXT_MUTED, Message::TrashSelected),
            ]
            .spacing(crate::theme::SPACE_XS),
            text("Keyboard: R reply · D trash · Esc close")
                .size(crate::theme::FONT_CAPTION)
                .color(crate::theme::TEXT_MUTED),
        ]
        .spacing(crate::theme::SPACE_XS)
        .padding(crate::theme::SPACE_SM),
    )
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(crate::theme::SURFACE_ALT)),
        border: iced::Border {
            width: 2.0,
            radius: crate::theme::RADIUS_LG.into(),
            color: crate::theme::ACCENT,
        },
        ..container::Style::default()
    })
    .into()
}

fn conflicts_view<'a>(conflicts: &'a [ConflictSummary]) -> Element<'a, Message> {
    let mut content = column![
        row![
            crate::components::list::section_label("Sync conflicts"),
            iced::widget::horizontal_space(),
            crate::components::badge::count(conflicts.len().min(u32::MAX as usize) as u32),
        ]
        .align_y(iced::Alignment::Center)
    ]
    .spacing(6)
    .padding([8, 12]);

    for conflict in conflicts.iter().take(3) {
        content = content.push(
            row![
                column![
                    text(&conflict.subject).size(13).color(crate::theme::TEXT),
                    text(&conflict.reason)
                        .size(12)
                        .color(crate::theme::TEXT_MUTED),
                ]
                .spacing(2)
                .width(Length::Fill),
                crate::components::action_bar::button_text(
                    "Open",
                    Message::SelectThread(conflict.thread_id.clone()),
                ),
                crate::components::action_bar::button_text(
                    "Keep",
                    Message::ResolveConflict(
                        conflict.message_id.clone(),
                        ConflictResolution::KeepLocal,
                    ),
                ),
                crate::components::action_bar::button_text(
                    "Accept",
                    Message::ResolveConflict(
                        conflict.message_id.clone(),
                        ConflictResolution::AcceptRemote,
                    ),
                ),
                crate::components::action_bar::button_text(
                    "Requeue",
                    Message::ResolveConflict(
                        conflict.message_id.clone(),
                        ConflictResolution::RequeueLocal,
                    ),
                ),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        );
    }

    container(content)
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(crate::theme::SURFACE_ALT)),
            border: iced::Border {
                width: 1.0,
                radius: 4.0.into(),
                color: crate::theme::WARNING,
            },
            ..container::Style::default()
        })
        .into()
}

fn notification_sidebar_controls(app: &App) -> Element<'_, Message> {
    if app.notification_policy.quiet {
        return crate::components::action_bar::button_toolbar_with_icon(
            "Unmute",
            Icon::Bell,
            crate::theme::TEXT_MUTED,
            Message::SetNotificationsQuiet(false),
        );
    }

    row![
        crate::components::action_bar::button_toolbar_with_icon(
            "15m",
            Icon::BellOff,
            crate::theme::TEXT_MUTED,
            Message::SetNotificationsQuietFor(900)
        ),
        crate::components::action_bar::button_toolbar_with_icon(
            "1h",
            Icon::BellOff,
            crate::theme::TEXT_MUTED,
            Message::SetNotificationsQuietFor(60 * 60)
        ),
        crate::components::action_bar::button_toolbar_with_icon(
            "4h",
            Icon::BellOff,
            crate::theme::TEXT_MUTED,
            Message::SetNotificationsQuietFor(4 * 60 * 60)
        ),
    ]
    .spacing(crate::theme::SPACE_XS)
    .into()
}

fn notification_policy_label(policy: &NotificationPolicyState) -> String {
    if policy.quiet {
        if let Some(until) = policy.quiet_until {
            return format!("Quiet {}", relative_minutes_label(until));
        }

        return "Quiet".to_string();
    }

    "Notify".to_string()
}

fn relative_minutes_label(unix_until: i64) -> String {
    let remaining = unix_until
        .saturating_sub(unix_timestamp())
        .div_euclid(60)
        .max(1);
    if remaining < 60 {
        format!("{remaining}m")
    } else {
        let hours = (remaining + 59) / 60;
        format!("{hours}h")
    }
}

fn quiet_duration_label(seconds: i64) -> String {
    let minutes = (seconds.max(60) + 59) / 60;
    if minutes < 60 {
        if minutes == 1 {
            "1 minute".to_string()
        } else {
            format!("{minutes} minutes")
        }
    } else {
        let hours = (minutes + 59) / 60;
        if hours == 1 {
            "1 hour".to_string()
        } else {
            format!("{hours} hours")
        }
    }
}

fn unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
        .min(i64::MAX as u64) as i64
}

fn notifications_view<'a>(
    notifications: &'a [DesktopNotification],
    unread: u32,
    policy: &'a NotificationPolicyState,
) -> Element<'a, Message> {
    let mut content = column![
        row![
            crate::components::list::section_label("Notifications"),
            iced::widget::horizontal_space(),
            crate::components::badge::count(unread),
            crate::components::action_bar::button_text(
                if policy.quiet { "Notify" } else { "Quiet" },
                Message::SetNotificationsQuiet(!policy.quiet),
            ),
            crate::components::action_bar::button_text(
                "15m",
                Message::SetNotificationsQuietFor(900)
            ),
            crate::components::action_bar::button_text(
                "1h",
                Message::SetNotificationsQuietFor(60 * 60)
            ),
            crate::components::action_bar::button_text(
                "4h",
                Message::SetNotificationsQuietFor(4 * 60 * 60)
            ),
            crate::components::action_bar::button_text("Clear", Message::ClearNotifications),
        ]
        .align_y(iced::Alignment::Center)
    ]
    .spacing(6)
    .padding([8, 12]);

    if policy.quiet || policy.suppressed_count > 0 {
        content = content.push(
            text(&policy.reason)
                .size(crate::theme::FONT_CAPTION)
                .color(crate::theme::TEXT_MUTED),
        );
    }

    for notification in notifications.iter().rev().take(3) {
        content = content.push(
            row![
                crate::components::badge::pill(notification_kind_label(&notification.kind)),
                column![
                    text(&notification.title).size(13).color(crate::theme::TEXT),
                    text(&notification.body)
                        .size(12)
                        .color(crate::theme::TEXT_MUTED),
                ]
                .spacing(2)
                .width(Length::Fill),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        );
    }

    container(content)
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(crate::theme::SURFACE_ALT)),
            border: iced::Border {
                width: 1.0,
                radius: 4.0.into(),
                color: crate::theme::BORDER,
            },
            ..container::Style::default()
        })
        .into()
}

fn notification_kind_label(kind: &NotificationKind) -> &'static str {
    match kind {
        NotificationKind::NewMail => "MAIL",
        NotificationKind::Sync => "SYNC",
        NotificationKind::Send => "SEND",
        NotificationKind::Warning => "WARN",
        NotificationKind::Error => "ERR",
    }
}

fn handle_engine_event(app: &mut App, event: EngineEvent) -> Task<Message> {
    match event {
        EngineEvent::Ready => {
            app.status = "Engine ready".to_string();
            let engine = app.engine.clone();
            return Task::perform(
                async move {
                    let _ = engine.send(EngineCommand::ListSendQueue).await;
                    let _ = engine.send(EngineCommand::ListConflicts).await;
                    let _ = engine.send(EngineCommand::CredentialStatus).await;
                    let _ = engine.send(EngineCommand::RunDueSendQueue).await;
                },
                |_| Message::SyncQueued,
            );
        }
        EngineEvent::AccountsUpdated(accounts) => {
            app.accounts = accounts;
            app.status = "Accounts loaded".to_string();
        }
        EngineEvent::IdentitiesUpdated(identities) => {
            app.identities = identities;
            app.status = "Identities loaded".to_string();
        }
        EngineEvent::MailboxesUpdated(mailboxes) => {
            app.mailboxes = mailboxes;
            app.status = "Mailboxes loaded".to_string();
        }
        EngineEvent::AccountSaved(account) => {
            app.account_setup_visible = false;
            app.editing_account_id = None;
            app.view_mode = ViewMode::Reader;
            reset_account_form(app);
            app.status = format!("Account saved: {}", account.email);
        }
        EngineEvent::AccountConnectionTested(result) => {
            let imap = endpoint_status("IMAP", &result.imap);
            let smtp = endpoint_status("SMTP", &result.smtp);
            app.account_connection_status = format!("{imap}; {smtp}");
            app.status = if result.imap.ok && result.smtp.ok {
                "Connection test passed".to_string()
            } else {
                "Connection test failed".to_string()
            };
        }
        EngineEvent::OAuth2AuthorizationStarted(result) => {
            app.status = match result {
                Ok(request) => format!("OAuth2 authorization ready: {}", request.auth_url),
                Err(error) => format!("OAuth2 setup failed: {error}"),
            };
        }
        EngineEvent::OAuth2Completed(result) => {
            app.status = match result {
                Ok(reference) => format!("OAuth2 credential stored: {}", reference.key),
                Err(error) => format!("OAuth2 incomplete: {error}"),
            };
        }
        EngineEvent::CredentialStoreChecked(status) => {
            app.status = if status.available {
                format!("Credential store ready: {}", status.backend)
            } else {
                status.message
            };
        }
        EngineEvent::CredentialSaved(result) => {
            app.status = match result {
                Ok(reference) => {
                    app.account_password.clear();
                    format!("Credential stored: {}", reference.key)
                }
                Err(error) => format!("Credential storage failed: {error}"),
            };
        }
        EngineEvent::IdentitySaved(identity) => {
            app.identity_name.clear();
            app.identity_email.clear();
            app.status = format!("Identity saved: {}", identity.email);
        }
        EngineEvent::SyncProgress { progress, .. } => {
            app.status = format!("Sync {:.0}%", progress * 100.0);
        }
        EngineEvent::NewMessages { messages, .. } => {
            app.status = format!("{} new message(s)", messages.len());
        }
        EngineEvent::ThreadsUpdated(threads) => {
            app.threads = threads;
            app.status = "Threads updated".to_string();
        }
        EngineEvent::MessageLoaded(body) => {
            app.selected_render = Some(render_message_body(&body));
            app.selected_thread = Some(body.thread_id.clone());
            app.selected_body = Some(body);
            app.status = "Message loaded".to_string();
        }
        EngineEvent::AttachmentPreviewLoaded(result) => match result {
            Ok(preview) => {
                app.attachment_open = None;
                app.status = preview.message.clone();
                app.attachment_preview = Some(preview);
            }
            Err(error) => {
                app.attachment_preview = None;
                app.status = error;
            }
        },
        EngineEvent::AttachmentOpenPrepared(request) => {
            app.attachment_preview = None;
            app.status = if request.allowed {
                format!("Attachment ready: {}", request.reason)
            } else {
                format!("Attachment blocked: {}", request.reason)
            };
            app.attachment_open = Some(request);
        }
        EngineEvent::AttachmentOpenExecuted(result) => match result {
            Ok(request) => {
                app.attachment_preview = None;
                app.attachment_open = None;
                app.status = format!("Opened attachment: {}", request.attachment.filename);
            }
            Err(error) => {
                app.status = format!("Attachment open failed: {error}");
            }
        },
        EngineEvent::AttachmentTransfersUpdated(transfers) => {
            app.attachment_transfers = transfers;
            app.status = "Attachment transfer state updated".to_string();
        }
        EngineEvent::SendQueueUpdated(queue) => {
            app.send_queue = queue;
            app.status = "Send queue updated".to_string();
        }
        EngineEvent::NetworkStatusChanged(status) => {
            let was_offline = !app.network_online;
            app.network_online = status.online;
            app.status = status.reason;
            if was_offline && app.network_online {
                return network_resume_task(app);
            }
        }
        EngineEvent::ConflictsUpdated(conflicts) => {
            app.conflicts = conflicts;
            if !app.conflicts.is_empty() {
                app.status = format!("{} sync conflict(s)", app.conflicts.len());
            }
        }
        EngineEvent::NotificationRaised(notification) => {
            app.status = notification.title.clone();
            app.notifications.push(notification);
            app.unread_notifications = app.unread_notifications.saturating_add(1);
            if app.notifications.len() > 20 {
                let overflow = app.notifications.len() - 20;
                app.notifications.drain(0..overflow);
            }
        }
        EngineEvent::NotificationPolicyChanged(policy) => {
            app.status = policy.reason.clone();
            app.notification_policy = policy;
        }
        EngineEvent::SendResult { result, .. } => {
            app.status = match result {
                Ok(()) => {
                    app.draft_to.clear();
                    app.draft_subject.clear();
                    app.draft_body.clear();
                    app.view_mode = ViewMode::Reader;
                    "Message sent".to_string()
                }
                Err(error) => format!("Send failed: {error}"),
            };
        }
        EngineEvent::Error(error) => {
            app.status = error;
        }
    }

    Task::none()
}

fn network_resume_task(app: &App) -> Task<Message> {
    let engine = app.engine.clone();
    let account_ids = enabled_account_ids(app);
    Task::perform(
        async move {
            let _ = engine.send(EngineCommand::RunDueSendQueue).await;
            for account_id in account_ids {
                let _ = engine.send(EngineCommand::SyncNow(account_id)).await;
            }
        },
        |_| Message::SyncQueued,
    )
}

fn enabled_account_ids(app: &App) -> Vec<AccountId> {
    app.accounts
        .iter()
        .filter(|account| account.enabled)
        .map(|account| account.id.clone())
        .collect()
}

fn render_message_body(body: &MessageBody) -> RenderTree {
    if body.content_type.to_ascii_lowercase().contains("html") {
        render_tree_from_html(&body.body)
    } else {
        render_tree_from_text(&body.body)
    }
}

fn selected_action_status(app: &App, action: &str) -> String {
    match app.selected_thread.as_ref() {
        Some(_) => format!("{action} queued"),
        None => format!("Select a message to {action}"),
    }
}

fn keyboard_shortcut(key: Key, modifiers: Modifiers) -> Option<Message> {
    if modifiers.command() || modifiers.control() || modifiers.alt() {
        return None;
    }

    match key.as_ref() {
        Key::Character("j") | Key::Named(key::Named::ArrowDown) => Some(Message::SelectNextThread),
        Key::Character("k") | Key::Named(key::Named::ArrowUp) => {
            Some(Message::SelectPreviousThread)
        }
        Key::Character("m") => Some(Message::OpenSelectedThreadContext),
        Key::Character("r") => Some(Message::ReplyInline),
        Key::Character("d") | Key::Named(key::Named::Delete) => Some(Message::TrashSelected),
        Key::Character("?") => Some(Message::ToggleShortcutsHelp),
        Key::Named(key::Named::Escape) => Some(Message::CancelActivePanel),
        _ => None,
    }
}

fn select_relative_thread(app: &mut App, direction: isize) -> Task<Message> {
    if app.account_setup_visible || app.view_mode != ViewMode::Reader || app.threads.is_empty() {
        return Task::none();
    }

    let current_index = app
        .selected_thread
        .as_ref()
        .and_then(|selected| app.threads.iter().position(|thread| thread.id == *selected));
    let next_index = match (current_index, direction.is_negative()) {
        (Some(index), true) => index.saturating_sub(1),
        (Some(index), false) => (index + 1).min(app.threads.len() - 1),
        (None, true) => app.threads.len() - 1,
        (None, false) => 0,
    };
    let thread_id = app.threads[next_index].id.clone();
    let engine = app.engine.clone();

    app.selected_thread = Some(thread_id.clone());
    app.inline_reply_open = false;
    app.context_thread = None;
    app.attachment_preview = None;
    app.attachment_open = None;
    app.status = "Loading message".to_string();

    Task::perform(
        async move {
            let _ = engine.send(EngineCommand::LoadThread(thread_id)).await;
        },
        |_| Message::SyncQueued,
    )
}

fn reply_subject(subject: &str) -> String {
    if subject.trim_start().to_ascii_lowercase().starts_with("re:") {
        subject.to_string()
    } else {
        format!("Re: {subject}")
    }
}

fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn account_config_from_form(app: &App) -> Result<AccountConfig, String> {
    let email = app.account_email.trim();
    let imap_host = app.account_imap_host.trim();
    let smtp_host = app.account_smtp_host.trim();

    if email.is_empty() || !email.contains('@') {
        return Err("Enter a valid email address".to_string());
    }
    if imap_host.is_empty() {
        return Err("Enter an IMAP host".to_string());
    }
    if smtp_host.is_empty() {
        return Err("Enter an SMTP host".to_string());
    }

    let imap_port = parse_port(&app.account_imap_port, "IMAP")?;
    let smtp_port = parse_port(&app.account_smtp_port, "SMTP")?;

    Ok(AccountConfig {
        id: app
            .editing_account_id
            .clone()
            .unwrap_or_else(|| AccountId(format!("account:{}", safe_identifier(email)))),
        email: email.to_string(),
        provider: edited_account(app)
            .map(|account| account.provider.clone())
            .unwrap_or(ProviderKind::GenericImap),
        imap_host: imap_host.to_string(),
        imap_port,
        smtp_host: smtp_host.to_string(),
        smtp_port,
        auth_type: edited_account(app)
            .map(|account| account.auth_type.clone())
            .unwrap_or(AuthType::Password),
    })
}

fn account_password_secret(app: &App, account: &AccountConfig) -> Option<CredentialSecret> {
    let password = app.account_password.trim();
    if password.is_empty() || !matches!(account.auth_type, AuthType::Password) {
        return None;
    }

    Some(CredentialSecret {
        reference: CredentialRef {
            account_id: account.id.clone(),
            kind: CredentialKind::Password,
            service: "dev.hephaestus.courier.password".to_string(),
            key: format!("{}:password", account.id.0),
        },
        secret: password.to_string(),
    })
}

fn edited_account(app: &App) -> Option<&AccountState> {
    let account_id = app.editing_account_id.as_ref()?;
    app.accounts
        .iter()
        .find(|account| account.id == *account_id)
}

fn identity_config_from_form(app: &App) -> Result<IdentityConfig, String> {
    let account_id = app
        .editing_account_id
        .clone()
        .ok_or_else(|| "Edit an account before adding an identity".to_string())?;
    let name = app.identity_name.trim();
    let email = app.identity_email.trim();

    if name.is_empty() {
        return Err("Enter an identity display name".to_string());
    }
    if email.is_empty() || !email.contains('@') {
        return Err("Enter a valid identity email".to_string());
    }

    Ok(IdentityConfig {
        id: IdentityId(format!(
            "identity:{}:{}",
            safe_identifier(&account_id.0),
            safe_identifier(email)
        )),
        account_id,
        name: name.to_string(),
        email: email.to_string(),
        reply_to: None,
    })
}

fn reset_account_form(app: &mut App) {
    app.account_email.clear();
    app.account_imap_host.clear();
    app.account_imap_port = "993".to_string();
    app.account_smtp_host.clear();
    app.account_smtp_port = "587".to_string();
    app.account_password.clear();
    app.identity_name.clear();
    app.identity_email.clear();
    app.account_connection_status.clear();
}

fn endpoint_status(label: &str, result: &courier_proto::EndpointCheckResult) -> String {
    if result.ok {
        format!("{label} {}:{} reachable", result.host, result.port)
    } else {
        format!(
            "{label} {}:{} failed: {}",
            result.host,
            result.port,
            result.error.as_deref().unwrap_or("unknown error")
        )
    }
}

fn parse_port(value: &str, label: &str) -> Result<u16, String> {
    value
        .trim()
        .parse::<u16>()
        .map_err(|_| format!("Enter a valid {label} port"))
}

fn account_domain(email: &str) -> Option<String> {
    let (_, domain) = email.trim().split_once('@')?;
    let domain = domain.trim();
    if domain.is_empty() {
        None
    } else {
        Some(domain.to_string())
    }
}

fn safe_identifier(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn default_data_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".courier")
}

fn shortcuts_help_modal<'a>() -> Element<'a, Message> {
    container(
        column![
            row![
                text("Keyboard Shortcuts").size(crate::theme::FONT_TITLE).color(crate::theme::TEXT),
                iced::widget::horizontal_space(),
                crate::components::action_bar::button_text_with_icon(
                    "Close",
                    Icon::Delete,
                    crate::theme::TEXT_MUTED,
                    Message::ToggleShortcutsHelp,
                ),
            ]
            .align_y(iced::Alignment::Center),
            crate::components::surface::divider(),
            column![
                shortcut_row("J / Arrow Down", "Move to next message"),
                shortcut_row("K / Arrow Up", "Move to previous message"),
                shortcut_row("R", "Reply to current message inline"),
                shortcut_row("D / Delete", "Move current message to trash"),
                shortcut_row("M", "Open context actions menu"),
                shortcut_row("Esc", "Close active panels, dialogs, or compose"),
                shortcut_row("?", "Toggle this shortcuts help dialog"),
            ]
            .spacing(12),
        ]
        .spacing(16)
        .padding(24)
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(crate::theme::SURFACE_ALT)),
        border: iced::Border {
            width: 1.0,
            radius: crate::theme::RADIUS_LG.into(),
            color: crate::theme::BORDER,
        },
        ..container::Style::default()
    })
    .into()
}

fn shortcut_row<'a>(key: &'static str, description: &'static str) -> Element<'a, Message> {
    row![
        container(text(key).size(12).font(iced::Font::MONOSPACE).color(crate::theme::TEXT))
            .padding([4, 8])
            .style(|_| container::Style {
                background: Some(iced::Background::Color(crate::theme::SURFACE)),
                border: iced::Border {
                    width: 1.0,
                    radius: crate::theme::RADIUS_SM.into(),
                    color: crate::theme::BORDER,
                },
                ..container::Style::default()
            }),
        text(description).size(13).color(crate::theme::TEXT_MUTED),
    ]
    .spacing(12)
    .align_y(iced::Alignment::Center)
    .into()
}
