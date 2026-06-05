use courier_security::sanitize_email_html;
use ego_tree::NodeRef;
use scraper::{ElementRef, Html, node::Node};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageSource {
    Cid(String),
    Attachment(String),
    RemoteUrl(String),
    LocalPath(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableCell {
    pub header: bool,
    pub colspan: u16,
    pub rowspan: u16,
    pub nodes: Vec<RenderNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableRow {
    pub cells: Vec<TableCell>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderNode {
    Text(String),
    Code(String),
    Preformatted(String),
    Paragraph(Vec<RenderNode>),
    Heading {
        level: u8,
        children: Vec<RenderNode>,
    },
    Link {
        href: String,
        children: Vec<RenderNode>,
    },
    Image(ImageSource),
    BlockQuote {
        depth: u8,
        collapsed: bool,
        children: Vec<RenderNode>,
    },
    List {
        ordered: bool,
        items: Vec<Vec<RenderNode>>,
    },
    Strong(Vec<RenderNode>),
    Emphasis(Vec<RenderNode>),
    HorizontalRule,
    LineBreak,
    Table {
        rows: Vec<TableRow>,
    },
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
            text_fallback(&sanitized.html),
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
    let fragment = Html::parse_fragment(input);
    block_children(fragment.root_element(), 0)
}

fn block_children(element: ElementRef<'_>, quote_depth: u8) -> Vec<RenderNode> {
    wrap_inline_runs(raw_children(element, quote_depth))
}

fn raw_children(element: ElementRef<'_>, quote_depth: u8) -> Vec<RenderNode> {
    element
        .children()
        .flat_map(|node| node_to_nodes(node, quote_depth))
        .collect()
}

fn inline_children(element: ElementRef<'_>, quote_depth: u8) -> Vec<RenderNode> {
    raw_children(element, quote_depth)
}

fn node_to_nodes(node: NodeRef<'_, Node>, quote_depth: u8) -> Vec<RenderNode> {
    match node.value() {
        Node::Text(text) => normalized_text(text)
            .map(RenderNode::Text)
            .into_iter()
            .collect(),
        Node::Element(_) => {
            let Some(element) = ElementRef::wrap(node) else {
                return Vec::new();
            };
            element_to_nodes(element, quote_depth)
        }
        _ => Vec::new(),
    }
}

fn element_to_nodes(element: ElementRef<'_>, quote_depth: u8) -> Vec<RenderNode> {
    let name = element.value().name();
    if is_hidden_element(element) {
        return Vec::new();
    }

    if is_quote_container(element) {
        let depth = quote_depth.saturating_add(1);
        return vec![RenderNode::BlockQuote {
            depth,
            collapsed: depth >= 2,
            children: block_children(element, depth),
        }];
    }

    match name {
        "html" | "body" | "main" | "article" | "section" | "div" | "center" => {
            block_children(element, quote_depth)
        }
        "p" => vec![RenderNode::Paragraph(inline_children(element, quote_depth))],
        "br" => vec![RenderNode::LineBreak],
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => vec![RenderNode::Heading {
            level: heading_level(name),
            children: inline_children(element, quote_depth),
        }],
        "a" => link_nodes(element, quote_depth),
        "img" => image_source_from_element(element)
            .map(RenderNode::Image)
            .into_iter()
            .collect(),
        "blockquote" => {
            let depth = quote_depth.saturating_add(1);
            vec![RenderNode::BlockQuote {
                depth,
                collapsed: depth >= 3,
                children: block_children(element, depth),
            }]
        }
        "ul" | "ol" => vec![RenderNode::List {
            ordered: name == "ol",
            items: list_items(element, quote_depth),
        }],
        "li" => block_children(element, quote_depth),
        "strong" | "b" => vec![RenderNode::Strong(inline_children(element, quote_depth))],
        "em" | "i" => vec![RenderNode::Emphasis(inline_children(element, quote_depth))],
        "hr" => vec![RenderNode::HorizontalRule],
        "table" => vec![RenderNode::Table {
            rows: table_rows(element, quote_depth),
        }],
        "thead" | "tbody" | "tfoot" => block_children(element, quote_depth),
        "tr" => {
            let row = table_row(element, quote_depth);
            if row.cells.is_empty() {
                Vec::new()
            } else {
                vec![RenderNode::Table { rows: vec![row] }]
            }
        }
        "td" | "th" => block_children(element, quote_depth),
        "pre" => preformatted_node(element).into_iter().collect(),
        "code" => code_node(element).into_iter().collect(),
        "span" | "small" | "label" | "font" => inline_children(element, quote_depth),
        _ => block_children(element, quote_depth),
    }
}

fn is_hidden_element(element: ElementRef<'_>) -> bool {
    if element.value().attr("hidden").is_some() {
        return true;
    }
    if element
        .value()
        .attr("aria-hidden")
        .is_some_and(|value| value.eq_ignore_ascii_case("true"))
    {
        return true;
    }
    let class = element.value().attr("class").unwrap_or_default();
    class
        .split_whitespace()
        .any(|name| matches!(name, "hidden" | "sr-only" | "visually-hidden" | "MsoHidden"))
}

fn is_quote_container(element: ElementRef<'_>) -> bool {
    let name = element.value().name();
    if !matches!(name, "div" | "section" | "blockquote") {
        return false;
    }

    let class = element.value().attr("class").unwrap_or_default();
    let id = element.value().attr("id").unwrap_or_default();
    let marker = format!("{class} {id}").to_ascii_lowercase();
    [
        "gmail_quote",
        "gmail_attr",
        "yahoo_quoted",
        "moz-cite-prefix",
        "protonmail_quote",
        "outlookmessageheader",
        "ms-outlook-mobile-signature",
        "x_gmail_quote",
    ]
    .iter()
    .any(|needle| marker.contains(needle))
}

fn preformatted_node(element: ElementRef<'_>) -> Option<RenderNode> {
    let text = preserved_text(element).trim_matches('\n').to_string();
    if text.is_empty() {
        None
    } else {
        Some(RenderNode::Preformatted(text))
    }
}

fn code_node(element: ElementRef<'_>) -> Option<RenderNode> {
    let text = preserved_text(element);
    if text.is_empty() {
        None
    } else {
        Some(RenderNode::Code(text))
    }
}

fn link_nodes(element: ElementRef<'_>, quote_depth: u8) -> Vec<RenderNode> {
    let children = inline_children(element, quote_depth);
    let href = element
        .value()
        .attr("href")
        .map(str::trim)
        .filter(|value| !value.is_empty());

    match href {
        Some(href) => vec![RenderNode::Link {
            href: href.to_string(),
            children,
        }],
        None => children,
    }
}

fn list_items(element: ElementRef<'_>, quote_depth: u8) -> Vec<Vec<RenderNode>> {
    element
        .children()
        .filter_map(ElementRef::wrap)
        .filter(|child| child.value().name() == "li")
        .map(|child| block_children(child, quote_depth))
        .filter(|nodes| !nodes.is_empty())
        .collect()
}

fn table_rows(element: ElementRef<'_>, quote_depth: u8) -> Vec<TableRow> {
    let mut rows = Vec::new();
    collect_table_rows(element, quote_depth, &mut rows);
    rows
}

fn collect_table_rows(element: ElementRef<'_>, quote_depth: u8, rows: &mut Vec<TableRow>) {
    for child in element.children().filter_map(ElementRef::wrap) {
        match child.value().name() {
            "tr" => {
                let row = table_row(child, quote_depth);
                if !row.cells.is_empty() {
                    rows.push(row);
                }
            }
            "thead" | "tbody" | "tfoot" => collect_table_rows(child, quote_depth, rows),
            _ => {}
        }
    }
}

fn table_row(row: ElementRef<'_>, quote_depth: u8) -> TableRow {
    let cells = row
        .children()
        .filter_map(ElementRef::wrap)
        .filter_map(|cell| match cell.value().name() {
            "td" => Some(TableCell {
                header: false,
                colspan: table_span(cell, "colspan"),
                rowspan: table_span(cell, "rowspan"),
                nodes: block_children(cell, quote_depth),
            }),
            "th" => Some(TableCell {
                header: true,
                colspan: table_span(cell, "colspan"),
                rowspan: table_span(cell, "rowspan"),
                nodes: block_children(cell, quote_depth),
            }),
            _ => None,
        })
        .filter(|cell| !cell.nodes.is_empty())
        .collect();

    TableRow { cells }
}

fn table_span(cell: ElementRef<'_>, attr: &str) -> u16 {
    cell.value()
        .attr(attr)
        .and_then(|value| value.trim().parse::<u16>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(1)
        .min(24)
}

fn wrap_inline_runs(nodes: Vec<RenderNode>) -> Vec<RenderNode> {
    let mut output = Vec::new();
    let mut inline = Vec::new();

    for node in nodes {
        if is_inline(&node) {
            inline.push(node);
        } else {
            flush_inline(&mut output, &mut inline);
            output.push(node);
        }
    }

    flush_inline(&mut output, &mut inline);
    output
}

fn flush_inline(output: &mut Vec<RenderNode>, inline: &mut Vec<RenderNode>) {
    if inline.is_empty() {
        return;
    }

    output.push(RenderNode::Paragraph(std::mem::take(inline)));
}

fn is_inline(node: &RenderNode) -> bool {
    matches!(
        node,
        RenderNode::Text(_)
            | RenderNode::Code(_)
            | RenderNode::Link { .. }
            | RenderNode::Image(_)
            | RenderNode::Strong(_)
            | RenderNode::Emphasis(_)
            | RenderNode::LineBreak
    )
}

fn heading_level(name: &str) -> u8 {
    name.trim_start_matches('h')
        .parse::<u8>()
        .unwrap_or(2)
        .clamp(1, 6)
}

fn image_source_from_element(element: ElementRef<'_>) -> Option<ImageSource> {
    if element.value().attr("data-courier-remote-image") == Some("blocked") {
        return Some(ImageSource::RemoteUrl("blocked".to_string()));
    }

    let src = element.value().attr("src")?.trim();
    if src.starts_with("cid:") {
        Some(ImageSource::Cid(src.trim_start_matches("cid:").to_string()))
    } else if src.starts_with("http://") || src.starts_with("https://") {
        Some(ImageSource::RemoteUrl(src.to_string()))
    } else if src.starts_with("attachment:") {
        Some(ImageSource::Attachment(
            src.trim_start_matches("attachment:").to_string(),
        ))
    } else {
        Some(ImageSource::LocalPath(src.to_string()))
    }
}

fn preserved_text(element: ElementRef<'_>) -> String {
    let mut output = String::new();
    push_preserved_text(*element, &mut output);
    output
}

fn push_preserved_text(node: NodeRef<'_, Node>, output: &mut String) {
    match node.value() {
        Node::Text(text) => output.push_str(text),
        Node::Element(element) if element.name() == "br" => output.push('\n'),
        Node::Element(_) => {
            for child in node.children() {
                push_preserved_text(child, output);
            }
        }
        _ => {}
    }
}

fn normalized_text(input: &str) -> Option<String> {
    let text = input.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() { None } else { Some(text) }
}

fn text_fallback(input: &str) -> String {
    let fragment = Html::parse_fragment(input);
    fragment.root_element().text().collect::<Vec<_>>().join(" ")
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
    fn html_render_tree_preserves_lists_tables_and_inline_formatting() {
        let tree = render_tree_from_html(
            r#"
            <h2>Digest</h2>
            <p><strong>Ship</strong> <em>today</em></p>
            <ul><li>One</li><li><a href="https://example.test/two">Two</a></li></ul>
            <table><thead><tr><th>Name</th><th>Status</th></tr></thead><tbody><tr><td>Courier</td><td>Ready</td></tr></tbody></table>
            <hr>
            "#,
        );

        assert!(matches!(
            tree.nodes.first(),
            Some(RenderNode::Heading { level: 2, .. })
        ));
        assert!(tree.nodes.iter().any(
            |node| matches!(node, RenderNode::List { ordered: false, items } if items.len() == 2)
        ));
        assert!(tree.nodes.iter().any(
            |node| matches!(node, RenderNode::Table { rows } if rows.len() == 2 && rows[0].cells[0].header)
        ));
        assert!(
            tree.nodes
                .iter()
                .any(|node| matches!(node, RenderNode::HorizontalRule))
        );
    }

    #[test]
    fn html_render_tree_preserves_code_whitespace() {
        let tree = render_tree_from_html(
            r#"
            <p>Run <code>cargo   check</code> first.</p>
            <pre>
fn main() {
    println!("ready");
}
</pre>
            "#,
        );

        assert!(matches!(
            &tree.nodes[0],
            RenderNode::Paragraph(nodes)
                if matches!(nodes.get(1), Some(RenderNode::Code(value)) if value == "cargo   check")
        ));
        assert!(tree.nodes.iter().any(
            |node| matches!(node, RenderNode::Preformatted(value) if value.contains("    println!") && value.contains('\n'))
        ));
    }

    #[test]
    fn text_render_tree_maps_lines_to_paragraphs() {
        let tree = render_tree_from_text("one\ntwo");

        assert_eq!(tree.nodes.len(), 2);
        assert!(!tree.blocked_remote_images);
    }
}
