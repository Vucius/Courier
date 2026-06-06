use courier_proto::{
    AttachmentOpenRequest, AttachmentPreview, AttachmentPreviewKind, AttachmentSummary,
    AttachmentTransfer, AttachmentTransferStatus, MessageBody,
};
use courier_render::{ImageSource, RenderNode, RenderTree, TableCell};
use courier_security::{AttachmentRisk, classify_attachment};
use iced::font::{Style, Weight};
use iced::widget::{column, container, progress_bar, row, scrollable, text};
use iced::{Background, Border, Element, Font, Length};

use crate::app::Message;
use crate::components::icon::Icon;

pub struct ReaderViewState<'a> {
    pub body: Option<&'a MessageBody>,
    pub render_tree: Option<&'a RenderTree>,
    pub attachment_preview: Option<&'a AttachmentPreview>,
    pub attachment_open: Option<&'a AttachmentOpenRequest>,
    pub attachment_transfers: &'a [AttachmentTransfer],
    pub inline_reply_open: bool,
    pub draft_to: &'a str,
    pub draft_subject: &'a str,
    pub draft_body: &'a str,
}

pub fn view<'a>(state: ReaderViewState<'a>) -> Element<'a, Message> {
    match state.body {
        Some(body) => {
            let recipients = if body.to.is_empty() {
                "No recipients".to_string()
            } else {
                body.to.join(", ")
            };
            let rendered_body = render_tree_view(state.render_tree, &body.body, &body.attachments);
            let mut content = column![
                crate::components::surface::header(
                    &body.subject,
                    crate::components::action_bar::button_primary_with_icon("Reply", Icon::Reply, Message::ReplyInline),
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
                    state.attachment_preview,
                    state.attachment_open,
                    state.attachment_transfers,
                ));
            }

            content = content.push(rendered_body);

            if state.inline_reply_open {
                content = content.push(inline_reply_view(
                    state.draft_to,
                    state.draft_subject,
                    state.draft_body,
                ));
            }

            container(content).height(Length::FillPortion(3)).into()
        }
        None => {
            container(
                column![
                    container(text("@").size(24).color(crate::theme::ACCENT))
                        .width(Length::Fixed(48.0))
                        .height(Length::Fixed(48.0))
                        .center_x(Length::Fixed(48.0))
                        .center_y(Length::Fixed(48.0))
                        .style(|_| container::Style {
                            background: Some(Background::Color(crate::theme::ROW_SELECTED)),
                            border: Border {
                                width: 1.0,
                                radius: 24.0.into(),
                                color: crate::theme::ACCENT_MUTED,
                            },
                            ..container::Style::default()
                        }),
                    text("No message selected").size(16).color(crate::theme::TEXT),
                    text("Select an email from the list to read, reply, archive, or delete it.")
                        .size(13)
                        .color(crate::theme::TEXT_MUTED),
                    row![
                        crate::components::action_bar::button_primary("Compose new email", Message::Compose),
                        crate::components::action_bar::button_toolbar("Sync inbox", Message::SyncNow),
                    ]
                    .spacing(10)
                    .align_y(iced::Alignment::Center),
                ]
                .align_x(iced::Alignment::Center)
                .spacing(12),
            )
            .center(Length::Fill)
            .into()
        }
    }
}

fn inline_reply_view<'a>(to: &'a str, subject: &'a str, body: &'a str) -> Element<'a, Message> {
    container(
        column![
            row![
                crate::components::list::section_label("Reply"),
                iced::widget::horizontal_space(),
                crate::components::action_bar::button_text_with_icon("Close", Icon::Delete, crate::theme::TEXT_MUTED, Message::CloseInlineReply),
                crate::components::action_bar::button_primary_with_icon("Send", Icon::Send, Message::SendDraft),
            ]
            .align_y(iced::Alignment::Center),
            crate::components::form::labeled_input(
                "To",
                "name@example.com",
                to,
                Message::DraftToChanged,
            ),
            crate::components::form::labeled_input(
                "Subject",
                "Subject",
                subject,
                Message::DraftSubjectChanged,
            ),
            crate::components::form::body_input("Write a reply", body, Message::DraftBodyChanged),
        ]
        .spacing(crate::theme::SPACE_SM),
    )
    .padding(10)
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(Background::Color(crate::theme::SURFACE_ALT)),
        border: Border {
            width: 1.0,
            radius: crate::theme::RADIUS_LG.into(),
            color: crate::theme::BORDER,
        },
        ..container::Style::default()
    })
    .into()
}

fn attachments_view<'a>(
    attachments: &'a [AttachmentSummary],
    preview: Option<&'a AttachmentPreview>,
    open_request: Option<&'a AttachmentOpenRequest>,
    transfers: &'a [AttachmentTransfer],
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
        let transfer = transfers
            .iter()
            .find(|transfer| transfer.attachment.id == attachment.id);
        content = content.push(attachment_row(attachment, transfer));
    }

    if let Some(preview) = preview {
        content = content.push(attachment_preview_view(preview));
    }

    if let Some(open_request) = open_request {
        content = content.push(attachment_open_view(open_request));
    }

    content.into()
}

