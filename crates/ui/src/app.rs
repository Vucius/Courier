use std::path::PathBuf;

use iced::widget::{button, column, container, row};
use iced::{Element, Length, Task, Theme};
use mailcore::{EngineConfig, EngineHandle, spawn_engine};
use mailproto::{AccountId, EngineCommand, FolderRole, FolderSummary, MessageBody, ThreadSummary};

#[derive(Debug, Clone)]
pub enum Message {
    SyncNow,
    SyncQueued,
}

pub struct App {
    engine: EngineHandle,
    folders: Vec<FolderSummary>,
    threads: Vec<ThreadSummary>,
    selected_body: Option<MessageBody>,
    status: String,
}

pub fn init() -> (App, Task<Message>) {
    let data_dir = default_data_dir();
    let (engine, _join) = spawn_engine(EngineConfig { data_dir });

    let app = App {
        engine,
        folders: demo_folders(),
        threads: Vec::new(),
        selected_body: None,
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
    }
}

pub fn view(app: &App) -> Element<'_, Message> {
    let folders = crate::views::folder_list::view(&app.folders);
    let threads = crate::views::thread_list::view(&app.threads);
    let reader = column![
        crate::views::reader::view(app.selected_body.as_ref()),
        crate::views::composer::view()
    ]
    .spacing(16);

    let toolbar = row![
        button("Sync").on_press(Message::SyncNow),
        crate::components::status_bar::view(&app.status)
    ]
    .spacing(12)
    .align_y(iced::Alignment::Center);

    let content = row![
        container(folders).width(Length::Fixed(220.0)),
        container(threads).width(Length::FillPortion(2)),
        container(reader).width(Length::FillPortion(3)),
    ]
    .height(Length::Fill)
    .spacing(1);

    container(
        column![toolbar, content]
            .spacing(8)
            .padding(crate::theme::APP_PADDING),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

pub fn theme(_app: &App) -> Theme {
    Theme::Light
}

fn demo_folders() -> Vec<FolderSummary> {
    vec![
        FolderSummary {
            id: mailproto::FolderId("local-demo:inbox".to_string()),
            account_id: AccountId("local-demo".to_string()),
            name: "Inbox".to_string(),
            role: FolderRole::Inbox,
            unread_count: 0,
        },
        FolderSummary {
            id: mailproto::FolderId("local-demo:sent".to_string()),
            account_id: AccountId("local-demo".to_string()),
            name: "Sent".to_string(),
            role: FolderRole::Sent,
            unread_count: 0,
        },
    ]
}

fn default_data_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".mailspring-rust")
}
