use courier_proto::{AccountId, CredentialKind, CredentialRef, CredentialStoreStatus};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("credential backend is unavailable: {0}")]
    BackendUnavailable(String),
    #[error("credential entry error: {0}")]
    Entry(String),
    #[error("stored credential is not valid UTF-8")]
    InvalidUtf8,
}

pub trait CredentialStore {
    fn status(&self) -> CredentialStoreStatus;
    fn put_secret(&self, reference: &CredentialRef, secret: &str) -> Result<()>;
    fn get_secret(&self, reference: &CredentialRef) -> Result<Option<String>>;
    fn delete_secret(&self, reference: &CredentialRef) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct OsCredentialStore {
    backend: String,
}

impl OsCredentialStore {
    pub fn new() -> Self {
        Self {
            backend: native_backend_name().to_string(),
        }
    }

    fn entry(&self, reference: &CredentialRef) -> Result<keyring::Entry> {
        keyring::Entry::new(&reference.service, &reference.key)
            .map_err(|error| Error::Entry(error.to_string()))
    }
}

impl Default for OsCredentialStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialStore for OsCredentialStore {
    fn status(&self) -> CredentialStoreStatus {
        let probe = CredentialRef {
            account_id: AccountId("status".to_string()),
            kind: CredentialKind::Password,
            service: "dev.hephaestus.courier.status".to_string(),
            key: "status".to_string(),
        };

        match self.entry(&probe) {
            Ok(_) => CredentialStoreStatus {
                available: true,
                backend: self.backend.clone(),
                message: "OS keyring backend is linked; secrets are stored outside SQLite"
                    .to_string(),
            },
            Err(error) => CredentialStoreStatus {
                available: false,
                backend: self.backend.clone(),
                message: error.to_string(),
            },
        }
    }

    fn put_secret(&self, reference: &CredentialRef, secret: &str) -> Result<()> {
        self.entry(reference)?
            .set_secret(secret.as_bytes())
            .map_err(|error| Error::Entry(error.to_string()))
    }

    fn get_secret(&self, reference: &CredentialRef) -> Result<Option<String>> {
        match self.entry(reference)?.get_secret() {
            Ok(secret) => String::from_utf8(secret)
                .map(Some)
                .map_err(|_| Error::InvalidUtf8),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(Error::Entry(error.to_string())),
        }
    }

    fn delete_secret(&self, reference: &CredentialRef) -> Result<()> {
        match self.entry(reference)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(Error::Entry(error.to_string())),
        }
    }
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

pub fn account_credential_refs(account_id: AccountId) -> [CredentialRef; 3] {
    [
        credential_ref(
            account_id.clone(),
            CredentialKind::Password,
            "dev.hephaestus.courier.password",
        ),
        credential_ref(
            account_id.clone(),
            CredentialKind::OAuthAccessToken,
            "dev.hephaestus.courier.oauth2",
        ),
        credential_ref(
            account_id,
            CredentialKind::OAuthRefreshToken,
            "dev.hephaestus.courier.oauth2",
        ),
    ]
}

fn native_backend_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "windows-credential-manager"
    }
    #[cfg(target_os = "macos")]
    {
        "macos-keychain"
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        "linux-secret-service"
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", unix)))]
    {
        "os-keyring"
    }
}
