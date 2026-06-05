use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use courier_adapter::{
    AttachmentFetchRequest, ConfiguredRemote, CredentialSecretResolver, MailRemote,
};
use courier_credential::{
    CredentialStore, OsCredentialStore, account_credential_refs, credential_ref,
};
use courier_proto::{
    AccountConfig, AccountConnectionTestResult, AccountId, AccountState, AccountSummary,
    AttachmentId, AttachmentOpenRequest, AttachmentSummary, AttachmentTransfer,
    AttachmentTransferStatus, AuthType, CredentialKind, CredentialSecret, DesktopNotification,
    EndpointCheckResult, EngineCommand, EngineEvent, MailboxId, MailboxRole, MailboxSummary,
    MessageBody, MessageId, NetworkStatus, NotificationKind, NotificationPolicyState,
    OAuth2AuthorizationRequest, OAuth2Callback, OAuth2ClientConfig, ProviderKind, TaskId, ThreadId,
    ThreadSummary,
};
use courier_provider::oauth2_client_config;
use courier_search::SearchIndex;
use courier_security::classify_attachment;
use courier_storage::Storage;
use courier_sync::SyncScheduler;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, CsrfToken, EndpointNotSet, EndpointSet,
    PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, RefreshToken, Scope, TokenResponse, TokenUrl,
    basic::BasicClient,
};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

pub type Result<T> = std::result::Result<T, Error>;
type ReadyOAuth2Client =
    BasicClient<EndpointSet, EndpointNotSet, EndpointNotSet, EndpointNotSet, EndpointSet>;

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
        command_tx: command_tx.clone(),
        event_tx: event_tx.clone(),
    };

    let runtime = EngineRuntime {
        config,
        event_tx,
        command_tx: command_tx.clone(),
        command_rx,
        notifications: NotificationPolicy::default(),
        attachment_transfers: HashMap::new(),
        pending_oauth2: HashMap::new(),
        network_online: true,
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
    command_tx: mpsc::Sender<EngineCommand>,
    command_rx: mpsc::Receiver<EngineCommand>,
    notifications: NotificationPolicy,
    attachment_transfers: HashMap<String, AttachmentTransfer>,
    pending_oauth2: HashMap<String, PendingOAuth2>,
    network_online: bool,
}

struct PendingOAuth2 {
    config: OAuth2ClientConfig,
    state: String,
    pkce_verifier: PkceCodeVerifier,
}

#[derive(Default)]
struct NotificationPolicy {
    recent: Vec<NotificationRecord>,
    quiet: bool,
    quiet_until: Option<i64>,
    suppressed_count: u32,
    last_suppressed_at: Option<i64>,
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
        self.expire_quiet(now);
        if self.is_quiet(now) && notification.kind != NotificationKind::Error {
            self.suppressed_count = self.suppressed_count.saturating_add(1);
            self.last_suppressed_at = Some(now);
            return false;
        }

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

    fn set_quiet(&mut self, quiet: bool) {
        self.quiet = quiet;
        self.quiet_until = None;
        if !quiet {
            self.suppressed_count = 0;
            self.last_suppressed_at = None;
        }
    }

    fn set_quiet_for(&mut self, seconds: i64, now: i64) {
        self.quiet = false;
        self.quiet_until = Some(now.saturating_add(seconds.max(0)));
    }

    fn expire_quiet(&mut self, now: i64) {
        if self.quiet_until.is_some_and(|until| until <= now) {
            self.quiet_until = None;
            self.suppressed_count = 0;
            self.last_suppressed_at = None;
        }
    }

    fn is_quiet(&self, now: i64) -> bool {
        self.quiet || self.quiet_until.is_some_and(|until| until > now)
    }

    fn state(&self) -> NotificationPolicyState {
        let now = unix_timestamp();
        let quiet = self.is_quiet(now);
        NotificationPolicyState {
            quiet,
            quiet_until: self.quiet_until.filter(|until| *until > now),
            suppressed_count: self.suppressed_count,
            last_suppressed_at: self.last_suppressed_at,
            reason: if quiet {
                if self.suppressed_count == 0 {
                    match self.quiet_until.filter(|until| *until > now) {
                        Some(until) => format!(
                            "Quiet notifications enabled for {} more minute(s)",
                            ((until - now) / 60).max(1)
                        ),
                        None => "Quiet notifications enabled".to_string(),
                    }
                } else {
                    format!(
                        "Quiet notifications enabled; {} notification(s) suppressed",
                        self.suppressed_count
                    )
                }
            } else {
                "Notifications enabled".to_string()
            },
        }
    }
}

