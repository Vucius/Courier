use mailproto::AccountId;

pub struct SyncScheduler;

impl SyncScheduler {
    pub async fn sync_now(&self, _account_id: AccountId) -> crate::Result<()> {
        Ok(())
    }
}
