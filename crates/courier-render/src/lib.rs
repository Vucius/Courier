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
    Link {
        href: String,
        children: Vec<RenderNode>,
    },
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
    let mut nodes = html_nodes(&sanitized.html);
    if nodes.is_empty() {
        nodes.push(RenderNode::Paragraph(vec![RenderNode::Text(
            text_from_html(&sanitized.html),
        )]));
    }

    RenderTree {
        nodes,
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

fn html_nodes(input: &str) -> Vec<RenderNode> {
    let mut nodes = Vec::new();
    let mut inline = Vec::new();
    let mut rest = input;

    while let Some(start) = rest.find('<') {
        push_text(&mut inline, &rest[..start]);
        let after_start = &rest[start..];
        let Some(end) = after_start.find('>') else {
            push_text(&mut inline, after_start);
            rest = "";
            break;
        };

        let tag = &after_start[..=end];
        let lower_tag = tag.to_ascii_lowercase();
        let after_tag = &after_start[end + 1..];

        if lower_tag.starts_with("</p")
            || lower_tag.starts_with("</div")
            || lower_tag.starts_with("<br")
            || lower_tag.starts_with("</tr")
        {
            flush_paragraph(&mut nodes, &mut inline);
            rest = after_tag;
            continue;
        }

        if lower_tag.starts_with("<img") {
            if let Some(source) = image_source_from_tag(tag) {
                inline.push(RenderNode::Image(source));
            }
            rest = after_tag;
            continue;
        }

        if lower_tag.starts_with("<a ") || lower_tag.starts_with("<a>") {
            let href = attr_value(tag, "href").unwrap_or_default();
            if let Some(close) = after_tag.to_ascii_lowercase().find("</a>") {
                let label = text_from_html(&after_tag[..close]);
                if href.is_empty() {
                    push_text(&mut inline, &label);
                } else {
                    inline.push(RenderNode::Link {
                        href,
                        children: vec![RenderNode::Text(label)],
                    });
                }
                rest = &after_tag[close + "</a>".len()..];
                continue;
            }
        }

        if lower_tag.starts_with("<blockquote") {
            flush_paragraph(&mut nodes, &mut inline);
            if let Some(close) = after_tag.to_ascii_lowercase().find("</blockquote>") {
                nodes.push(RenderNode::BlockQuote(html_nodes(&after_tag[..close])));
                rest = &after_tag[close + "</blockquote>".len()..];
                continue;
            }
        }

        rest = after_tag;
    }

    push_text(&mut inline, rest);
    flush_paragraph(&mut nodes, &mut inline);

    nodes
}

fn push_text(nodes: &mut Vec<RenderNode>, value: &str) {
    let text = decode_entities(&text_from_html(value));
    let text = text.trim();
    if !text.is_empty() {
        nodes.push(RenderNode::Text(text.to_string()));
    }
}

fn flush_paragraph(nodes: &mut Vec<RenderNode>, inline: &mut Vec<RenderNode>) {
    if inline.is_empty() {
        return;
    }

    nodes.push(RenderNode::Paragraph(std::mem::take(inline)));
}

fn image_source_from_tag(tag: &str) -> Option<ImageSource> {
    if tag.contains("data-courier-remote-image=\"blocked\"") {
        return Some(ImageSource::RemoteUrl("blocked".to_string()));
    }

    let src = attr_value(tag, "src")?;
    if src.starts_with("cid:") {
        Some(ImageSource::Cid(src.trim_start_matches("cid:").to_string()))
    } else if src.starts_with("http://") || src.starts_with("https://") {
        Some(ImageSource::RemoteUrl(src))
    } else if src.starts_with("attachment:") {
        Some(ImageSource::Attachment(
            src.trim_start_matches("attachment:").to_string(),
        ))
    } else {
        Some(ImageSource::LocalPath(src))
    }
}

fn attr_value(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let needle = format!("{name}=");
    let start = lower.find(&needle)? + needle.len();
    let quote = tag[start..].chars().next()?;

    match quote {
        '"' | '\'' => {
            let value_start = start + quote.len_utf8();
            let value_end = tag[value_start..].find(quote)? + value_start;
            Some(tag[value_start..value_end].to_string())
        }
        _ => {
            let value_end = tag[start..]
                .find(|ch: char| ch.is_whitespace() || ch == '>')
                .map(|end| start + end)
                .unwrap_or_else(|| tag.len().saturating_sub(1));
            Some(tag[start..value_end].to_string())
        }
    }
}

fn text_from_html(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut in_tag = false;

    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(ch),
            _ => {}
        }
    }

    output
}

fn decode_entities(input: &str) -> String {
    input
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_render_tree_sanitizes_and_preserves_links() {
        let tree = render_tree_from_html(
            r#"<p>Hello <a href="https://example.test">there</a></p><script>x()</script><img src="https://example.test/pixel.png">"#,
        );

        assert!(tree.blocked_remote_images);
        assert_eq!(tree.nodes.len(), 2);
        assert!(matches!(&tree.nodes[0], RenderNode::Paragraph(_)));
        assert!(
            matches!(&tree.nodes[1], RenderNode::Paragraph(nodes) if matches!(nodes.first(), Some(RenderNode::Image(ImageSource::RemoteUrl(value))) if value == "blocked"))
        );
        assert!(matches!(
            &tree.nodes[0],
            RenderNode::Paragraph(nodes) if nodes.iter().any(|node| matches!(node, RenderNode::Link { href, .. } if href == "https://example.test"))
        ));
    }

    #[test]
    fn text_render_tree_maps_lines_to_paragraphs() {
        let tree = render_tree_from_text("one\ntwo");

        assert_eq!(tree.nodes.len(), 2);
        assert!(!tree.blocked_remote_images);
    }
}
