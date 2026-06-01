use courier_proto::MessageBody;
use courier_render::{ImageSource, RenderNode, RenderTree};
use iced::Element;
use iced::Length;
use iced::widget::{column, container, row, scrollable, text};

use crate::app::Message;

pub fn view<'a>(
    body: Option<&'a MessageBody>,
    render_tree: Option<&'a RenderTree>,
) -> Element<'a, Message> {
    match body {
        Some(body) => {
            let recipients = if body.to.is_empty() {
                "No recipients".to_string()
            } else {
                body.to.join(", ")
            };
            let rendered_body = render_tree_view(render_tree, &body.body);

            container(
                column![
                    crate::components::surface::header(
                        &body.subject,
                        crate::components::action_bar::button_text("Reply", Message::Compose),
                    ),
                    crate::components::surface::divider(),
                    row![
                        crate::components::avatar::view(&body.from, false),
                        crate::components::list::metadata_rows(vec![
                            ("From", body.from.clone()),
                            ("To", recipients),
                        ]),
                    ]
                    .spacing(10)
                    .padding(12),
                    rendered_body,
                ]
                .height(Length::Fill),
            )
            .height(Length::FillPortion(3))
            .into()
        }
        None => crate::components::empty_state::view(
            "Select a message",
            "The message body and reply actions will appear here.",
        ),
    }
}

fn render_tree_view<'a>(
    render_tree: Option<&'a RenderTree>,
    fallback: &'a str,
) -> Element<'a, Message> {
    let mut content = column![].spacing(8).padding(12).width(Length::Fill);

    match render_tree {
        Some(tree) => {
            if tree.blocked_remote_images {
                content = content.push(crate::components::notice::inline(
                    crate::components::notice::NoticeKind::Warning,
                    "Remote images were blocked for this message.",
                ));
            }

            for node in &tree.nodes {
                content = content.push(render_node(node));
            }
        }
        None => {
            content = content.push(
                text(fallback)
                    .size(14)
                    .color(crate::theme::TEXT)
                    .width(Length::Fill),
            );
        }
    }

    scrollable(content).height(Length::Fill).into()
}

fn render_node<'a>(node: &'a RenderNode) -> Element<'a, Message> {
    match node {
        RenderNode::Text(value) => text(value)
            .size(14)
            .color(crate::theme::TEXT)
            .width(Length::Fill)
            .into(),
        RenderNode::Paragraph(children) => {
            let mut line = row![].spacing(4).width(Length::Fill);
            for child in children {
                line = line.push(render_inline_node(child));
            }
            line.into()
        }
        RenderNode::Link { href, children } => {
            let label = children_text(children);
            column![
                text(label).size(14).color(crate::theme::ACCENT),
                text(href).size(11).color(crate::theme::TEXT_MUTED),
            ]
            .spacing(2)
            .into()
        }
        RenderNode::Image(source) => image_label(source),
        RenderNode::BlockQuote(children) => {
            let mut quote = column![].spacing(6).padding(8).width(Length::Fill);
            for child in children {
                quote = quote.push(render_node(child));
            }
            container(quote).width(Length::Fill).into()
        }
        RenderNode::Table(children) => {
            let mut table = column![].spacing(4).width(Length::Fill);
            for child in children {
                table = table.push(render_node(child));
            }
            table.into()
        }
    }
}

fn render_inline_node<'a>(node: &'a RenderNode) -> Element<'a, Message> {
    match node {
        RenderNode::Text(value) => text(value).size(14).color(crate::theme::TEXT).into(),
        RenderNode::Link { href, children } => {
            let label = children_text(children);
            text(format!("{label} ({href})"))
                .size(14)
                .color(crate::theme::ACCENT)
                .into()
        }
        RenderNode::Image(source) => image_label(source),
        other => render_node(other),
    }
}

fn image_label<'a>(source: &'a ImageSource) -> Element<'a, Message> {
    match source {
        ImageSource::RemoteUrl(_) => {
            crate::components::attachment::image_placeholder("Remote image blocked")
        }
        ImageSource::Cid(value) => crate::components::attachment::chip(value, "inline image"),
        ImageSource::Attachment(value) => {
            crate::components::attachment::chip(value, "attachment image")
        }
        ImageSource::LocalPath(value) => crate::components::attachment::chip(value, "image"),
    }
}

fn children_text(children: &[RenderNode]) -> String {
    children
        .iter()
        .filter_map(|node| match node {
            RenderNode::Text(value) => Some(value.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}