fn attachment_row<'a>(
    attachment: &'a AttachmentSummary,
    transfer: Option<&'a AttachmentTransfer>,
) -> Element<'a, Message> {
    let progress = transfer
        .map(|transfer| transfer.progress)
        .unwrap_or_else(|| {
            if attachment.blob_path.is_some() {
                1.0
            } else {
                0.0
            }
        });
    let message = transfer
        .map(|transfer| transfer.message.as_str())
        .unwrap_or_else(|| {
            if attachment.blob_path.is_some() {
                "Available locally"
            } else {
                "Not downloaded"
            }
        });

    let mut actions = row![].spacing(6);
    match transfer.map(|transfer| &transfer.status) {
        Some(AttachmentTransferStatus::Downloading) => {
            actions = actions.push(crate::components::action_bar::button_text(
                "Cancel",
                Message::CancelAttachmentDownload(attachment.id.clone()),
            ));
        }
        Some(AttachmentTransferStatus::Failed | AttachmentTransferStatus::Cancelled)
        | Some(AttachmentTransferStatus::Missing)
        | None
            if attachment.blob_path.is_none() =>
        {
            actions = actions.push(crate::components::action_bar::button_text(
                "Download",
                Message::DownloadAttachment(attachment.id.clone()),
            ));
            actions = actions.push(crate::components::action_bar::button_text(
                "Retry",
                Message::RetryAttachmentDownload(attachment.id.clone()),
            ));
        }
        _ => {
            actions = actions.push(crate::components::action_bar::button_text(
                "Preview",
                Message::PreviewAttachment(attachment.id.clone()),
            ));
            actions = actions.push(crate::components::action_bar::button_text(
                "Open",
                Message::OpenAttachment(attachment.id.clone()),
            ));
        }
    }

    column![
        row![
            crate::components::attachment::chip(
                attachment.filename.clone(),
                attachment_detail(attachment),
            ),
            actions,
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center)
        .width(Length::Fill),
        row![
            progress_bar(0.0..=1.0, progress.clamp(0.0, 1.0)).width(Length::Fixed(160.0)),
            text(message).size(12).color(crate::theme::TEXT_MUTED),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center)
        .width(Length::Fill),
    ]
    .spacing(4)
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
        AttachmentPreviewKind::Pdf => {
            content = content.push(
                container(
                    column![
                        text("PDF document").size(14).color(crate::theme::TEXT),
                        text(preview.path.as_deref().unwrap_or("Local PDF attachment"))
                            .size(12)
                            .color(crate::theme::TEXT_MUTED),
                        crate::components::action_bar::button_text(
                            "Open PDF",
                            Message::OpenAttachment(preview.attachment.id.clone()),
                        ),
                    ]
                    .spacing(6),
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

    let mut content = row![
        crate::components::notice::inline(kind, message),
        iced::widget::horizontal_space(),
        crate::components::action_bar::button_text("Dismiss", Message::DismissAttachmentNotice),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center)
    .width(Length::Fill);

    if request.allowed && request.path.is_some() {
        content = content.push(crate::components::action_bar::button_text(
            "Open now",
            Message::ConfirmOpenAttachment(request.attachment.id.clone()),
        ));
    }

    content.into()
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
    attachments: &'a [AttachmentSummary],
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
                content = content.push(render_node(node, attachments));
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

fn render_node<'a>(
    node: &'a RenderNode,
    attachments: &'a [AttachmentSummary],
) -> Element<'a, Message> {
    match node {
        RenderNode::Text(value) => text(value)
            .size(14)
            .color(crate::theme::TEXT)
            .width(Length::Fill)
            .into(),
        RenderNode::Code(value) => code_inline(value),
        RenderNode::Preformatted(value) => preformatted_block(value),
        RenderNode::Paragraph(children) => {
            let mut line = row![].spacing(4).width(Length::Fill);
            for child in children {
                line = line.push(render_inline_node(child, attachments));
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
        RenderNode::Image(source) => image_label(source, attachments),
        RenderNode::BlockQuote {
            depth,
            collapsed,
            children,
        } => {
            let mut quote = column![].spacing(6).padding(8).width(Length::Fill);
            if *collapsed {
                quote = quote.push(
                    text(format!("Quoted reply depth {} collapsed", depth))
                        .size(12)
                        .color(crate::theme::TEXT_MUTED),
                );
            } else {
                for child in children {
                    quote = quote.push(render_node(child, attachments));
                }
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
                    item_body = item_body.push(render_node(node, attachments));
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
        RenderNode::Table { rows } => table_view(rows, attachments),
    }
}

fn render_inline_node<'a>(
    node: &'a RenderNode,
    attachments: &'a [AttachmentSummary],
) -> Element<'a, Message> {
    match node {
        RenderNode::Text(value) => text(value).size(14).color(crate::theme::TEXT).into(),
        RenderNode::Code(value) => code_inline(value),
        RenderNode::Preformatted(value) => preformatted_block(value),
        RenderNode::Link { href, children } => {
            let label = children_text(children);
            text(format!("{label} ({href})"))
                .size(14)
                .color(crate::theme::ACCENT)
                .into()
        }
        RenderNode::Image(source) => image_label(source, attachments),
        RenderNode::Strong(children) => {
            inline_text(children, Weight::Bold, Style::Normal, crate::theme::TEXT)
        }
        RenderNode::Emphasis(children) => {
            inline_text(children, Weight::Normal, Style::Italic, crate::theme::TEXT)
        }
        RenderNode::LineBreak => text(" ").width(Length::Fixed(1.0)).into(),
        other => render_node(other, attachments),
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

fn code_inline<'a>(value: &'a str) -> Element<'a, Message> {
    container(
        text(value)
            .size(13)
            .color(crate::theme::TEXT)
            .font(Font::MONOSPACE),
    )
    .padding([2, 5])
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

fn preformatted_block<'a>(value: &'a str) -> Element<'a, Message> {
    container(
        scrollable(
            text(value)
                .size(13)
                .color(crate::theme::TEXT)
                .font(Font::MONOSPACE),
        )
        .direction(scrollable::Direction::Both {
            vertical: scrollable::Scrollbar::default(),
            horizontal: scrollable::Scrollbar::default(),
        }),
    )
    .padding(10)
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

fn table_view<'a>(
    rows: &'a [courier_render::TableRow],
    attachments: &'a [AttachmentSummary],
) -> Element<'a, Message> {
    let mut table = column![].spacing(0).width(Length::Fill);

    for row_data in rows {
        let mut table_row = row![].spacing(0).width(Length::Fill);
        for cell in &row_data.cells {
            table_row = table_row.push(table_cell(cell, attachments));
        }
        table = table.push(table_row);
    }

    table.into()
}

fn table_cell<'a>(
    cell: &'a TableCell,
    attachments: &'a [AttachmentSummary],
) -> Element<'a, Message> {
    let mut content = column![].spacing(4).width(Length::Fill);
    for node in &cell.nodes {
        content = content.push(render_node(node, attachments));
    }

    let background = if cell.header {
        crate::theme::SURFACE_ALT
    } else {
        crate::theme::SURFACE
    };

    container(content)
        .padding(8)
        .width(Length::FillPortion(cell.colspan.max(1)))
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

