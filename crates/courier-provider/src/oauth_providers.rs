use courier_proto::ProviderKind;

pub struct OAuthProviderConfig {
    pub client_id: &'static str,
    // client_secret: Option<&'static str>, // Retained for future use if needed
}

fn get_default_config(provider: &ProviderKind) -> Option<OAuthProviderConfig> {
    match provider {
        ProviderKind::Outlook => Some(OAuthProviderConfig {
            client_id: "81f526cb-b874-4663-a95a-41d5f48bb950",
        }),
        ProviderKind::Gmail => Some(OAuthProviderConfig {
            client_id: "configure-client-id",
        }),
        _ => None,
    }
}

pub fn get_client_id(provider: &ProviderKind) -> String {
    let env_var_name = match provider {
        ProviderKind::Outlook => "COURIER_OUTLOOK_CLIENT_ID",
        ProviderKind::Gmail => "COURIER_GMAIL_CLIENT_ID",
        _ => "COURIER_OAUTH_CLIENT_ID",
    };

    std::env::var(env_var_name)
        .ok()
        .or_else(|| get_default_config(provider).map(|cfg| cfg.client_id.to_string()))
        .unwrap_or_else(|| "configure-client-id".to_string())
}
