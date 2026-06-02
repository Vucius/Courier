use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use courier_credential::{CredentialStore, UnsupportedCredentialStore, credential_ref};
use courier_proto::{
    AccountConnectionTestResult, AccountId, AccountSummary, AttachmentId, AttachmentOpenRequest,
    AttachmentSummary, AttachmentTransfer, AttachmentTransferStatus, CredentialKind,
    DesktopNotification, EndpointCheckResult, EngineCommand, EngineEvent, MailboxId, MailboxRole,
    MailboxSummary, MessageBody, MessageId, NotificationKind, OAuth2AuthorizationRequest,
    OAuth2Callback, ProviderKind, TaskId, ThreadId, ThreadSummary,
};
use courier_provider::{authorization_url, oauth2_client_config};
use courier_search::SearchIndex;
use courier_security::classify_attachment;
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
        notifications: NotificationPolicy::default(),
        attachment_transfers: HashMap::new(),
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
    notifications: NotificationPolicy,
    attachment_transfers: HashMap<String, AttachmentTransfer>,
}

#[derive(Default)]
struct NotificationPolicy {
    recent: Vec<NotificationRecord>,
}

struct NotificationRecord {
    key: String,
    created_at: i64,
}

impl NotificationPolicy {
    const DEDUPE_WINDOW_SECONDS: i64 = 30;
    const MAX_RECENT: usize = 64;

    fn should_publish(&mut self, notification: &DesktopNotification) -> bool {
        let now = notification.created_at;
        self.recent
            .retain(|record| now.saturating_sub(record.created_at) <= Self::DEDUPE_WINDOW_SECONDS);

        let key = notification_key(notification);
        if self.recent.iter().any(|record| record.key == key) {
            return false;
        }

        self.recent.push(NotificationRecord {
            key,
            created_at: now,
        });
        if self.recent.len() > Self::MAX_RECENT {
            let overflow = self.recent.len() - Self::MAX_RECENT;
            self.recent.drain(0..overflow);
        }

        true
    }
}

impl EngineRuntime {
    async fn run(mut self) -> Result<()> {
        let storage = Storage::open(self.config.data_dir.clone())?;
        storage.initialize()?;
        seed_demo_data(&storage)?;

        let sync = SyncScheduler::new(storage.clone());
        let search = SearchIndex::new(storage.clone());
        let mut send_queue_tick = tokio::time::interval(Duration::from_secs(5));

        let _ = self.event_tx.send(EngineEvent::Ready);
        self.publish_snapshot(&storage);

        loop {
            tokio::select! {
                command = self.command_rx.recv() => {
                    let Some(command) = command else {
                        break;
                    };
                    self.handle_command(command, &storage, &sync, &search).await;
                }
                _ = send_queue_tick.tick() => {
                    self.run_due_send_queue(&storage, &sync).await;
                }
            }
        }

        Ok(())
    }