fn image_label<'a>(
    source: &'a ImageSource,
    attachments: &'a [AttachmentSummary],
) -> Element<'a, Message> {
    if let Some(attachment) = inline_image_attachment(source, attachments) {
        return inline_image_attachment_view(attachment);
    }

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

fn inline_image_attachment<'a>(
    source: &ImageSource,
    attachments: &'a [AttachmentSummary],
) -> Option<&'a AttachmentSummary> {
    match source {
        ImageSource::Cid(value) => {
            let needle = normalize_cid(value);
            attachments.iter().find(|attachment| {
                attachment.inline
                    && attachment
                        .content_id
                        .as_deref()
                        .map(normalize_cid)
                        .is_some_and(|content_id| content_id == needle)
            })
        }
        ImageSource::Attachment(value) | ImageSource::LocalPath(value) => {
            let needle = value.trim();
            attachments.iter().find(|attachment| {
                attachment.id.0 == needle || attachment.filename.eq_ignore_ascii_case(needle)
            })
        }
        ImageSource::RemoteUrl(_) => None,
    }
}

fn inline_image_attachment_view<'a>(attachment: &'a AttachmentSummary) -> Element<'a, Message> {
    let state = if attachment.blob_path.is_some() {
        "inline image available locally"
    } else {
        "inline image not downloaded"
    };
    let mut actions = row![
        crate::components::action_bar::button_text(
            "Preview",
            Message::PreviewAttachment(attachment.id.clone()),
        ),
        crate::components::action_bar::button_text(
            "Open",
            Message::OpenAttachment(attachment.id.clone())
        ),
    ]
    .spacing(6);

    if attachment.blob_path.is_none() {
        actions = actions.push(crate::components::action_bar::button_text(
            "Download",
            Message::DownloadAttachment(attachment.id.clone()),
        ));
    }

    container(
        row![
            crate::components::attachment::image_placeholder(&attachment.filename),
            text(state).size(12).color(crate::theme::TEXT_MUTED),
            iced::widget::horizontal_space(),
            actions,
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center)
        .width(Length::Fill),
    )
    .padding(8)
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

fn normalize_cid(value: &str) -> String {
    value
        .trim()
        .trim_start_matches("cid:")
        .trim_matches(['<', '>'])
        .to_ascii_lowercase()
}

fn children_text(children: &[RenderNode]) -> String {
    children
        .iter()
        .filter_map(|node| match node {
            RenderNode::Text(value) => Some(value.clone()),
            RenderNode::Code(value) | RenderNode::Preformatted(value) => Some(value.clone()),
            RenderNode::Link { children, .. }
            | RenderNode::Strong(children)
            | RenderNode::Emphasis(children)
            | RenderNode::Paragraph(children)
            | RenderNode::Heading { children, .. } => Some(children_text(children)),
            RenderNode::BlockQuote { children, .. } => Some(children_text(children)),
            RenderNode::Image(_) => Some("[image]".to_string()),
            RenderNode::LineBreak => Some("\n".to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}
