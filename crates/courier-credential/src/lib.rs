use courier_proto::{AccountId, CredentialKind, CredentialRef, CredentialStoreStatus};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("credential backend is unavailable: {0}")]
    BackendUnavailable(String),
}

pub trait CredentialStore {
    fn status(&self) -> CredentialStoreStatus;
    fn put_secret(&self, reference: &CredentialRef, secret: &str) -> Result<()>;
    fn get_secret(&self, reference: &CredentialRef) -> Result<Option<String>>;
    fn delete_secret(&self, reference: &CredentialRef) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct UnsupportedCredentialStore;

impl CredentialStore for UnsupportedCredentialStore {
    fn status(&self) -> CredentialStoreStatus {
        CredentialStoreStatus {
            available: false,
            backend: "unsupported".to_string(),
            message: "OS keyring backend is not linked yet; secrets are not stored in SQLite"
                .to_string(),
        }
    }

    fn put_secret(&self, _reference: &CredentialRef, _secret: &str) -> Result<()> {
        Err(Error::BackendUnavailable(
            "keyring backend is not linked".to_string(),
        ))
    }

    fn get_secret(&self, _reference: &CredentialRef) -> Result<Option<String>> {
        Err(Error::BackendUnavailable(
            "keyring backend is not linked".to_string(),
        ))
    }

    fn delete_secret(&self, _reference: &CredentialRef) -> Result<()> {
        Err(Error::BackendUnavailable(
            "keyring backend is not linked".to_string(),
        ))
    }
}

pub fn credential_ref(
    account_id: AccountId,
    kind: CredentialKind,
    service: impl Into<String>,
) -> CredentialRef {
    let service = service.into();
    let key_kind = match kind {
        CredentialKind::Password => "password",
        CredentialKind::OAuthAccessToken => "oauth-access-token",
        CredentialKind::OAuthRefreshToken => "oauth-refresh-token",
    };

    CredentialRef {
        key: format!("{}:{key_kind}", account_id.0),
        account_id,
        kind,
        service,
    }
}