    async fn handle_command(
        &mut self,
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
                        self.publish_conflicts(storage);
                        for update in report.mailbox_updates {
                            let _ = self.event_tx.send(EngineEvent::NewMessages {
                                mailbox_id: update.mailbox_id.clone(),
                                messages: update.message_ids.clone(),
                            });
                            if !update.message_ids.is_empty() {
                                self.publish_notification(DesktopNotification {
                                    id: format!(
                                        "new-mail:{}:{}",
                                        update.mailbox_id.0,
                                        unix_timestamp()
                                    ),
                                    kind: NotificationKind::NewMail,
                                    title: "New mail".to_string(),
                                    body: format!(
                                        "{} new message(s) arrived",
                                        update.message_ids.len()
                                    ),
                                    account_id: Some(account_id.clone()),
                                    message_ids: update.message_ids,
                                    created_at: unix_timestamp(),
                                });
                            }
                        }
                        let _ = self.event_tx.send(EngineEvent::SyncProgress {
                            account_id,
                            progress: 1.0,
                        });
                    }
                    Err(error) => {
                        self.publish_notification(DesktopNotification {
                            id: format!("sync-error:{}", unix_timestamp()),
                            kind: NotificationKind::Error,
                            title: "Sync failed".to_string(),
                            body: error.to_string(),
                            account_id: Some(account_id),
                            message_ids: Vec::new(),
                            created_at: unix_timestamp(),
                        });
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
                        self.publish_attachment_transfers_for_message(storage, &body.id);
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
            EngineCommand::SaveAccount(account) => match storage.upsert_account_config(&account) {
                Ok(()) => {
                    tracing::info!(
                        account_id = %account.id.0,
                        email = %account.email,
                        "saved account configuration"
                    );
                    self.publish_snapshot(storage);
                    let _ = self
                        .event_tx
                        .send(EngineEvent::AccountSaved(AccountSummary {
                            id: account.id,
                            email: account.email,
                            provider: account.provider,
                        }));
                }
                Err(error) => {
                    let _ = self.event_tx.send(EngineEvent::Error(error.to_string()));
                }
            },
            EngineCommand::SetAccountEnabled(account_id, enabled) => {
                match storage.set_account_enabled(&account_id, enabled) {
                    Ok(()) => {
                        tracing::info!(
                            account_id = %account_id.0,
                            enabled,
                            "updated account enabled state"
                        );
                        self.publish_snapshot(storage);
                    }
                    Err(error) => {
                        let _ = self.event_tx.send(EngineEvent::Error(error.to_string()));
                    }
                }
            }
            EngineCommand::DeleteAccount(account_id) => match storage.delete_account(&account_id) {
                Ok(()) => {
                    tracing::info!(account_id = %account_id.0, "deleted account");
                    self.publish_snapshot(storage);
                }
                Err(error) => {
                    let _ = self.event_tx.send(EngineEvent::Error(error.to_string()));
                }
            },
            EngineCommand::TestAccountConnection(account) => {
                let result = test_account_connection(&account).await;
                tracing::info!(
                    account_id = %account.id.0,
                    imap_ok = result.imap.ok,
                    smtp_ok = result.smtp.ok,
                    "tested account TCP connectivity"
                );
                let _ = self
                    .event_tx
                    .send(EngineEvent::AccountConnectionTested(result));
            }
            EngineCommand::BeginOAuth2(account_id) => {
                let result = self.begin_oauth2(storage, account_id);
                if let Ok(request) = result.as_ref() {
                    start_oauth2_loopback_listener(request.clone(), self.event_tx.clone());
                }
                let _ = self
                    .event_tx
                    .send(EngineEvent::OAuth2AuthorizationStarted(result));
            }
            EngineCommand::CompleteOAuth2(callback) => {
                let reference = credential_ref(
                    callback.account_id,
                    CredentialKind::OAuthRefreshToken,
                    "courier-oauth2",
                );
                let _ = self.event_tx.send(EngineEvent::OAuth2Completed(Err(format!(
                    "Token exchange is not implemented yet; reserved credential key {}",
                    reference.key
                ))));
            }
            EngineCommand::CredentialStatus => {
                let store = UnsupportedCredentialStore;
                let _ = self
                    .event_tx
                    .send(EngineEvent::CredentialStoreChecked(store.status()));
            }
            EngineCommand::SaveIdentity(identity) => match storage.upsert_identity(&identity) {
                Ok(()) => {
                    tracing::info!(
                        identity_id = %identity.id.0,
                        account_id = %identity.account_id.0,
                        "saved sending identity"
                    );
                    self.publish_snapshot(storage);
                    let _ = self.event_tx.send(EngineEvent::IdentitySaved(
                        courier_proto::IdentitySummary {
                            id: identity.id,
                            account_id: identity.account_id,
                            name: identity.name,
                            email: identity.email,
                            reply_to: identity.reply_to,
                        },
                    ));
                }
                Err(error) => {
                    let _ = self.event_tx.send(EngineEvent::Error(error.to_string()));
                }
            },
            EngineCommand::DeleteIdentity(identity_id) => {
                match storage.delete_identity(&identity_id) {
                    Ok(()) => {
                        tracing::info!(identity_id = %identity_id.0, "deleted sending identity");
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
                        self.publish_send_queue(storage);
                        let _ = self.event_tx.send(EngineEvent::SendResult {
                            task_id: report.task_id,
                            result: Ok(()),
                        });
                    }
                    Err(error) => {
                        self.publish_send_queue(storage);
                        let _ = self.event_tx.send(EngineEvent::SendResult {
                            task_id,
                            result: Err(error.to_string()),
                        });
                    }
                }
            }
            EngineCommand::SaveDraft(draft) => match storage.save_draft(&draft) {
                Ok(()) => {
                    tracing::info!(draft_id = ?draft.id, "queued draft save");
                    self.publish_send_queue(storage);
                    self.run_due_send_queue(storage, sync).await;
                }
                Err(error) => {
                    let _ = self.event_tx.send(EngineEvent::Error(error.to_string()));
                }
            },
            EngineCommand::ListSendQueue => {
                self.publish_send_queue(storage);
            }
            EngineCommand::ListConflicts => {
                self.publish_conflicts(storage);
            }
            EngineCommand::ResolveConflict(message_id, resolution) => {
                match storage.resolve_conflict(&message_id, resolution.clone()) {
                    Ok(()) => {
                        tracing::info!(
                            message_id = %message_id.0,
                            ?resolution,
                            "resolved sync conflict"
                        );
                        self.publish_snapshot(storage);
                        self.publish_conflicts(storage);
                    }
                    Err(error) => {
                        let _ = self.event_tx.send(EngineEvent::Error(error.to_string()));
                    }
                }
            }
            EngineCommand::RetrySend(draft_id) => {
                if let Err(error) = storage.mark_draft_pending_now(&draft_id) {
                    let _ = self.event_tx.send(EngineEvent::Error(error.to_string()));
                    return;
                }
                self.publish_send_queue(storage);
                let task_id = TaskId(format!("send:{}", draft_id.0));
                match sync.send_draft(draft_id).await {
                    Ok(report) => {
                        self.publish_snapshot(storage);
                        self.publish_send_queue(storage);
                        self.publish_notification(DesktopNotification {
                            id: format!("send-ok:{}", report.task_id.0),
                            kind: NotificationKind::Send,
                            title: "Message sent".to_string(),
                            body: format!("Draft {} was sent", report.draft_id.0),
                            account_id: None,
                            message_ids: vec![report.message_id.clone()],
                            created_at: unix_timestamp(),
                        });
                        let _ = self.event_tx.send(EngineEvent::SendResult {
                            task_id: report.task_id,
                            result: Ok(()),
                        });
                    }
                    Err(error) => {
                        self.publish_send_queue(storage);
                        self.publish_notification(DesktopNotification {
                            id: format!("send-error:{}", task_id.0),
                            kind: NotificationKind::Error,
                            title: "Send failed".to_string(),
                            body: error.to_string(),
                            account_id: None,
                            message_ids: Vec::new(),
                            created_at: unix_timestamp(),
                        });
                        let _ = self.event_tx.send(EngineEvent::SendResult {
                            task_id,
                            result: Err(error.to_string()),
                        });
                    }
                }
            }
            EngineCommand::CancelSend(draft_id) => match storage.cancel_draft_send(&draft_id) {
                Ok(()) => {
                    tracing::info!(draft_id = %draft_id.0, "cancelled draft send");
                    self.publish_send_queue(storage);
                }
                Err(error) => {
                    let _ = self.event_tx.send(EngineEvent::Error(error.to_string()));
                }
            },
            EngineCommand::RunDueSendQueue => {
                self.run_due_send_queue(storage, sync).await;
            }
            EngineCommand::PreviewAttachment(attachment_id) => {
                match storage.attachment_preview(&attachment_id, 64 * 1024) {
                    Ok(Some(preview)) => {
                        let _ = self
                            .event_tx
                            .send(EngineEvent::AttachmentPreviewLoaded(Ok(preview)));
                    }
                    Ok(None) => {
                        let _ =
                            self.event_tx
                                .send(EngineEvent::AttachmentPreviewLoaded(Err(format!(
                                    "Attachment not found: {}",
                                    attachment_id.0
                                ))));
                    }
                    Err(error) => {
                        let _ = self
                            .event_tx
                            .send(EngineEvent::AttachmentPreviewLoaded(Err(error.to_string())));
                    }
                }
            }
            EngineCommand::OpenAttachment(attachment_id) => {
                match attachment_open_request(storage, &attachment_id) {
                    Ok(Some(request)) => {
                        let _ = self
                            .event_tx
                            .send(EngineEvent::AttachmentOpenPrepared(request));
                    }
                    Ok(None) => publish_attachment_missing(&self.event_tx, &attachment_id),
                    Err(error) => {
                        let _ = self.event_tx.send(EngineEvent::Error(error));
                    }
                }
            }
            EngineCommand::ConfirmOpenAttachment(attachment_id) => {
                match attachment_open_request(storage, &attachment_id) {
                    Ok(Some(request)) if request.allowed => {
                        if let Some(path) = request.path.as_deref() {
                            match open_with_system_default(path) {
                                Ok(()) => {
                                    let _ = self
                                        .event_tx
                                        .send(EngineEvent::AttachmentOpenExecuted(Ok(request)));
                                }
                                Err(error) => {
                                    let _ = self.event_tx.send(
                                        EngineEvent::AttachmentOpenExecuted(Err(error.to_string())),
                                    );
                                }
                            }
                        } else {
                            let _ = self.event_tx.send(EngineEvent::AttachmentOpenExecuted(Err(
                                format!(
                                    "{} has no local file to open",
                                    request.attachment.filename
                                ),
                            )));
                        }
                    }
                    Ok(Some(request)) => {
                        let _ = self
                            .event_tx
                            .send(EngineEvent::AttachmentOpenExecuted(Err(request.reason)));
                    }
                    Ok(None) => publish_attachment_missing(&self.event_tx, &attachment_id),
                    Err(error) => {
                        let _ = self
                            .event_tx
                            .send(EngineEvent::AttachmentOpenExecuted(Err(error)));
                    }
                }
            }
            EngineCommand::DownloadAttachment(attachment_id) => {
                self.mark_attachment_downloading(storage, &attachment_id);
                match storage.attachment_transfer(&attachment_id) {
                    Ok(Some(mut transfer)) => {
                        if matches!(transfer.status, AttachmentTransferStatus::Missing) {
                            transfer.status = AttachmentTransferStatus::Failed;
                            transfer.progress = 0.0;
                            transfer.message =
                                "Remote attachment fetch waits for a real protocol adapter"
                                    .to_string();
                        }
                        self.upsert_attachment_transfer(transfer);
                    }
                    Ok(None) => publish_attachment_missing(&self.event_tx, &attachment_id),
                    Err(error) => {
                        let _ = self.event_tx.send(EngineEvent::Error(error.to_string()));
                    }
                }
            }
            EngineCommand::CancelAttachmentDownload(attachment_id) => {
                match storage.attachment_transfer(&attachment_id) {
                    Ok(Some(mut transfer)) => {
                        transfer.status = AttachmentTransferStatus::Cancelled;
                        transfer.progress = 0.0;
                        transfer.message = "Attachment download cancelled".to_string();
                        self.upsert_attachment_transfer(transfer);
                    }
                    Ok(None) => publish_attachment_missing(&self.event_tx, &attachment_id),
                    Err(error) => {
                        let _ = self.event_tx.send(EngineEvent::Error(error.to_string()));
                    }
                }
            }
            EngineCommand::RetryAttachmentDownload(attachment_id) => {
                self.mark_attachment_downloading(storage, &attachment_id);
                match storage.attachment_transfer(&attachment_id) {
                    Ok(Some(mut transfer)) => {
                        if matches!(transfer.status, AttachmentTransferStatus::Missing) {
                            transfer.status = AttachmentTransferStatus::Failed;
                            transfer.progress = 0.0;
                            transfer.message =
                                "Remote attachment fetch waits for a real protocol adapter"
                                    .to_string();
                        }
                        self.upsert_attachment_transfer(transfer);
                    }
                    Ok(None) => publish_attachment_missing(&self.event_tx, &attachment_id),
                    Err(error) => {
                        let _ = self.event_tx.send(EngineEvent::Error(error.to_string()));
                    }
                }
            }
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

    fn publish_send_queue(&self, storage: &Storage) {
        match storage.list_send_queue() {
            Ok(queue) => {
                let _ = self.event_tx.send(EngineEvent::SendQueueUpdated(queue));
            }
            Err(error) => {
                let _ = self.event_tx.send(EngineEvent::Error(error.to_string()));
            }
        }
    }

    fn publish_attachment_transfers_for_message(
        &mut self,
        storage: &Storage,
        message_id: &MessageId,
    ) {
        match storage.attachment_transfers_for_message(message_id) {
            Ok(transfers) => {
                for transfer in transfers {
                    self.attachment_transfers
                        .insert(transfer.attachment.id.0.clone(), transfer);
                }
                self.publish_attachment_transfers();
            }
            Err(error) => {
                let _ = self.event_tx.send(EngineEvent::Error(error.to_string()));
            }
        }
    }

    fn mark_attachment_downloading(&mut self, storage: &Storage, attachment_id: &AttachmentId) {
        match storage.attachment_transfer(attachment_id) {
            Ok(Some(mut transfer)) => {
                transfer.status = AttachmentTransferStatus::Downloading;
                transfer.progress = 0.25;
                transfer.message = "Checking local attachment availability".to_string();
                self.upsert_attachment_transfer(transfer);
            }
            Ok(None) => publish_attachment_missing(&self.event_tx, attachment_id),
            Err(error) => {
                let _ = self.event_tx.send(EngineEvent::Error(error.to_string()));
            }
        }
    }

    fn upsert_attachment_transfer(&mut self, transfer: AttachmentTransfer) {
        self.attachment_transfers
            .insert(transfer.attachment.id.0.clone(), transfer);
        self.publish_attachment_transfers();
    }

    fn publish_attachment_transfers(&self) {
        let transfers = self
            .attachment_transfers
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let _ = self
            .event_tx
            .send(EngineEvent::AttachmentTransfersUpdated(transfers));
    }

    async fn run_due_send_queue(&mut self, storage: &Storage, sync: &SyncScheduler) {
        match sync.send_due_drafts(unix_timestamp(), 8).await {
            Ok(report) => {
                if report.attempted > 0 {
                    tracing::info!(
                        attempted = report.attempted,
                        sent = report.sent.len(),
                        failed = report.failed.len(),
                        "drained due send queue"
                    );
                    self.publish_snapshot(storage);
                    self.publish_send_queue(storage);
                    for sent in report.sent {
                        self.publish_notification(DesktopNotification {
                            id: format!("send-ok:{}", sent.task_id.0),
                            kind: NotificationKind::Send,
                            title: "Message sent".to_string(),
                            body: format!("Draft {} was sent", sent.draft_id.0),
                            account_id: None,
                            message_ids: vec![sent.message_id],
                            created_at: unix_timestamp(),
                        });
                    }
                    for (draft_id, error) in report.failed {
                        self.publish_notification(DesktopNotification {
                            id: format!("send-error:{}", draft_id.0),
                            kind: NotificationKind::Error,
                            title: "Send retry failed".to_string(),
                            body: error,
                            account_id: None,
                            message_ids: Vec::new(),
                            created_at: unix_timestamp(),
                        });
                    }
                }
            }
            Err(error) => {
                let _ = self.event_tx.send(EngineEvent::Error(error.to_string()));
            }
        }
    }

    fn begin_oauth2(
        &self,
        storage: &Storage,
        account_id: AccountId,
    ) -> std::result::Result<OAuth2AuthorizationRequest, String> {
        let account = storage
            .list_accounts()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|account| account.id == account_id)
            .ok_or_else(|| format!("Account not found: {}", account_id.0))?;
        let redirect_uri = "http://127.0.0.1:48176/oauth/callback";
        let client_id = std::env::var("COURIER_OAUTH_CLIENT_ID")
            .unwrap_or_else(|_| "configure-client-id".to_string());
        let config = oauth2_client_config(&account.provider, client_id, redirect_uri)
            .ok_or_else(|| "OAuth2 is not configured for this provider".to_string())?;
        if config.auth_url.is_empty() {
            return Err("OAuth2 authorization URL is not configured for this provider".to_string());
        }
        let state = format!("{}:{}", account.id.0, unix_timestamp());
        let auth_url = authorization_url(&config, &state);

        Ok(OAuth2AuthorizationRequest {
            account_id: account.id,
            provider: account.provider,
            auth_url,
            redirect_uri: redirect_uri.to_string(),
            state,
            scopes: config.scopes,
        })
    }

