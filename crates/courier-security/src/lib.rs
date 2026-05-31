use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageLoadPolicy {
    BlockRemoteByDefault,
    LoadForMessage,
    TrustSender,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkClickPolicy {
    OpenSystemBrowser,
    ConfirmBeforeOpen,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SanitizedHtml {
    pub html: String,
    pub blocked_remote_images: bool,
}

pub fn sanitize_email_html(input: &str) -> SanitizedHtml {
    let mut html = input.to_string();
    for tag in [
        "script", "iframe", "object", "embed", "form", "input", "button",
    ] {
        html = strip_tag_blocks(&html, tag);
    }

    SanitizedHtml {
        blocked_remote_images: html.contains("http://") || html.contains("https://"),
        html,
    }
}

pub fn redact_email(value: &str) -> String {
    match value.split_once('@') {
        Some((local, domain)) if !local.is_empty() => {
            let first = local.chars().next().unwrap_or('*');
            format!("{first}***@{domain}")
        }
        _ => "[redacted]".to_string(),
    }
}

pub fn redact_token(_: &str) -> &'static str {
    "[redacted-token]"
}

fn strip_tag_blocks(input: &str, tag: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;
    let open = format!("<{tag}");
    let close = format!("</{tag}>");

    while let Some(start) = rest.to_ascii_lowercase().find(&open) {
        output.push_str(&rest[..start]);
        let after_open = &rest[start..];
        if let Some(end) = after_open.to_ascii_lowercase().find(&close) {
            rest = &after_open[end + close.len()..];
        } else {
            return output;
        }
    }

    output.push_str(rest);
    output
}
