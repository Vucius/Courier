use courier_proto::AccountId;
use courier_storage::Storage;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("sync worker is not running for account {0}")]
    WorkerNotRunning(String),
}

#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub base_delay_ms: u64,
}

#[derive(Debug, Clone)]
pub struct SyncScheduler {
    storage: Storage,
    retry_policy: RetryPolicy,
}

impl SyncScheduler {
    pub fn new(storage: Storage) -> Self {
        Self {
            storage,
            retry_policy: RetryPolicy {
                max_retries: 5,
                base_delay_ms: 500,
            },
        }
    }

    pub async fn sync_now(&self, account_id: AccountId) -> Result<()> {
        tracing::info!(?account_id, db = %self.storage.db_path().display(), "sync requested");
        Ok(())
    }

    pub fn retry_policy(&self) -> &RetryPolicy {
        &self.retry_policy
    }
}

#[derive(Debug, Clone)]
pub struct SyncWorker {
    pub account_id: AccountId,
}

#[derive(Debug, Clone)]
pub struct SendQueue;

#[derive(Debug, Clone)]
pub struct OpQueue;
