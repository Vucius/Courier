use std::path::PathBuf;

use courier_proto::{
    AccountId, AccountSummary, EngineCommand, EngineEvent, MailboxId, MailboxRole, MailboxSummary,
    MessageBody, MessageId, ProviderKind, TaskId, ThreadId, ThreadSummary,
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
        seed_demo_data(&storage)?;

        let sync = SyncScheduler::new(storage.clone());
        let search = SearchIndex::new(storage.clone());

        let _ = self.event_tx.send(EngineEvent::Ready);
        self.publish_snapshot(&storage);

        while let Some(command) = self.command_rx.recv().await {
            self.handle_command(command, &storage, &sync, &search).await;
        }

        Ok(())
    }

    async fn handle_command(
        &self,
        command: EngineCommand,
        storage: &Storage,
        sync: &SyncScheduler,
        search: &SearchIndex,
    ) {
        match command {
            EngineCommand::SyncNow(account_id) => {
                let _ = self.event_tx.send(EngineEvent::SyncProgress {
                    account_id: account_id.clone(),
                    progress: 0.25,
                });

                match sync.sync_now(account_id.clone()).await {
                    Ok(report) => {
                        tracing::info!(
                            account_id = %report.account_id.0,
                            pending_ops = report.pending_ops,
                            applied_ops = report.applied_ops,
                            remote_messages = report.remote_messages,
                            "sync queue flushed"
                        );
                        self.publish_snapshot(storage);
                        for update in report.mailbox_updates {
                            let _ = self.event_tx.send(EngineEvent::NewMessages {
                                mailbox_id: update.mailbox_id,
                                messages: update.message_ids,
                            });
                        }
                        let _ = self.event_tx.send(EngineEvent::SyncProgress {
                            account_id,
                            progress: 1.0,
                        });
                    }
                    Err(error) => {
                        let _ = self.event_tx.send(EngineEvent::Error(error.to_string()));
                    }
                }
            }
            EngineCommand::ListThreads { mailbox_id, query } => {
                let results = if query.trim().is_empty() {
                    storage.list_threads_for_mailbox(mailbox_id.as_ref())
                } else {
                    storage.search_threads_for_mailbox(&query, mailbox_id.as_ref())
                };

                match results {
                    Ok(threads) => {
                        let _ = self.event_tx.send(EngineEvent::ThreadsUpdated(threads));
                    }
                    Err(error) => {
                        let _ = self.event_tx.send(EngineEvent::Error(error.to_string()));
                    }
                }
            }
            EngineCommand::LoadThread(thread_id) => {
                match storage.load_message_for_thread(&thread_id) {
                    Ok(Some(body)) => {
                        let _ = self.event_tx.send(EngineEvent::MessageLoaded(body));
                    }
                    Ok(None) => {
                        let _ = self.event_tx.send(EngineEvent::Error(format!(
                            "No message body found for thread {}",
                            thread_id.0
                        )));
                    }
                    Err(error) => {
                        let _ = self.event_tx.send(EngineEvent::Error(error.to_string()));
                    }
                }
            }
            EngineCommand::MarkRead(message_id, read) => {
                match storage.mark_message_read(&message_id, read) {
                    Ok(()) => {
                        tracing::info!(?message_id, read, "queued local-first mark-read op");
                        self.publish_snapshot(storage);
                    }
                    Err(error) => {
                        let _ = self.event_tx.send(EngineEvent::Error(error.to_string()));
                    }
                }
            }
            EngineCommand::ArchiveMessage(message_id) => {
                match storage.move_message_to_mailbox_role(&message_id, MailboxRole::Archive) {
                    Ok(()) => {
                        tracing::info!(?message_id, "queued local-first archive op");
                        self.publish_snapshot(storage);
                    }
                    Err(error) => {
                        let _ = self.event_tx.send(EngineEvent::Error(error.to_string()));
                    }
                }
            }
            EngineCommand::MoveToTrash(message_id) => {
                match storage.move_message_to_mailbox_role(&message_id, MailboxRole::Trash) {
                    Ok(()) => {
                        tracing::info!(?message_id, "queued local-first trash op");
                        self.publish_snapshot(storage);
                    }
                    Err(error) => {
                        let _ = self.event_tx.send(EngineEvent::Error(error.to_string()));
                    }
                }
            }
            EngineCommand::SendMessage(draft_id) => {
                let task_id = TaskId(format!("send:{}", draft_id.0));
                match sync.send_draft(draft_id).await {
                    Ok(report) => {
                        tracing::info!(
                            task_id = %report.task_id.0,
                            message_id = %report.message_id.0,
                            remote_id = ?report.remote_id,
                            "draft sent through sync scheduler"
                        );
                        self.publish_snapshot(storage);
                        let _ = self.event_tx.send(EngineEvent::SendResult {
                            task_id: report.task_id,
                            result: Ok(()),
                        });
                    }
                    Err(error) => {
                        let _ = self.event_tx.send(EngineEvent::SendResult {
                            task_id,
                            result: Err(error.to_string()),
                        });
                    }
                }
            }
            EngineCommand::SaveDraft(draft) => match storage.save_draft(&draft) {
                Ok(()) => tracing::info!(draft_id = ?draft.id, "queued draft save"),
                Err(error) => {
                    let _ = self.event_tx.send(EngineEvent::Error(error.to_string()));
                }
            },
            EngineCommand::Snooze(message_id, run_at) => {
                tracing::info!(?message_id, run_at, "queued snooze command");
            }
            EngineCommand::Search(query) => {
                let results = if query.trim().is_empty() {
                    storage.list_threads().unwrap_or_default()
                } else {
                    search.query(&query).await
                };
                let _ = self.event_tx.send(EngineEvent::ThreadsUpdated(results));
            }
        }
    }

    fn publish_snapshot(&self, storage: &Storage) {
        match storage.list_mailboxes() {
            Ok(mailboxes) => {
                let _ = self.event_tx.send(EngineEvent::MailboxesUpdated(mailboxes));
            }
            Err(error) => {
                let _ = self.event_tx.send(EngineEvent::Error(error.to_string()));
            }
        }

        match storage.list_threads() {
            Ok(threads) => {
                let _ = self.event_tx.send(EngineEvent::ThreadsUpdated(threads));
            }
            Err(error) => {
                let _ = self.event_tx.send(EngineEvent::Error(error.to_string()));
            }
        }
    }
}

