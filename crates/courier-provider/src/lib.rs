use courier_proto::ProviderKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub provider: ProviderKind,
    pub supports_oauth2: bool,
    pub supports_idle: bool,
    pub supports_labels: bool,
    pub supports_jmap: bool,
}

impl ProviderCapabilities {
    pub fn generic_imap() -> Self {
        Self {
            provider: ProviderKind::GenericImap,
            supports_oauth2: false,
            supports_idle: true,
            supports_labels: false,
            supports_jmap: false,
        }
    }
}
