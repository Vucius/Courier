use courier_proto::{
    AttachmentOpenRequest, AttachmentPreview, AttachmentPreviewKind, AttachmentSummary, MessageBody,
};
use courier_render::{ImageSource, RenderNode, RenderTree, TableCell};
use courier_security::{AttachmentRisk, classify_attachment};
use iced::font::{Style, Weight};
use iced::widget::{column, container, row, scrollable, text};
use iced::{Background, Border, Element, Font, Length};

use crate::app::Message;

pub fn view<'a>(
    body: Option<&'a MessageBody>,
    render_tree: Option<&'a RenderTree>,
    attachment_preview: Option<&'a AttachmentPreview>,
    attachment_open: Option<&'a AttachmentOpenRequest>,
) -> Element<'a, Message> {
    match body {
        Some(body) => {
            let recipients = if body.to.is_empty() {
                "No recipients".to_string()
            } else {
                body.to.join(", ")
            };
            let rendered_body = render_tree_view(render_tree, &body.body);
            let mut content = column![
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
            ]
            .height(Length::Fill);

            if !body.attachments.is_empty() {
                content = content.push(attachments_view(
                    &body.attachments,
                    attachment_preview,
                    attachment_open,
                ));
            }

            content = content.push(rendered_body);

            container(content).height(Length::FillPortion(3)).into()
        }
        None => crate::components::empty_state::view(
            "Select a message",
            "The message body and reply actions will appear here.",
        ),
    }
}

fn attachments_view<'a>(
    attachments: &'a [AttachmentSummary],
    preview: Option<&'a AttachmentPreview>,
    open_request: Option<&'a AttachmentOpenRequest>,
) -> Element<'a, Message> {
    let mut content = column![crate::components::list::section_label("Attachments")]
        .spacing(6)
        .padding([8, 12]);

    if let Some(notice) = attachment_policy_notice(attachments) {
        content = content.push(crate::components::notice::inline(
            crate::components::notice::NoticeKind::Warning,
            notice,
        ));
    }

    for attachment in attachments {
        content = content.push(attachment_row(attachment));
    }

    if let Some(preview) = preview {
        content = content.push(attachment_preview_view(preview));
    }

    if let Some(open_request) = open_request {
        content = content.push(attachment_open_view(open_request));
    }

    content.into()
}

fn attachment_row<'a>(attachment: &'a AttachmentSummary) -> Element<'a, Message> {
    row![
        crate::components::attachment::chip(
            attachment.filename.clone(),
            attachment_detail(attachment),
        ),
        crate::components::action_bar::button_text(
            "Preview",
            Message::PreviewAttachment(attachment.id.clone()),
        ),
        crate::components::action_bar::button_text(
            "Open",
            Message::OpenAttachment(attachment.id.clone()),
        ),
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center)
    .width(Length::Fill)
    .into()
}

fn attachment_preview_view<'a>(preview: &'a AttachmentPreview) -> Element<'a, Message> {
    let mut content = column![
        row![
            crate::components::list::section_label("Attachment preview"),
            iced::widget::horizontal_space(),
            crate::components::action_bar::button_text(
                "Dismiss",
                Message::DismissAttachmentNotice,
            ),
        ]
        .align_y(iced::Alignment::Center),
        text(&preview.attachment.filename)
            .size(13)
            .color(crate::theme::TEXT),
        text(&preview.message)
            .size(12)
            .color(crate::theme::TEXT_MUTED),
    ]
    .spacing(6);

    match preview.kind {
        AttachmentPreviewKind::Text => {
            if let Some(content_text) = preview.content.as_ref() {
                content = content.push(
                    container(
                        text(content_text)
                            .size(12)
                            .color(crate::theme::TEXT)
                            .width(Length::Fill),
                    )
                    .padding(8)
                    .width(Length::Fill)
                    .style(|_| container::Style {
                        background: Some(Background::Color(crate::theme::SURFACE)),
                        border: Border {
                            width: 1.0,
                            radius: 4.0.into(),
                            color: crate::theme::BORDER,
                        },
                        ..container::Style::default()
                    }),
                );
            }
        }
        AttachmentPreviewKind::Image => {
            content = content.push(crate::components::attachment::image_placeholder(
                preview.path.as_deref().unwrap_or("Image attachment"),
            ));
        }
        AttachmentPreviewKind::Unsupported
        | AttachmentPreviewKind::MissingBlob
        | AttachmentPreviewKind::Blocked => {}
    }

    container(content)
        .padding(8)
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(crate::theme::SURFACE_ALT)),
            border: Border {
                width: 1.0,
                radius: 6.0.into(),
                color: crate::theme::BORDER,
            },
            ..container::Style::default()
        })
        .into()
}

fn attachment_open_view<'a>(request: &'a AttachmentOpenRequest) -> Element<'a, Message> {
    let kind = if request.allowed {
        crate::components::notice::NoticeKind::Success
    } else {
        crate::components::notice::NoticeKind::Error
    };
    let message = if request.allowed {
        match request.path.as_ref() {
            Some(path) => format!("Ready to open {} at {}", request.attachment.filename, path),
            None => format!("{} has no local file to open", request.attachment.filename),
        }
    } else {
        format!("{}: {}", request.attachment.filename, request.reason)
    };

    crate::components::notice::inline(kind, message)
}

