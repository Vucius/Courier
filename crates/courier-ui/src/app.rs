use std::path::PathBuf;

use courier_app::{EngineConfig, EngineHandle, spawn_engine};
use courier_proto::{
    AccountConfig, AccountId, AccountState, AttachmentId, AttachmentOpenRequest, AttachmentPreview,
    AttachmentTransfer, AuthType, ConflictResolution, ConflictSummary, DesktopNotification,
    DraftId, DraftMessage, EngineCommand, EngineEvent, IdentityConfig, IdentityId, IdentitySummary,
    MailboxId, MailboxSummary, MessageBody, MessageId, NotificationKind, ProviderKind,
    SendQueueItem, ThreadId, ThreadSummary,
};
use courier_render::{RenderTree, render_tree_from_html, render_tree_from_text};
use iced::futures::SinkExt;
use iced::widget::{column, container, row, text};
use iced::{Element, Length, Subscription, Task, Theme};

#[derive(Debug, Clone)]
pub enum Message {
    SyncNow,
    SyncQueued,
    EngineEvent(EngineEvent),
    MailboxSelected(Option<MailboxId>, String),
    AddAccount,
    Compose,
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
    DismissAttachmentNotice,
    ClearNotifications,
    ResolveConflict(MessageId, ConflictResolution),
    AccountEmailChanged(String),
    AccountImapHostChanged(String),
    AccountImapPortChanged(String),
    AccountSmtpHostChanged(String),
    AccountSmtpPortChanged(String),
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
    search_query: String,
    draft_to: String,
    draft_subject: String,
    draft_body: String,
    account_setup_visible: bool,
    editing_account_id: Option<AccountId>,
    account_email: String,
    account_imap_host: String,
    account_imap_port: String,
    account_smtp_host: String,
    account_smtp_port: String,
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
        search_query: String::new(),
        draft_to: String::new(),
        draft_subject: String::new(),
        draft_body: String::new(),
        account_setup_visible: false,
        editing_account_id: None,
        account_email: String::new(),
        account_imap_host: String::new(),
        account_imap_port: "993".to_string(),
        account_smtp_host: String::new(),
        account_smtp_port: "587".to_string(),
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
            app.status = "Sync queued".to_string();
            Task::perform(
                async move {
                    let _ = engine
                        .send(EngineCommand::SyncNow(AccountId("local-demo".to_string())))
                        .await;
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
            app.status = "Account setup ready".to_string();
            Task::none()
        }
        Message::Compose => {
            app.account_setup_visible = false;
            app.status = "Draft ready".to_string();
            Task::none()
        }
        Message::ArchiveSelected => {
            if let Some(body) = app.selected_body.as_ref() {
                let engine = app.engine.clone();
                let message_id = body.id.clone();
                app.selected_body = None;
                app.selected_render = None;
                app.attachment_preview = None;
                app.attachment_open = None;
                app.selected_thread = None;
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
        }
        Message::MarkReadSelected => {
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
        }
        Message::TrashSelected => {
            if let Some(body) = app.selected_body.as_ref() {
                let engine = app.engine.clone();
                let message_id = body.id.clone();
                app.selected_body = None;
                app.selected_render = None;
                app.attachment_preview = None;
                app.attachment_open = None;
                app.selected_thread = None;
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
        Message::SaveAccount => match account_config_from_form(app) {
            Ok(account) => {
                let engine = app.engine.clone();
                app.status = if app.editing_account_id.is_some() {
                    "Updating account".to_string()
                } else {
                    "Saving account".to_string()
                };
                Task::perform(
                    async move {
                        let _ = engine.send(EngineCommand::SaveAccount(account)).await;
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
    }
}

pub fn subscription(app: &App) -> Subscription<Message> {
    let mut receiver = app.engine.subscribe();

    Subscription::run_with_id(
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
    )
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
    let reader = if app.account_setup_visible {
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
                identity_name: &app.identity_name,
                identity_email: &app.identity_email,
                connection_status: &app.account_connection_status,
            },
        )]
        .height(Length::Fill)
        .spacing(10)
    } else {
        let mut reader_stack = column![].height(Length::Fill).spacing(10);
        if !app.conflicts.is_empty() {
            reader_stack = reader_stack.push(conflicts_view(&app.conflicts));
        }
        if !app.notifications.is_empty() {
            reader_stack = reader_stack.push(notifications_view(
                &app.notifications,
                app.unread_notifications,
            ));
        }
        reader_stack = reader_stack.push(crate::views::reader::view(
            app.selected_body.as_ref(),
            app.selected_render.as_ref(),
            app.attachment_preview.as_ref(),
            app.attachment_open.as_ref(),
            &app.attachment_transfers,
        ));
        reader_stack = reader_stack.push(crate::views::composer::view(
            &app.draft_to,
            &app.draft_subject,
            &app.draft_body,
            &app.send_queue,
        ));
        reader_stack
    };

    let left_actions = row![
        crate::components::action_bar::button_primary("Compose", Message::Compose),
        crate::components::action_bar::button_toolbar("Account", Message::AddAccount),
        crate::components::action_bar::button_toolbar("Sync", Message::SyncNow),
    ]
    .spacing(8);

    let right_actions = row![
        crate::components::action_bar::button_text("Archive", Message::ArchiveSelected),
        crate::components::action_bar::button_text("Mark read", Message::MarkReadSelected),
        crate::components::action_bar::button_text("Trash", Message::TrashSelected),
        crate::components::badge::count(app.unread_notifications),
        crate::components::action_bar::button_text("Clear", Message::ClearNotifications),
        crate::components::status_bar::view(&app.status),
    ]
    .spacing(4)
    .align_y(iced::Alignment::Center);

    let toolbar = crate::components::surface::toolbar_surface(
        crate::components::action_bar::toolbar(left_actions, right_actions),
    );

    let thread_column = column![crate::components::search::view(&app.search_query), threads,]
        .spacing(8)
        .padding(8);

    let content = row![
        crate::components::surface::pane(mailboxes)
            .width(Length::Fixed(crate::theme::SIDEBAR_WIDTH)),
        crate::components::surface::pane(thread_column)
            .width(Length::Fixed(crate::theme::THREAD_LIST_WIDTH)),
        crate::components::surface::pane(reader).width(Length::Fill),
    ]
    .height(Length::Fill)
    .spacing(0);

    crate::components::surface::app_background(
        column![toolbar, content]
            .spacing(0)
            .padding(crate::theme::APP_PADDING),
    )
    .into()
}

pub fn theme(_app: &App) -> Theme {
    Theme::Light
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

fn notifications_view<'a>(
    notifications: &'a [DesktopNotification],
    unread: u32,
) -> Element<'a, Message> {
    let mut content = column![
        row![
            crate::components::list::section_label("Notifications"),
            iced::widget::horizontal_space(),
            crate::components::badge::count(unread),
        ]
        .align_y(iced::Alignment::Center)
    ]
    .spacing(6)
    .padding([8, 12]);

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
        EngineEvent::SendResult { result, .. } => {
            app.status = match result {
                Ok(()) => {
                    app.draft_to.clear();
                    app.draft_subject.clear();
                    app.draft_body.clear();
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
