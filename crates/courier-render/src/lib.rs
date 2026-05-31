use courier_security::sanitize_email_html;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageSource {
    Cid(String),
    Attachment(String),
    RemoteUrl(String),
    LocalPath(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderNode {
    Text(String),
    Paragraph(Vec<RenderNode>),
    Link { href: String, children: Vec<RenderNode> },
    Image(ImageSource),
    BlockQuote(Vec<RenderNode>),
    Table(Vec<RenderNode>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderTree {
    pub nodes: Vec<RenderNode>,
    pub blocked_remote_images: bool,
}

pub fn render_tree_from_html(input: &str) -> RenderTree {
    let sanitized = sanitize_email_html(input);
    RenderTree {
        nodes: vec![RenderNode::Paragraph(vec![RenderNode::Text(sanitized.html)])],
        blocked_remote_images: sanitized.blocked_remote_images,
    }
}

pub fn render_tree_from_text(input: &str) -> RenderTree {
    RenderTree {
        nodes: input
            .lines()
            .map(|line| RenderNode::Paragraph(vec![RenderNode::Text(line.to_string())]))
            .collect(),
        blocked_remote_images: false,
    }
}