fn attachment_detail(attachment: &AttachmentSummary) -> String {
    let decision =
        classify_attachment(&attachment.filename, &attachment.mime_type, attachment.size);
    format!(
        "{} - {} - {}",
        attachment.mime_type,
        format_size(attachment.size),
        decision.reason
    )
}

fn attachment_policy_notice(attachments: &[AttachmentSummary]) -> Option<&'static str> {
    let mut has_caution = false;
    for attachment in attachments {
        match classify_attachment(&attachment.filename, &attachment.mime_type, attachment.size).risk
        {
            AttachmentRisk::Blocked => {
                return Some("One or more attachments are blocked by policy.");
            }
            AttachmentRisk::Caution => has_caution = true,
            AttachmentRisk::Low => {}
        }
    }

    if has_caution {
        Some("Review attachment details before opening.")
    } else {
        None
    }
}

fn format_size(size: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;

    if size >= MIB {
        format!("{:.1} MB", size as f64 / MIB as f64)
    } else if size >= KIB {
        format!("{:.1} KB", size as f64 / KIB as f64)
    } else {
        format!("{size} B")
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
        RenderNode::Heading { level, children } => text(children_text(children))
            .size(heading_size(*level))
            .color(crate::theme::TEXT)
            .font(Font {
                weight: Weight::Semibold,
                ..Font::DEFAULT
            })
            .width(Length::Fill)
            .into(),
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
            container(quote)
                .padding([8, 10])
                .width(Length::Fill)
                .style(|_| container::Style {
                    background: Some(Background::Color(crate::theme::SURFACE_ALT)),
                    border: Border {
                        width: 1.0,
                        radius: 4.0.into(),
                        color: crate::theme::BORDER,
                    },
                    ..container::Style::default()
                })
                .into()
        }
        RenderNode::List { ordered, items } => {
            let mut list = column![].spacing(6).width(Length::Fill);
            for (index, item) in items.iter().enumerate() {
                let marker = if *ordered {
                    format!("{}.", index + 1)
                } else {
                    "-".to_string()
                };
                let mut item_body = column![].spacing(4).width(Length::Fill);
                for node in item {
                    item_body = item_body.push(render_node(node));
                }
                list = list.push(
                    row![
                        text(marker)
                            .size(14)
                            .color(crate::theme::TEXT_MUTED)
                            .width(Length::Fixed(24.0)),
                        item_body,
                    ]
                    .spacing(6)
                    .width(Length::Fill),
                );
            }
            list.into()
        }
        RenderNode::Strong(children) => render_styled_inline(children, Weight::Bold, Style::Normal),
        RenderNode::Emphasis(children) => {
            render_styled_inline(children, Weight::Normal, Style::Italic)
        }
        RenderNode::HorizontalRule => crate::components::surface::divider(),
        RenderNode::LineBreak => text("").height(Length::Fixed(6.0)).into(),
        RenderNode::Table { rows } => table_view(rows),
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
        RenderNode::Strong(children) => {
            inline_text(children, Weight::Bold, Style::Normal, crate::theme::TEXT)
        }
        RenderNode::Emphasis(children) => {
            inline_text(children, Weight::Normal, Style::Italic, crate::theme::TEXT)
        }
        RenderNode::LineBreak => text(" ").width(Length::Fixed(1.0)).into(),
        other => render_node(other),
    }
}

fn render_styled_inline<'a>(
    children: &'a [RenderNode],
    weight: Weight,
    style: Style,
) -> Element<'a, Message> {
    inline_text(children, weight, style, crate::theme::TEXT)
}

fn inline_text<'a>(
    children: &'a [RenderNode],
    weight: Weight,
    style: Style,
    color: iced::Color,
) -> Element<'a, Message> {
    text(children_text(children))
        .size(14)
        .color(color)
        .font(Font {
            weight,
            style,
            ..Font::DEFAULT
        })
        .into()
}

fn table_view<'a>(rows: &'a [courier_render::TableRow]) -> Element<'a, Message> {
    let mut table = column![].spacing(0).width(Length::Fill);

    for row_data in rows {
        let mut table_row = row![].spacing(0).width(Length::Fill);
        for cell in &row_data.cells {
            table_row = table_row.push(table_cell(cell));
        }
        table = table.push(table_row);
    }

    table.into()
}

fn table_cell<'a>(cell: &'a TableCell) -> Element<'a, Message> {
    let mut content = column![].spacing(4).width(Length::Fill);
    for node in &cell.nodes {
        content = content.push(render_node(node));
    }

    let background = if cell.header {
        crate::theme::SURFACE_ALT
    } else {
        crate::theme::SURFACE
    };

    container(content)
        .padding(8)
        .width(Length::FillPortion(1))
        .style(move |_| container::Style {
            background: Some(Background::Color(background)),
            border: Border {
                width: 1.0,
                radius: 0.0.into(),
                color: crate::theme::BORDER,
            },
            ..container::Style::default()
        })
        .into()
}

fn heading_size(level: u8) -> u16 {
    match level {
        1 => 22,
        2 => 19,
        3 => 17,
        _ => 15,
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
            RenderNode::Text(value) => Some(value.clone()),
            RenderNode::Link { children, .. }
            | RenderNode::Strong(children)
            | RenderNode::Emphasis(children)
            | RenderNode::Paragraph(children)
            | RenderNode::Heading { children, .. } => Some(children_text(children)),
            RenderNode::Image(_) => Some("[image]".to_string()),
            RenderNode::LineBreak => Some("\n".to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}