impl EngineRuntime {
    async fn run(mut self) -> Result<()> {
        let storage = Storage::open(self.config.data_dir.clone())?;
        let migration_report = storage.initialize_with_report()?;
        tracing::info!(
            db_path = %migration_report.db_path.display(),
            migrations = ?migration_report.sql_migrations,
            compatibility_steps = ?migration_report.compatibility_steps,
            "storage migration runner completed"
        );
        seed_demo_data(&storage)?;

        let search = SearchIndex::new(storage.clone());
        let mut send_queue_tick = tokio::time::interval(Duration::from_secs(5));

        let _ = self.event_tx.send(EngineEvent::Ready);
        self.publish_notification_policy();
        self.publish_snapshot(&storage);

        loop {
            tokio::select! {
                command = self.command_rx.recv() => {
                    let Some(command) = command else {
                        break;
                    };
                    self.handle_command(command, &storage, &search).await;
                }
                _ = send_queue_tick.tick() => {
                    self.run_due_send_queue(&storage).await;
                }
            }
        }

        Ok(())
    }

    async fn handle_command(
        &mut self,
        command: EngineCommand,
        storage: &Storage,
        search: &SearchIndex,
    ) {
        match command {
            EngineCommand::SyncNow(account_id) => {
                if !self.network_online {
                    self.publish_network_paused("Sync skipped because Courier is offline");
                    let _ = self.event_tx.send(EngineEvent::Error(
                        "Network is offline; sync is paused".to_string(),
                    ));
                    return;
                }

                let _ = self.event_tx.send(EngineEvent::SyncProgress {
                    account_id: account_id.clone(),
                    progress: 0.25,
                });

                if let Err(error) = self.refresh_oauth2_if_needed(storage, &account_id).await {
                    tracing::warn!(
                        account_id = %account_id.0,
                        error = %error,
                        "oauth2 token refresh failed before sync"
                    );
                }

                let sync = match scheduler_for_account(storage, &account_id) {
                    Ok(sync) => sync,
                    Err(error) => {
                        self.publish_notification(DesktopNotification {
                            id: format!("sync-error:{}", unix_timestamp()),
                            kind: NotificationKind::Error,
                            title: "Sync failed".to_string(),
                            body: error,
                            account_id: Some(account_id),
                            message_ids: Vec::new(),
                            created_at: unix_timestamp(),
                        });
                        return;
                    }
                };

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
                    let store = OsCredentialStore::new();
                    for reference in account_credential_refs(account_id.clone()) {
                        if let Err(error) = store.delete_secret(&reference) {
                            tracing::warn!(
                                account_id = %account_id.0,
                                credential_key = %reference.key,
                                error = %error,
                                "failed to delete account credential"
                            );
                        }
                    }
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
                    start_oauth2_loopback_listener(
                        request.clone(),
                        self.command_tx.clone(),
                        self.event_tx.clone(),
                    );
                }
                let _ = self
                    .event_tx
                    .send(EngineEvent::OAuth2AuthorizationStarted(result));
            }
            EngineCommand::CompleteOAuth2(callback) => {
                let result = self.complete_oauth2(callback).await;
                let _ = self.event_tx.send(EngineEvent::OAuth2Completed(result));
            }
            EngineCommand::CredentialStatus => {
                let store = OsCredentialStore::new();
                let _ = self
                    .event_tx
                    .send(EngineEvent::CredentialStoreChecked(store.status()));
            }
            EngineCommand::SaveCredentialSecret(secret) => {
                let result = save_credential_secret(secret);
                let _ = self.event_tx.send(EngineEvent::CredentialSaved(result));
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
                if !self.network_online {
                    self.publish_send_queue(storage);
                    self.publish_network_paused("Send queued until network is online");
                    let _ = self.event_tx.send(EngineEvent::SendResult {
                        task_id,
                        result: Err("Network is offline; send remains queued".to_string()),
                    });
                    return;
                }

                let sync = match scheduler_for_draft(storage, &draft_id) {
                    Ok(sync) => sync,
                    Err(error) => {
                        self.publish_send_queue(storage);
                        let _ = self.event_tx.send(EngineEvent::SendResult {
                            task_id,
                            result: Err(error),
                        });
                        return;
                    }
                };

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
                    self.run_due_send_queue(storage).await;
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
                if !self.network_online {
                    self.publish_network_paused("Retry queued until network is online");
                    let _ = self.event_tx.send(EngineEvent::SendResult {
                        task_id,
                        result: Err("Network is offline; retry remains queued".to_string()),
                    });
                    return;
                }

                let sync = match scheduler_for_draft(storage, &draft_id) {
                    Ok(sync) => sync,
                    Err(error) => {
                        self.publish_send_queue(storage);
                        let _ = self.event_tx.send(EngineEvent::SendResult {
                            task_id,
                            result: Err(error),
                        });
                        return;
                    }
                };

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
                self.run_due_send_queue(storage).await;
            }
            EngineCommand::ProbeNetwork => {
                let status = match storage.list_accounts() {
                    Ok(accounts) => probe_network_status(&accounts).await,
                    Err(error) => NetworkStatus {
                        online: false,
                        reason: format!("Network probe failed: {error}"),
                        checked_at: unix_timestamp(),
                    },
                };
                let was_online = self.network_online;
                self.network_online = status.online;
                let _ = self
                    .event_tx
                    .send(EngineEvent::NetworkStatusChanged(status.clone()));
                if was_online != status.online {
                    self.publish_notification(DesktopNotification {
                        id: format!("network:{}:{}", status.online, status.checked_at),
                        kind: NotificationKind::Warning,
                        title: if status.online {
                            "Network restored".to_string()
                        } else {
                            "Network unavailable".to_string()
                        },
                        body: status.reason,
                        account_id: None,
                        message_ids: Vec::new(),
                        created_at: status.checked_at,
                    });
                }
            }
            EngineCommand::SetNetworkOnline(online) => {
                self.network_online = online;
                let status = NetworkStatus {
                    online,
                    reason: if online {
                        "Network sends and sync are enabled".to_string()
                    } else {
                        "Network sends and sync are paused".to_string()
                    },
                    checked_at: unix_timestamp(),
                };
                let _ = self
                    .event_tx
                    .send(EngineEvent::NetworkStatusChanged(status.clone()));
                self.publish_notification(DesktopNotification {
                    id: format!("network:{}", if online { "online" } else { "offline" }),
                    kind: if online {
                        NotificationKind::Sync
                    } else {
                        NotificationKind::Warning
                    },
                    title: if online {
                        "Network online".to_string()
                    } else {
                        "Network offline".to_string()
                    },
                    body: status.reason,
                    account_id: None,
                    message_ids: Vec::new(),
                    created_at: unix_timestamp(),
                });
            }
            EngineCommand::SetNotificationsQuiet(quiet) => {
                self.notifications.set_quiet(quiet);
                self.publish_notification_policy();
                if quiet {
                    let _ = self.event_tx.send(EngineEvent::Error(
                        "Quiet notifications enabled; errors will still appear".to_string(),
                    ));
                }
            }
            EngineCommand::SetNotificationsQuietFor(seconds) => {
                self.notifications.set_quiet_for(seconds, unix_timestamp());
                self.publish_notification_policy();
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
                self.fetch_attachment(storage, &attachment_id).await;
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
                self.fetch_attachment(storage, &attachment_id).await;
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
                transfer.message = "Preparing remote attachment fetch".to_string();
                self.upsert_attachment_transfer(transfer);
            }
            Ok(None) => publish_attachment_missing(&self.event_tx, attachment_id),
            Err(error) => {
                let _ = self.event_tx.send(EngineEvent::Error(error.to_string()));
            }
        }
    }

    async fn fetch_attachment(&mut self, storage: &Storage, attachment_id: &AttachmentId) {
        let transfer = match storage.attachment_transfer(attachment_id) {
            Ok(Some(transfer)) => transfer,
            Ok(None) => {
                publish_attachment_missing(&self.event_tx, attachment_id);
                return;
            }
            Err(error) => {
                let _ = self.event_tx.send(EngineEvent::Error(error.to_string()));
                return;
            }
        };

        let request = AttachmentFetchRequest {
            attachment_id: transfer.attachment.id.clone(),
            filename: transfer.attachment.filename.clone(),
            expected_size: transfer.attachment.size,
        };
        let remote = match remote_for_attachment(storage, &transfer.attachment.id) {
            Ok(remote) => remote,
            Err(error) => {
                let mut failed = transfer;
                failed.status = AttachmentTransferStatus::Failed;
                failed.progress = 0.0;
                failed.message = error;
                self.upsert_attachment_transfer(failed);
                return;
            }
        };

        match remote.fetch_attachment(request).await {
            Ok(result) => {
                let mut completed = transfer;
                completed.status = AttachmentTransferStatus::Ready;
                completed.progress = 1.0;
                completed.message = format!(
                    "Fetched {} byte(s) from remote attachment worker",
                    result.bytes.len()
                );
                self.upsert_attachment_transfer(completed);
            }
            Err(error) => {
                let mut failed = transfer;
                failed.status = AttachmentTransferStatus::Failed;
                failed.progress = 0.0;
                failed.message = error.to_string();
                self.upsert_attachment_transfer(failed);
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

    async fn run_due_send_queue(&mut self, storage: &Storage) {
        if !self.network_online {
            self.publish_network_paused("Due send queue paused while Courier is offline");
            self.publish_send_queue(storage);
            return;
        }

        match storage.due_draft_ids(unix_timestamp(), 8) {
            Ok(draft_ids) if !draft_ids.is_empty() => {
                let mut sent = Vec::new();
                let mut failed = Vec::new();

                for draft_id in draft_ids {
                    match scheduler_for_draft(storage, &draft_id) {
                        Ok(sync) => match sync.send_draft(draft_id.clone()).await {
                            Ok(report) => sent.push(report),
                            Err(error) => failed.push((draft_id, error.to_string())),
                        },
                        Err(error) => {
                            let _ = storage.mark_draft_failed(&draft_id, &error);
                            failed.push((draft_id, error));
                        }
                    }
                }

                tracing::info!(
                    attempted = sent.len() + failed.len(),
                    sent = sent.len(),
                    failed = failed.len(),
                    "drained due send queue"
                );
                self.publish_snapshot(storage);
                self.publish_send_queue(storage);
                for sent in sent {
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
                for (draft_id, error) in failed {
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
            Ok(_) => {}
            Err(error) => {
                let _ = self.event_tx.send(EngineEvent::Error(error.to_string()));
            }
        }
    }

    fn begin_oauth2(
        &mut self,
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
        let client = oauth2_client_from_config(&config)?;
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
        let mut authorization = client
            .authorize_url(|| CsrfToken::new(state.clone()))
            .set_pkce_challenge(pkce_challenge);
        for scope in &config.scopes {
            authorization = authorization.add_scope(Scope::new(scope.clone()));
        }
        let (auth_url, csrf_token) = authorization.url();
        self.pending_oauth2.insert(
            account.id.0.clone(),
            PendingOAuth2 {
                config: config.clone(),
                state: csrf_token.secret().clone(),
                pkce_verifier,
            },
        );

        Ok(OAuth2AuthorizationRequest {
            account_id: account.id,
            provider: account.provider,
            auth_url: auth_url.to_string(),
            redirect_uri: redirect_uri.to_string(),
            state: csrf_token.secret().clone(),
            scopes: config.scopes,
        })
    }

    async fn complete_oauth2(
        &mut self,
        callback: OAuth2Callback,
    ) -> std::result::Result<courier_proto::CredentialRef, String> {
        let Some(pending) = self.pending_oauth2.remove(&callback.account_id.0) else {
            return Err(format!(
                "No pending OAuth2 authorization for account {}",
                callback.account_id.0
            ));
        };

        if callback.state != pending.state {
            return Err("OAuth2 callback state did not match the pending request".to_string());
        }

        let client = oauth2_client_from_config(&pending.config)?;
        let http_client = oauth2::reqwest::ClientBuilder::new()
            .redirect(oauth2::reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| format!("OAuth2 HTTP client setup failed: {error}"))?;
        let token = client
            .exchange_code(AuthorizationCode::new(callback.code))
            .set_pkce_verifier(pending.pkce_verifier)
            .request_async(&http_client)
            .await
            .map_err(|error| format!("OAuth2 token exchange failed: {error}"))?;

        let store = OsCredentialStore::new();
        let access_ref = credential_ref(
            callback.account_id.clone(),
            CredentialKind::OAuthAccessToken,
            "dev.hephaestus.courier.oauth2",
        );
        store
            .put_secret(&access_ref, token.access_token().secret())
            .map_err(|error| format!("OAuth2 access token storage failed: {error}"))?;

        let refresh_ref = credential_ref(
            callback.account_id,
            CredentialKind::OAuthRefreshToken,
            "dev.hephaestus.courier.oauth2",
        );
        let refresh_token = token
            .refresh_token()
            .ok_or_else(|| "OAuth2 provider did not return a refresh token".to_string())?;
        store
            .put_secret(&refresh_ref, refresh_token.secret())
            .map_err(|error| format!("OAuth2 refresh token storage failed: {error}"))?;

        Ok(refresh_ref)
    }

    async fn refresh_oauth2_if_needed(
        &self,
        storage: &Storage,
        account_id: &AccountId,
    ) -> std::result::Result<Option<courier_proto::CredentialRef>, String> {
        let Some(account) = storage
            .list_accounts()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|account| account.id == *account_id)
        else {
            return Ok(None);
        };

        if !matches!(account.auth_type, AuthType::OAuth2) {
            return Ok(None);
        }

        let redirect_uri = "http://127.0.0.1:48176/oauth/callback";
        let client_id = std::env::var("COURIER_OAUTH_CLIENT_ID")
            .unwrap_or_else(|_| "configure-client-id".to_string());
        let config = oauth2_client_config(&account.provider, client_id, redirect_uri)
            .ok_or_else(|| "OAuth2 is not configured for this provider".to_string())?;
        if config.token_url.is_empty() {
            return Err("OAuth2 token URL is not configured for this provider".to_string());
        }

        let store = OsCredentialStore::new();
        let refresh_ref = credential_ref(
            account.id.clone(),
            CredentialKind::OAuthRefreshToken,
            "dev.hephaestus.courier.oauth2",
        );
        let Some(refresh_token) = store
            .get_secret(&refresh_ref)
            .map_err(|error| format!("OAuth2 refresh token lookup failed: {error}"))?
        else {
            return Ok(None);
        };

        let client = oauth2_client_from_config(&config)?;
        let http_client = oauth2::reqwest::ClientBuilder::new()
            .redirect(oauth2::reqwest::redirect::Policy::none())
            .build()
            .map_err(|error| format!("OAuth2 HTTP client setup failed: {error}"))?;
        let token = client
            .exchange_refresh_token(&RefreshToken::new(refresh_token))
            .request_async(&http_client)
            .await
            .map_err(|error| format!("OAuth2 token refresh failed: {error}"))?;

        let access_ref = credential_ref(
            account.id.clone(),
            CredentialKind::OAuthAccessToken,
            "dev.hephaestus.courier.oauth2",
        );
        store
            .put_secret(&access_ref, token.access_token().secret())
            .map_err(|error| format!("OAuth2 access token storage failed: {error}"))?;

        if let Some(new_refresh_token) = token.refresh_token() {
            store
                .put_secret(&refresh_ref, new_refresh_token.secret())
                .map_err(|error| format!("OAuth2 refresh token rotation failed: {error}"))?;
        }

        Ok(Some(access_ref))
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
        } else {
            self.publish_notification_policy();
        }
    }

    fn publish_notification_policy(&self) {
        let _ = self.event_tx.send(EngineEvent::NotificationPolicyChanged(
            self.notifications.state(),
        ));
    }

    fn publish_network_paused(&mut self, message: &str) {
        let status = NetworkStatus {
            online: false,
            reason: message.to_string(),
            checked_at: unix_timestamp(),
        };
        let _ = self
            .event_tx
            .send(EngineEvent::NetworkStatusChanged(status.clone()));
        self.publish_notification(DesktopNotification {
            id: format!("network-paused:{}", unix_timestamp()),
            kind: NotificationKind::Warning,
            title: "Network paused".to_string(),
            body: status.reason,
            account_id: None,
            message_ids: Vec::new(),
            created_at: status.checked_at,
        });
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

fn scheduler_for_account(
    storage: &Storage,
    account_id: &AccountId,
) -> std::result::Result<SyncScheduler<ConfiguredRemote>, String> {
    Ok(SyncScheduler::with_remote(
        storage.clone(),
        remote_for_account(storage, account_id)?,
    ))
}

fn scheduler_for_draft(
    storage: &Storage,
    draft_id: &courier_proto::DraftId,
) -> std::result::Result<SyncScheduler<ConfiguredRemote>, String> {
    let draft = storage
        .load_draft(draft_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Draft not found: {}", draft_id.0))?;

    scheduler_for_account(storage, &draft.account_id)
}

fn remote_for_attachment(
    storage: &Storage,
    attachment_id: &AttachmentId,
) -> std::result::Result<ConfiguredRemote, String> {
    let account_id = storage
        .attachment_account_id(attachment_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| {
            format!(
                "Attachment {} is not linked to a stored message account",
                attachment_id.0
            )
        })?;

    remote_for_account(storage, &account_id)
}

fn remote_for_account(
    storage: &Storage,
    account_id: &AccountId,
) -> std::result::Result<ConfiguredRemote, String> {
    let account = storage
        .list_accounts()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|account| account.id == *account_id)
        .ok_or_else(|| format!("Account not found: {}", account_id.0))?;

    if !account.enabled {
        return Err(format!("Account {} is disabled", account.email));
    }

    let store = std::sync::Arc::new(OsCredentialStore::new());
    let resolver_store = store.clone();
    let resolver = CredentialSecretResolver::new(move |reference| {
        resolver_store
            .get_secret(reference)
            .map_err(|error| error.to_string())
    });

    Ok(ConfiguredRemote::from_account_config_with_secret_resolver(
        account_config(account),
        Some(resolver),
    ))
}

fn account_config(account: AccountState) -> AccountConfig {
    AccountConfig {
        id: account.id,
        email: account.email,
        provider: account.provider,
        imap_host: account.imap_host,
        imap_port: account.imap_port,
        smtp_host: account.smtp_host,
        smtp_port: account.smtp_port,
        auth_type: account.auth_type,
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

async fn probe_network_status(accounts: &[AccountState]) -> NetworkStatus {
    let enabled = accounts
        .iter()
        .filter(|account| account.enabled)
        .collect::<Vec<_>>();

    if enabled.is_empty() {
        return NetworkStatus {
            online: true,
            reason: "No enabled accounts; network probe idle".to_string(),
            checked_at: unix_timestamp(),
        };
    }

    let mut reachable = 0usize;
    let mut checked = 0usize;
    let mut failures = Vec::new();

    for account in enabled {
        let imap = test_endpoint(&account.imap_host, account.imap_port).await;
        let smtp = test_endpoint(&account.smtp_host, account.smtp_port).await;
        checked += 2;
        if imap.ok || smtp.ok {
            reachable += 1;
        } else {
            failures.push(format!(
                "{} IMAP {} SMTP {}",
                account.email,
                endpoint_probe_label(&imap),
                endpoint_probe_label(&smtp)
            ));
        }
    }

    let online = reachable > 0;
    let reason = if online {
        format!("Network probe reachable for {reachable} enabled account(s)")
    } else {
        format!(
            "Network probe found no reachable endpoints across {checked} check(s): {}",
            failures.join("; ")
        )
    };

    NetworkStatus {
        online,
        reason,
        checked_at: unix_timestamp(),
    }
}

fn endpoint_probe_label(endpoint: &EndpointCheckResult) -> String {
    if endpoint.ok {
        format!("{}:{} ok", endpoint.host, endpoint.port)
    } else {
        format!(
            "{}:{} {}",
            endpoint.host,
            endpoint.port,
            endpoint.error.as_deref().unwrap_or("failed")
        )
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

fn oauth2_client_from_config(
    config: &OAuth2ClientConfig,
) -> std::result::Result<ReadyOAuth2Client, String> {
    Ok(BasicClient::new(ClientId::new(config.client_id.clone()))
        .set_auth_uri(AuthUrl::new(config.auth_url.clone()).map_err(|error| error.to_string())?)
        .set_token_uri(TokenUrl::new(config.token_url.clone()).map_err(|error| error.to_string())?)
        .set_redirect_uri(
            RedirectUrl::new(config.redirect_uri.clone()).map_err(|error| error.to_string())?,
        ))
}

fn save_credential_secret(
    secret: CredentialSecret,
) -> std::result::Result<courier_proto::CredentialRef, String> {
    if secret.secret.trim().is_empty() {
        return Err("Credential secret is empty".to_string());
    }

    let store = OsCredentialStore::new();
    store
        .put_secret(&secret.reference, &secret.secret)
        .map_err(|error| format!("Credential storage failed: {error}"))?;

    Ok(secret.reference)
}

fn start_oauth2_loopback_listener(
    request: OAuth2AuthorizationRequest,
    command_tx: mpsc::Sender<EngineCommand>,
    event_tx: broadcast::Sender<EngineEvent>,
) {
    tokio::spawn(async move {
        match listen_for_oauth2_callback(request).await {
            Ok(callback) => {
                let account_id = callback.account_id.clone();
                if command_tx
                    .send(EngineCommand::CompleteOAuth2(callback))
                    .await
                    .is_err()
                {
                    let _ = event_tx.send(EngineEvent::OAuth2Completed(Err(format!(
                        "Authorization code received for {}, but the engine command channel is closed",
                        account_id.0
                    ))));
                }
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
