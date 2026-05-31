use mailproto::{AccountId, AuthType};

#[derive(Debug, Clone)]
pub struct AccountConfig {
    pub id: AccountId,
    pub email: String,
    pub provider: String,
    pub imap_host: String,
    pub smtp_host: String,
    pub auth_type: AuthType,
}
