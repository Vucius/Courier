use courier_proto::AttachmentId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BodyKind {
    PlainText,
    Html,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedBody {
    pub kind: BodyKind,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedAttachment {
    pub id: AttachmentId,
    pub filename: String,
    pub mime_type: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedMessage {
    pub body: ParsedBody,
    pub attachments: Vec<ParsedAttachment>,
}

pub fn choose_display_body(plain: Option<String>, html: Option<String>) -> ParsedBody {
    if let Some(html) = html {
        ParsedBody {
            kind: BodyKind::Html,
            content: html,
        }
    } else {
        ParsedBody {
            kind: BodyKind::PlainText,
            content: plain.unwrap_or_default(),
        }
    }
}
