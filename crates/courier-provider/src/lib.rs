use courier_proto::ProviderKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthMechanism {
    Password,
    OAuth2AuthorizationCode,
    OAuth2ClientCredentials,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportSecurity {
    Tls,
    StartTls,
    Plain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderEndpoint {
    pub host: String,
    pub port: u16,
    pub security: TransportSecurity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub provider: ProviderKind,
    pub supports_oauth2: bool,
    pub supports_password: bool,
    pub supports_idle: bool,
    pub supports_labels: bool,
    pub supports_jmap: bool,
    pub supports_smtp: bool,
    pub supports_move: bool,
    pub supports_delete: bool,
    pub auth_mechanisms: Vec<AuthMechanism>,
    pub imap_endpoint: Option<ProviderEndpoint>,
    pub smtp_endpoint: Option<ProviderEndpoint>,
    pub jmap_endpoint: Option<ProviderEndpoint>,
}

impl ProviderCapabilities {
    pub fn generic_imap() -> Self {
        Self {
            provider: ProviderKind::GenericImap,
            supports_oauth2: false,
            supports_password: true,
            supports_idle: true,
            supports_labels: false,
            supports_jmap: false,
            supports_smtp: true,
            supports_move: true,
            supports_delete: true,
            auth_mechanisms: vec![AuthMechanism::Password],
            imap_endpoint: None,
            smtp_endpoint: None,
            jmap_endpoint: None,
        }
    }

    pub fn gmail() -> Self {
        Self {
            provider: ProviderKind::Gmail,
            supports_oauth2: true,
            supports_password: false,
            supports_idle: true,
            supports_labels: true,
            supports_jmap: false,
            supports_smtp: true,
            supports_move: true,
            supports_delete: true,
            auth_mechanisms: vec![AuthMechanism::OAuth2AuthorizationCode],
            imap_endpoint: Some(ProviderEndpoint {
                host: "imap.gmail.com".to_string(),
                port: 993,
                security: TransportSecurity::Tls,
            }),
            smtp_endpoint: Some(ProviderEndpoint {
                host: "smtp.gmail.com".to_string(),
                port: 587,
                security: TransportSecurity::StartTls,
            }),
            jmap_endpoint: None,
        }
    }

    pub fn outlook() -> Self {
        Self {
            provider: ProviderKind::Outlook,
            supports_oauth2: true,
            supports_password: false,
            supports_idle: true,
            supports_labels: false,
            supports_jmap: false,
            supports_smtp: true,
            supports_move: true,
            supports_delete: true,
            auth_mechanisms: vec![AuthMechanism::OAuth2AuthorizationCode],
            imap_endpoint: Some(ProviderEndpoint {
                host: "outlook.office365.com".to_string(),
                port: 993,
                security: TransportSecurity::Tls,
            }),
            smtp_endpoint: Some(ProviderEndpoint {
                host: "smtp.office365.com".to_string(),
                port: 587,
                security: TransportSecurity::StartTls,
            }),
            jmap_endpoint: None,
        }
    }

    pub fn jmap() -> Self {
        Self {
            provider: ProviderKind::Jmap,
            supports_oauth2: true,
            supports_password: true,
            supports_idle: false,
            supports_labels: true,
            supports_jmap: true,
            supports_smtp: false,
            supports_move: true,
            supports_delete: true,
            auth_mechanisms: vec![
                AuthMechanism::Password,
                AuthMechanism::OAuth2AuthorizationCode,
            ],
            imap_endpoint: None,
            smtp_endpoint: None,
            jmap_endpoint: None,
        }
    }
}

pub fn capabilities_for(provider: &ProviderKind) -> ProviderCapabilities {
    match provider {
        ProviderKind::GenericImap => ProviderCapabilities::generic_imap(),
        ProviderKind::Gmail => ProviderCapabilities::gmail(),
        ProviderKind::Outlook => ProviderCapabilities::outlook(),
        ProviderKind::Jmap => ProviderCapabilities::jmap(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_defaults_capture_auth_and_transport_shape() {
        let gmail = capabilities_for(&ProviderKind::Gmail);
        assert!(gmail.supports_oauth2);
        assert!(!gmail.supports_password);
        assert!(gmail.imap_endpoint.is_some());
        assert!(gmail.smtp_endpoint.is_some());

        let generic = capabilities_for(&ProviderKind::GenericImap);
        assert!(generic.supports_password);
        assert!(generic.imap_endpoint.is_none());
    }
}
