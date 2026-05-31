pub mod account;
pub mod imap;
pub mod model;
pub mod search;
pub mod smtp;
pub mod storage;
pub mod sync;

use std::path::PathBuf;

use mailproto::{
    AccountId, EngineCommand, EngineEvent, FolderId, MessageBody, MessageId, TaskId, ThreadId,
    ThreadSummary,
};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("engine command channel is closed")]
    CommandChannelClosed,
    #[error("storage error: {0}")]
    Storage(String),
}

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub data_dir: PathBuf,
}

#[derive(Clone)]
pub struct EngineHandle {
    command_tx: mpsc::Sender<EngineCommand>,
    event_tx: broadcast::Sender<EngineEvent>,
}

impl EngineHandle {
    pub async fn send(&self, command: EngineCommand) -> Result<()> {
        self.command_tx
            .send(command)
            .await
            .map_err(|_| Error::CommandChannelClosed)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        self.event_tx.subscribe()
    }
}

pub fn spawn_engine(config: EngineConfig) -> (EngineHandle, JoinHandle<Result<()>>) {
    let (command_tx, command_rx) = mpsc::channel(128);
    let (event_tx, _) = broadcast::channel(256);
    let handle = EngineHandle {
        command_tx,
        event_tx: event_tx.clone(),
    };

    let runtime = EngineRuntime {
        config,
        event_tx,
        command_rx,
    };
    let join = tokio::spawn(runtime.run());

    (handle, join)
}

struct EngineRuntime {
    config: EngineConfig,
    event_tx: broadcast::Sender<EngineEvent>,
    command_rx: mpsc::Receiver<EngineCommand>,
}

impl EngineRuntime {
    async fn run(mut self) -> Result<()> {
        let storage = storage::Storage::open(self.config.data_dir.clone()).await?;
        storage.initialize().await?;

        let _ = self.event_tx.send(EngineEvent::Ready);

        while let Some(command) = self.command_rx.recv().await {
            self.handle_command(command).await;
        }

        Ok(())
    }

    async fn handle_command(&self, command: EngineCommand) {
        match command {
            EngineCommand::SyncNow(account_id) => self.simulate_sync(account_id).await,
            EngineCommand::MarkRead(message_id, read) => {
                tracing::info!(?message_id, read, "queued mark-read command");
            }
            EngineCommand::SendMessage(draft_id) => {
                tracing::info!(?draft_id, "queued send command");
                let task_id = TaskId(format!("send:{}", draft_id.0));
                let _ = self.event_tx.send(EngineEvent::SendResult {
                    task_id,
                    result: Ok(()),
                });
            }
            EngineCommand::SaveDraft(draft) => {
                tracing::info!(draft_id = ?draft.id, "queued draft save");
            }
            EngineCommand::Snooze(message_id, run_at) => {
                tracing::info!(?message_id, run_at, "queued snooze command");
            }
            EngineCommand::Search(query) => {
                tracing::info!(query, "queued search command");
            }
        }
    }

    async fn simulate_sync(&self, account_id: AccountId) {
        let _ = self.event_tx.send(EngineEvent::SyncProgress {
            account_id: account_id.clone(),
            progress: 0.25,
        });

        let inbox = FolderId(format!("{}:inbox", account_id.0));
        let message_id = MessageId("demo-message".to_string());
        let thread = ThreadSummary {
            id: ThreadId("demo-thread".to_string()),
            account_id: account_id.clone(),
            subject: "Welcome to MailSpring Rust".to_string(),
            sender: "mailcore@example.test".to_string(),
            snippet: "The engine skeleton is running.".to_string(),
            unread: true,
            last_message_ts: 0,
        };

        let _ = self
            .event_tx
            .send(EngineEvent::ThreadsUpdated(vec![thread]));
        let _ = self.event_tx.send(EngineEvent::NewMessages {
            folder_id: inbox,
            messages: vec![message_id.clone()],
        });
        let _ = self.event_tx.send(EngineEvent::MessageLoaded(MessageBody {
            id: message_id,
            thread_id: ThreadId("demo-thread".to_string()),
            subject: "Welcome to MailSpring Rust".to_string(),
            from: "mailcore@example.test".to_string(),
            to: vec!["you@example.test".to_string()],
            content_type: "text/plain".to_string(),
            body: "This is placeholder content until IMAP fetch is implemented.".to_string(),
        }));

        let _ = self.event_tx.send(EngineEvent::SyncProgress {
            account_id,
            progress: 1.0,
        });
    }
}