    fn publish_conflicts(&mut self, storage: &Storage) {
        match storage.list_conflicts() {
            Ok(conflicts) => {
                let conflict_count = conflicts.len();
                let _ = self.event_tx.send(EngineEvent::ConflictsUpdated(conflicts));
                if conflict_count > 0 {
                    self.publish_notification(DesktopNotification {
                        id: format!("conflicts:{}", unix_timestamp()),
                        kind: NotificationKind::Warning,
                        title: "Sync conflicts need review".to_string(),
                        body: format!("{conflict_count} message conflict(s) are waiting"),
                        account_id: None,
                        message_ids: Vec::new(),
                        created_at: unix_timestamp(),
                    });
                }
            }
            Err(error) => {
                let _ = self.event_tx.send(EngineEvent::Error(error.to_string()));
            }
        }
    }

    fn publish_notification(&mut self, notification: DesktopNotification) {
        if self.notifications.should_publish(&notification) {
            let _ = self
                .event_tx
                .send(EngineEvent::NotificationRaised(notification));
        }
    }

    fn publish_snapshot(&self, storage: &Storage) {
        match storage.list_accounts() {
            Ok(accounts) => {
                let _ = self.event_tx.send(EngineEvent::AccountsUpdated(accounts));
            }
            Err(error) => {
                let _ = self.event_tx.send(EngineEvent::Error(error.to_string()));
            }
        }

        match storage.list_identities() {
            Ok(identities) => {
                let _ = self
                    .event_tx
                    .send(EngineEvent::IdentitiesUpdated(identities));
            }
            Err(error) => {
                let _ = self.event_tx.send(EngineEvent::Error(error.to_string()));
            }
        }

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

async fn test_account_connection(
    account: &courier_proto::AccountConfig,
) -> AccountConnectionTestResult {
    AccountConnectionTestResult {
        account_id: account.id.clone(),
        imap: test_endpoint(&account.imap_host, account.imap_port).await,
        smtp: test_endpoint(&account.smtp_host, account.smtp_port).await,
    }
}

async fn test_endpoint(host: &str, port: u16) -> EndpointCheckResult {
    let address = format!("{host}:{port}");
    match tokio::time::timeout(
        Duration::from_secs(5),
        tokio::net::TcpStream::connect(&address),
    )
    .await
    {
        Ok(Ok(_stream)) => EndpointCheckResult {
            host: host.to_string(),
            port,
            ok: true,
            error: None,
        },
        Ok(Err(error)) => EndpointCheckResult {
            host: host.to_string(),
            port,
            ok: false,
            error: Some(error.to_string()),
        },
        Err(_) => EndpointCheckResult {
            host: host.to_string(),
            port,
            ok: false,
            error: Some("connection timed out".to_string()),
        },
    }
}

fn start_oauth2_loopback_listener(
    request: OAuth2AuthorizationRequest,
    event_tx: broadcast::Sender<EngineEvent>,
) {
    tokio::spawn(async move {
        match listen_for_oauth2_callback(request).await {
            Ok(callback) => {
                let _ = event_tx.send(EngineEvent::OAuth2Completed(Err(format!(
                    "Authorization code received for {}; token exchange is not implemented yet",
                    callback.account_id.0
                ))));
            }
            Err(error) => {
                let _ = event_tx.send(EngineEvent::OAuth2Completed(Err(error)));
            }
        }
    });
}

async fn listen_for_oauth2_callback(
    request: OAuth2AuthorizationRequest,
) -> std::result::Result<OAuth2Callback, String> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:48176")
        .await
        .map_err(|error| format!("OAuth2 loopback listener failed: {error}"))?;
    let (socket, _) = listener
        .accept()
        .await
        .map_err(|error| format!("OAuth2 callback accept failed: {error}"))?;

    socket
        .readable()
        .await
        .map_err(|error| format!("OAuth2 callback read failed: {error}"))?;
    let mut buffer = [0_u8; 4096];
    let bytes = socket
        .try_read(&mut buffer)
        .map_err(|error| format!("OAuth2 callback read failed: {error}"))?;
    let request_line = std::str::from_utf8(&buffer[..bytes])
        .map_err(|error| format!("OAuth2 callback was not UTF-8: {error}"))?
        .lines()
        .next()
        .unwrap_or_default();
    let callback = parse_oauth2_callback_request(&request, request_line)?;
    write_oauth2_callback_response(socket, true).await?;

    Ok(callback)
}

fn parse_oauth2_callback_request(
    expected: &OAuth2AuthorizationRequest,
    request_line: &str,
) -> std::result::Result<OAuth2Callback, String> {
    let target = request_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| "OAuth2 callback request target was missing".to_string())?;
    let query = target
        .split_once('?')
        .map(|(_, query)| query)
        .ok_or_else(|| "OAuth2 callback did not include a query string".to_string())?;
    let code = query_value(query, "code")
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "OAuth2 callback did not include an authorization code".to_string())?;
    let state = query_value(query, "state")
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "OAuth2 callback did not include state".to_string())?;

    if state != expected.state {
        return Err("OAuth2 callback state did not match the pending request".to_string());
    }

    Ok(OAuth2Callback {
        account_id: expected.account_id.clone(),
        code,
        state,
    })
}

