use courier_proto::{OAuth2ClientConfig, ProviderKind};
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

pub fn oauth2_client_config(
    provider: &ProviderKind,
    client_id: impl Into<String>,
    redirect_uri: impl Into<String>,
) -> Option<OAuth2ClientConfig> {
    let client_id = client_id.into();
    let redirect_uri = redirect_uri.into();
    match provider {
        ProviderKind::Gmail => Some(OAuth2ClientConfig {
            provider: ProviderKind::Gmail,
            client_id,
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
            token_url: "https://oauth2.googleapis.com/token".to_string(),
            scopes: vec![
                "https://mail.google.com/".to_string(),
                "openid".to_string(),
                "email".to_string(),
            ],
            redirect_uri,
        }),
        ProviderKind::Outlook => Some(OAuth2ClientConfig {
            provider: ProviderKind::Outlook,
            client_id,
            auth_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize".to_string(),
            token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token".to_string(),
            scopes: vec![
                "offline_access".to_string(),
                "https://outlook.office.com/IMAP.AccessAsUser.All".to_string(),
                "https://outlook.office.com/SMTP.Send".to_string(),
            ],
            redirect_uri,
        }),
        ProviderKind::Jmap => Some(OAuth2ClientConfig {
            provider: ProviderKind::Jmap,
            client_id,
            auth_url: String::new(),
            token_url: String::new(),
            scopes: vec!["mail".to_string()],
            redirect_uri,
        }),
        ProviderKind::GenericImap => None,
    }
}

pub fn authorization_url(config: &OAuth2ClientConfig, state: &str) -> String {
    let scopes = config.scopes.join(" ");
    format!(
        "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}",
        config.auth_url,
        encode_query_component(&config.client_id),
        encode_query_component(&config.redirect_uri),
        encode_query_component(&scopes),
        encode_query_component(state),
    )
}

fn encode_query_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            b' ' => encoded.push_str("%20"),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
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

    #[test]
    fn oauth2_authorization_url_encodes_redirect_and_scope() {
        let config = oauth2_client_config(
            &ProviderKind::Gmail,
            "client id",
            "http://127.0.0.1:48176/callback",
        )
        .expect("gmail oauth config");
        let url = authorization_url(&config, "state value");

        assert!(url.contains("client_id=client%20id"));
        assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A48176%2Fcallback"));
        assert!(url.contains("state=state%20value"));
    }
}
