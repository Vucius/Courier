use std::path::PathBuf;

use courier_app::{EngineConfig, EngineHandle, spawn_engine};
use courier_proto::{
    AccountId, DraftId, DraftMessage, EngineCommand, EngineEvent, MailboxId, MailboxSummary,
    MessageBody, ThreadId, ThreadSummary,
};
use courier_render::{RenderTree, render_tree_from_html, render_tree_from_text};
use iced::futures::SinkExt;
use iced::widget::{column, row};
use iced::{Element, Length, Subscription, Task, Theme};

#[derive(Debug, Clone)]
pub enum Message {
    SyncNow,
    SyncQueued,
    EngineEvent(EngineEvent),
    MailboxSelected(Option<MailboxId>, String),
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
}

pub struct App {
    engine: EngineHandle,
    mailboxes: Vec<MailboxSummary>,
    threads: Vec<ThreadSummary>,
    selected_mailbox_id: Option<MailboxId>,
    selected_mailbox_name: String,
    selected_thread: Option<ThreadId>,
    selected_body: Option<MessageBody>,
    selected_render: Option<RenderTree>,
    search_query: String,
    draft_to: String,
    draft_subject: String,
    draft_body: String,
    status: String,
}

pub fn init() -> (App, Task<Message>) {
    let data_dir = default_data_dir();
    let (engine, _join) = spawn_engine(EngineConfig { data_dir });

    let app = App {
        engine,
        mailboxes: Vec::new(),
        threads: Vec::new(),
        selected_mailbox_id: None,
        selected_mailbox_name: "Unified Inbox".to_string(),
        selected_thread: None,
        selected_body: None,
        selected_render: None,
        search_query: String::new(),
        draft_to: String::new(),
        draft_subject: String::new(),
        draft_body: String::new(),
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
            app.selected_mailbox_id = mailbox_id.clone();
            app.selected_mailbox_name = name.clone();
            app.selected_body = None;
            app.selected_render = None;
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
        Message::Compose => {
            app.status = "Draft ready".to_string();
            Task::none()
        }
        Message::ArchiveSelected => {
            if let Some(body) = app.selected_body.as_ref() {
                let engine = app.engine.clone();
                let message_id = body.id.clone();
                app.selected_body = None;
                app.selected_render = None;
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
            app.selected_thread = Some(thread_id.clone());
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
                        let draft_id = draft.id.clone();
                        let _ = engine.send(EngineCommand::SaveDraft(draft)).await;
                        let _ = engine.send(EngineCommand::SendMessage(draft_id)).await;
                    },
                    |_| Message::SyncQueued,
                )
            }
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
    let reader = column![
        crate::views::reader::view(app.selected_body.as_ref(), app.selected_render.as_ref()),
        crate::views::composer::view(&app.draft_to, &app.draft_subject, &app.draft_body,)
    ]
    .height(Length::Fill)
    .spacing(10);

    let left_actions = row![
        crate::components::action_bar::button_primary("Compose", Message::Compose),
        crate::components::action_bar::button_toolbar("Sync", Message::SyncNow),
    ]
    .spacing(8);

    let right_actions = row![
        crate::components::action_bar::button_text("Archive", Message::ArchiveSelected),
        crate::components::action_bar::button_text("Mark read", Message::MarkReadSelected),
        crate::components::action_bar::button_text("Trash", Message::TrashSelected),
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

fn handle_engine_event(app: &mut App, event: EngineEvent) -> Task<Message> {
    match event {
        EngineEvent::Ready => {
            app.status = "Engine ready".to_string();
        }
        EngineEvent::MailboxesUpdated(mailboxes) => {
            app.mailboxes = mailboxes;
            app.status = "Mailboxes loaded".to_string();
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

fn default_data_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".courier")
}