fn query_value(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == key).then(|| percent_decode(value))
    })
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = String::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                output.push(' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = &value[index + 1..index + 3];
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    output.push(byte as char);
                    index += 3;
                } else {
                    output.push('%');
                    index += 1;
                }
            }
            byte => {
                output.push(byte as char);
                index += 1;
            }
        }
    }
    output
}

async fn write_oauth2_callback_response(
    socket: tokio::net::TcpStream,
    success: bool,
) -> std::result::Result<(), String> {
    let body = if success {
        "Courier received the authorization response. You can return to the app."
    } else {
        "Courier could not process the authorization response."
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    socket
        .writable()
        .await
        .map_err(|error| format!("OAuth2 callback response failed: {error}"))?;
    socket
        .try_write(response.as_bytes())
        .map_err(|error| format!("OAuth2 callback response failed: {error}"))?;
    Ok(())
}

fn attachment_open_request(
    storage: &Storage,
    attachment_id: &AttachmentId,
) -> std::result::Result<Option<AttachmentOpenRequest>, String> {
    storage
        .attachment_by_id(attachment_id)
        .map_err(|error| error.to_string())
        .map(|attachment| {
            attachment.map(|attachment| {
                let summary = AttachmentSummary::from(attachment.clone());
                let decision =
                    classify_attachment(&summary.filename, &summary.mime_type, summary.size);
                let path = attachment
                    .blob_path
                    .map(|path| storage.data_dir().join(path).to_string_lossy().into_owned());
                AttachmentOpenRequest {
                    attachment: summary,
                    path,
                    allowed: decision.can_open,
                    reason: decision.reason,
                }
            })
        })
}

fn publish_attachment_missing(
    event_tx: &broadcast::Sender<EngineEvent>,
    attachment_id: &AttachmentId,
) {
    let _ = event_tx.send(EngineEvent::Error(format!(
        "Attachment not found: {}",
        attachment_id.0
    )));
}

fn open_with_system_default(path: &str) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", path])
            .spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(path).spawn()?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open").arg(path).spawn()?;
    }
    Ok(())
}

fn unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
        .min(i64::MAX as u64) as i64
}

fn notification_key(notification: &DesktopNotification) -> String {
    format!(
        "{:?}|{}|{}|{}",
        notification.kind,
        notification.title,
        notification.body,
        notification
            .account_id
            .as_ref()
            .map(|account_id| account_id.0.as_str())
            .unwrap_or_default()
    )
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
                    attachments: Vec::new(),
                };
                (thread, body)
            },
        )
        .collect()
}
