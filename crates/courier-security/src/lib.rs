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
pub enum AttachmentRisk {
    Low,
    Caution,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentDecision {
    pub risk: AttachmentRisk,
    pub can_preview: bool,
    pub can_open: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SanitizedHtml {
    pub html: String,
    pub blocked_remote_images: bool,
}

pub fn sanitize_email_html(input: &str) -> SanitizedHtml {
    let mut html = strip_comments(input);
    for tag in [
        "script", "style", "iframe", "object", "embed", "form", "input", "button",
    ] {
        html = strip_tag_blocks(&html, tag);
    }

    let blocked_remote_images = contains_remote_image(&html);
    html = strip_unsafe_attributes(&html);
    html = strip_remote_image_sources(&html);

    SanitizedHtml {
        blocked_remote_images,
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

pub fn classify_attachment(filename: &str, mime_type: &str, size: u64) -> AttachmentDecision {
    let extension = filename
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    let mime_type = mime_type.to_ascii_lowercase();

    if is_blocked_extension(&extension) {
        return AttachmentDecision {
            risk: AttachmentRisk::Blocked,
            can_preview: false,
            can_open: false,
            reason: "Executable attachment blocked".to_string(),
        };
    }

    if is_caution_extension(&extension) || is_caution_mime(&mime_type) {
        return AttachmentDecision {
            risk: AttachmentRisk::Caution,
            can_preview: false,
            can_open: true,
            reason: "Review before opening".to_string(),
        };
    }

    let can_preview = is_previewable_mime(&mime_type) && size <= 10 * 1024 * 1024;
    AttachmentDecision {
        risk: AttachmentRisk::Low,
        can_preview,
        can_open: true,
        reason: if can_preview {
            "Preview available".to_string()
        } else {
            "Open with system app".to_string()
        },
    }
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

fn strip_comments(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;

    while let Some(start) = rest.find("<!--") {
        output.push_str(&rest[..start]);
        let after_start = &rest[start + 4..];
        if let Some(end) = after_start.find("-->") {
            rest = &after_start[end + 3..];
        } else {
            return output;
        }
    }

    output.push_str(rest);
    output
}

fn strip_unsafe_attributes(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;

    while let Some(start) = rest.find('<') {
        output.push_str(&rest[..start]);
        let after_start = &rest[start..];
        if let Some(end) = after_start.find('>') {
            output.push_str(&sanitize_tag(&after_start[..=end]));
            rest = &after_start[end + 1..];
        } else {
            output.push_str(after_start);
            return output;
        }
    }

    output.push_str(rest);
    output
}

fn sanitize_tag(tag: &str) -> String {
    if tag.starts_with("</") || tag.starts_with("<!") {
        return tag.to_string();
    }

    let tag = tag.trim_end_matches('>');
    let self_closing = tag.ends_with('/');
    let inner = tag.trim_start_matches('<').trim_end_matches('/').trim();
    let Some(name_end) = inner.find(char::is_whitespace) else {
        return if self_closing {
            format!("<{} />", inner)
        } else {
            format!("<{}>", inner)
        };
    };
    let name = &inner[..name_end];
    if name.is_empty() {
        return "<>".to_string();
    }

    let mut safe = format!("<{}", name);
    for attr in parse_attrs(&inner[name_end..]) {
        let attr_name = attr.name.to_ascii_lowercase();

        if !is_allowed_attr(&attr_name) {
            continue;
        }

        if attr_name == "href"
            && attr
                .value
                .as_deref()
                .unwrap_or_default()
                .trim_start()
                .to_ascii_lowercase()
                .starts_with("javascript:")
        {
            continue;
        }

        safe.push(' ');
        safe.push_str(&attr.name);
        if let Some(value) = attr.value {
            safe.push_str("=\"");
            safe.push_str(&escape_attr_value(&value));
            safe.push('"');
        }
    }

    if self_closing {
        safe.push_str(" />");
    } else {
        safe.push('>');
    }
    safe
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HtmlAttr {
    name: String,
    value: Option<String>,
}

fn parse_attrs(input: &str) -> Vec<HtmlAttr> {
    let mut attrs = Vec::new();
    let mut rest = input.trim();

    while !rest.is_empty() {
        let name_end = rest
            .find(|ch: char| ch.is_whitespace() || ch == '=' || ch == '/' || ch == '>')
            .unwrap_or(rest.len());
        let name = rest[..name_end].trim();
        if name.is_empty() {
            break;
        }

        rest = rest[name_end..].trim_start();
        let mut value = None;

        if let Some(after_equals) = rest.strip_prefix('=') {
            rest = after_equals.trim_start();
            match rest.chars().next() {
                Some('"') | Some('\'') => {
                    let quote = rest.chars().next().unwrap();
                    let value_start = quote.len_utf8();
                    if let Some(value_end) = rest[value_start..].find(quote) {
                        value = Some(rest[value_start..value_start + value_end].to_string());
                        rest = rest[value_start + value_end + quote.len_utf8()..].trim_start();
                    } else {
                        value = Some(rest[value_start..].to_string());
                        rest = "";
                    }
                }
                Some(_) => {
                    let value_end = rest
                        .find(|ch: char| ch.is_whitespace() || ch == '/' || ch == '>')
                        .unwrap_or(rest.len());
                    value = Some(rest[..value_end].to_string());
                    rest = rest[value_end..].trim_start();
                }
                None => rest = "",
            }
        }

        attrs.push(HtmlAttr {
            name: name.to_string(),
            value,
        });
    }

    attrs
}

fn is_allowed_attr(name: &str) -> bool {
    matches!(
        name,
        "href" | "src" | "alt" | "title" | "width" | "height" | "data-courier-remote-image"
    )
}

fn escape_attr_value(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn contains_remote_image(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    let mut rest = lower.as_str();

    while let Some(start) = rest.find("<img") {
        let after_start = &rest[start..];
        if let Some(end) = after_start.find('>') {
            let tag = &after_start[..=end];
            if tag.contains("src=\"http://")
                || tag.contains("src='http://")
                || tag.contains("src=http://")
                || tag.contains("src=\"https://")
                || tag.contains("src='https://")
                || tag.contains("src=https://")
            {
                return true;
            }
            rest = &after_start[end + 1..];
        } else {
            return false;
        }
    }

    false
}

fn strip_remote_image_sources(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;

    while let Some(start) = rest.to_ascii_lowercase().find("<img") {
        output.push_str(&rest[..start]);
        let after_start = &rest[start..];
        if let Some(end) = after_start.find('>') {
            let tag = &after_start[..=end];
            output.push_str(&strip_remote_src_from_tag(tag));
            rest = &after_start[end + 1..];
        } else {
            output.push_str(after_start);
            return output;
        }
    }

    output.push_str(rest);
    output
}

fn strip_remote_src_from_tag(tag: &str) -> String {
    let lower = tag.to_ascii_lowercase();
    let Some(src_start) = lower.find("src=") else {
        return tag.to_string();
    };
    let value_start = src_start + 4;
    let quote = tag[value_start..].chars().next();

    let (value_end, value) = match quote {
        Some('"') | Some('\'') => {
            let quote = quote.unwrap();
            let after_quote = value_start + quote.len_utf8();
            if let Some(end) = tag[after_quote..].find(quote) {
                (
                    after_quote + end + quote.len_utf8(),
                    &tag[after_quote..after_quote + end],
                )
            } else {
                return tag.to_string();
            }
        }
        Some(_) => {
            let end = tag[value_start..]
                .find(|ch: char| ch.is_whitespace() || ch == '>')
                .map(|end| value_start + end)
                .unwrap_or(tag.len() - 1);
            (end, &tag[value_start..end])
        }
        None => return tag.to_string(),
    };

    if !(value.to_ascii_lowercase().starts_with("http://")
        || value.to_ascii_lowercase().starts_with("https://"))
    {
        return tag.to_string();
    }

    let mut clean = String::with_capacity(tag.len());
    clean.push_str(&tag[..src_start]);
    clean.push_str("data-courier-remote-image=\"blocked\"");
    clean.push_str(&tag[value_end..]);
    clean
}

fn is_blocked_extension(extension: &str) -> bool {
    matches!(
        extension,
        "exe"
            | "msi"
            | "bat"
            | "cmd"
            | "com"
            | "scr"
            | "ps1"
            | "vbs"
            | "js"
            | "jar"
            | "sh"
            | "app"
            | "dmg"
    )
}

fn is_caution_extension(extension: &str) -> bool {
    matches!(
        extension,
        "zip" | "rar" | "7z" | "tar" | "gz" | "docm" | "xlsm" | "pptm"
    )
}

fn is_caution_mime(mime_type: &str) -> bool {
    mime_type.contains("application/zip")
        || mime_type.contains("application/x-7z")
        || mime_type.contains("application/x-rar")
        || mime_type.contains("application/vnd.ms-excel.sheet.macroenabled")
        || mime_type.contains("application/vnd.ms-word.document.macroenabled")
}

fn is_previewable_mime(mime_type: &str) -> bool {
    mime_type.starts_with("text/")
        || mime_type == "application/pdf"
        || mime_type == "image/png"
        || mime_type == "image/jpeg"
        || mime_type == "image/gif"
        || mime_type == "image/webp"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizer_removes_active_content_and_blocks_remote_images() {
        let sanitized = sanitize_email_html(
            r#"<p onclick="x()">Hi</p><script>alert(1)</script><img src="https://example.test/a.png">"#,
        );

        assert!(!sanitized.html.contains("script"));
        assert!(!sanitized.html.contains("onclick"));
        assert!(!sanitized.html.contains("https://example.test/a.png"));
        assert!(sanitized.blocked_remote_images);
    }

    #[test]
    fn sanitizer_keeps_safe_links_and_removes_javascript_links() {
        let sanitized = sanitize_email_html(
            r#"<a href="https://example.test">ok</a><a href="javascript:alert(1)">bad</a>"#,
        );

        assert!(sanitized.html.contains("https://example.test"));
        assert!(!sanitized.html.contains("javascript:"));
    }

    #[test]
    fn sanitizer_handles_quoted_attributes_with_spaces() {
        let sanitized = sanitize_email_html(
            r#"<p style="color: red" onclick="alert with spaces" title="safe title">Hi</p>"#,
        );

        assert!(sanitized.html.contains("title=\"safe title\""));
        assert!(!sanitized.html.contains("style"));
        assert!(!sanitized.html.contains("onclick"));
        assert!(!sanitized.html.contains("alert with spaces"));
    }

    #[test]
    fn attachment_policy_blocks_executables() {
        let decision = classify_attachment("invoice.exe", "application/octet-stream", 42);

        assert_eq!(decision.risk, AttachmentRisk::Blocked);
        assert!(!decision.can_open);
        assert!(!decision.can_preview);
    }

    #[test]
    fn attachment_policy_allows_safe_previewable_files() {
        let decision = classify_attachment("note.txt", "text/plain", 42);

        assert_eq!(decision.risk, AttachmentRisk::Low);
        assert!(decision.can_open);
        assert!(decision.can_preview);
    }

    #[test]
    fn attachment_policy_marks_archives_for_review() {
        let decision = classify_attachment("payload.zip", "application/zip", 42);

        assert_eq!(decision.risk, AttachmentRisk::Caution);
        assert!(decision.can_open);
        assert!(!decision.can_preview);
    }
}