fn seed_demo_data(storage: &Storage) -> Result<()> {
    let account = AccountSummary {
        id: AccountId("local-demo".to_string()),
        email: "you@example.test".to_string(),
        provider: ProviderKind::GenericImap,
    };

    storage.upsert_account(&account)?;

    let mailboxes = [
        MailboxSummary {
            id: MailboxId("local-demo:inbox".to_string()),
            account_id: account.id.clone(),
            name: "Inbox".to_string(),
            role: MailboxRole::Inbox,
            unread_count: 0,
        },
        MailboxSummary {
            id: MailboxId("local-demo:sent".to_string()),
            account_id: account.id.clone(),
            name: "Sent".to_string(),
            role: MailboxRole::Sent,
            unread_count: 0,
        },
        MailboxSummary {
            id: MailboxId("local-demo:drafts".to_string()),
            account_id: account.id.clone(),
            name: "Drafts".to_string(),
            role: MailboxRole::Drafts,
            unread_count: 0,
        },
        MailboxSummary {
            id: MailboxId("local-demo:archive".to_string()),
            account_id: account.id.clone(),
            name: "Archive".to_string(),
            role: MailboxRole::Archive,
            unread_count: 0,
        },
        MailboxSummary {
            id: MailboxId("local-demo:trash".to_string()),
            account_id: account.id.clone(),
            name: "Trash".to_string(),
            role: MailboxRole::Trash,
            unread_count: 0,
        },
    ];

    for mailbox in &mailboxes {
        storage.upsert_mailbox(mailbox)?;
    }

    let inbox = MailboxId("local-demo:inbox".to_string());
    let threads = demo_threads(account.id.clone());
    for (thread, body) in threads {
        storage.upsert_thread(&thread)?;
        storage.upsert_message(&inbox, &thread, &body)?;
    }

    Ok(())
}

fn demo_threads(account_id: AccountId) -> Vec<(ThreadSummary, MessageBody)> {
    let rows = [
        (
            "thread:roadmap",
            "message:roadmap",
            "Courier local-first roadmap",
            "Design Notes",
            "Storage, search, and event delivery are now wired through the engine.",
            "The UI can render this message from local SQLite-backed storage. Search uses the FTS table, and sync commands publish engine events back into the desktop shell.",
            "text/plain",
            true,
            1_780_214_400,
        ),
        (
            "thread:sync",
            "message:sync",
            "Sync engine status",
            "Local Demo",
            "The engine booted and is ready for local-first commands.",
            "This message is seeded through courier-app on startup so the UI has a realistic local snapshot before any remote account is configured.",
            "text/plain",
            false,
            1_780_210_800,
        ),
        (
            "thread:security",
            "message:security",
            "Attachment policy review",
            "Security",
            "Keep attachment and remote content decisions visible inside the reader surface.",
            r#"<p>Courier keeps the render path native.</p><p><a href="https://example.test/policy">Review policy</a></p><img src="https://example.test/pixel.png"><script>alert("blocked")</script>"#,
            "text/html",
            false,
            1_780_124_400,
        ),
    ];

    rows.into_iter()
        .map(
            |(
                thread_id,
                message_id,
                subject,
                sender,
                snippet,
                body,
                content_type,
                unread,
                timestamp,
            )| {
                let thread = ThreadSummary {
                    id: ThreadId(thread_id.to_string()),
                    account_id: account_id.clone(),
                    subject: subject.to_string(),
                    sender: sender.to_string(),
                    snippet: snippet.to_string(),
                    unread,
                    last_message_ts: timestamp,
                };
                let body = MessageBody {
                    id: MessageId(message_id.to_string()),
                    thread_id: thread.id.clone(),
                    subject: subject.to_string(),
                    from: sender.to_string(),
                    to: vec!["you@example.test".to_string()],
                    content_type: content_type.to_string(),
                    body: body.to_string(),
                };
                (thread, body)
            },
        )
        .collect()
}
