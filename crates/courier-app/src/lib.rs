use std::path::PathBuf;

use courier_proto::{
    AccountId, EngineCommand, EngineEvent, MailboxId, MessageBody, MessageId, TaskId, ThreadId,
    ThreadSummary,
};
use courier_search::SearchIndex;
use courier_storage::Storage;
use courier_sync::SyncScheduler;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("engine command channel is closed")]
    CommandChannelClosed,
    #[error("storage error: {0}")]
    Storage(#[from] courier_storage::Error),
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

pub struct MailService;
pub struct InboxService;
pub struct DraftService;
pub struct AccountService;
pub struct AttachmentService;

struct EngineRuntime {
    config: EngineConfig,
    event_tx: broadcast::Sender<EngineEvent>,
    command_rx: mpsc::Receiver<EngineCommand>,
}

impl EngineRuntime {
    async fn run(mut self) -> Result<()> {
        let storage = Storage::open(self.config.data_dir.clone())?;
        storage.initialize()?;

        let sync = SyncScheduler::new(storage.clone());
        let search = SearchIndex::new(storage);

        let _ = self.event_tx.send(EngineEvent::Ready);

        while let Some(command) = self.command_rx.recv().await {
            self.handle_command(command, &sync, &search).await;
        }

        Ok(())
    }

    async fn handle_command(
        &self,
        command: EngineCommand,
        sync: &SyncScheduler,
        search: &SearchIndex,
    ) {
        match command {
            EngineCommand::SyncNow(account_id) => {
                let _ = sync.sync_now(account_id.clone()).await;
                self.simulate_sync(account_id).await;
            }
            EngineCommand::MarkRead(message_id, read) => {
                tracing::info!(?message_id, read, "queued local-first mark-read op");
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
                let results = search.query(&query).await;
                let _ = self.event_tx.send(EngineEvent::ThreadsUpdated(results));
            }
        }
    }

    async fn simulate_sync(&self, account_id: AccountId) {
        let _ = self.event_tx.send(EngineEvent::SyncProgress {
            account_id: account_id.clone(),
            progress: 0.25,
        });

        let inbox = MailboxId(format!("{}:inbox", account_id.0));
        let message_id = MessageId("demo-message".to_string());
        let thread = ThreadSummary {
            id: ThreadId("demo-thread".to_string()),
            account_id: account_id.clone(),
            subject: "Welcome to Courier".to_string(),
            sender: "courier@example.test".to_string(),
            snippet: "The local-first engine skeleton is running.".to_string(),
            unread: true,
            last_message_ts: 0,
        };

        let _ = self
            .event_tx
            .send(EngineEvent::ThreadsUpdated(vec![thread]));
        let _ = self.event_tx.send(EngineEvent::NewMessages {
            mailbox_id: inbox,
            messages: vec![message_id.clone()],
        });
        let _ = self.event_tx.send(EngineEvent::MessageLoaded(MessageBody {
            id: message_id,
            thread_id: ThreadId("demo-thread".to_string()),
            subject: "Welcome to Courier".to_string(),
            from: "courier@example.test".to_string(),
            to: vec!["you@example.test".to_string()],
            content_type: "text/plain".to_string(),
            body: "This placeholder is rendered from local storage first; remote mail servers remain the sync authority.".to_string(),
        }));

        let _ = self.event_tx.send(EngineEvent::SyncProgress {
            account_id,
            progress: 1.0,
        });
    }
}
