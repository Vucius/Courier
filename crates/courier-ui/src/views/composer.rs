use courier_proto::SendQueueItem;
use iced::Element;
use iced::Length;
use iced::widget::{column, container, row, text};

use crate::app::Message;

pub fn view<'a>(
    to: &'a str,
    subject: &'a str,
    body: &'a str,
    send_queue: &'a [SendQueueItem],
) -> Element<'a, Message> {
    container({
        let mut content = column![
            crate::components::surface::header(
                "Compose",
                crate::components::action_bar::button_primary("Send", Message::SendDraft),
            ),
            crate::components::surface::divider(),
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
            crate::components::form::body_input(
                "Write a reply or new message",
                body,
                Message::DraftBodyChanged,
            ),
        ]
        .spacing(0);

        if !send_queue.is_empty() {
            content = content.push(send_queue_view(send_queue));
        }

        content
    })
    .height(Length::FillPortion(2))
    .into()
}

fn send_queue_view<'a>(queue: &'a [SendQueueItem]) -> Element<'a, Message> {
    let mut content = column![crate::components::list::section_label("Send queue")]
        .spacing(6)
        .padding([8, 12]);

    for item in queue {
        content = content.push(send_queue_row(item));
    }

    content.into()
}

fn send_queue_row<'a>(item: &'a SendQueueItem) -> Element<'a, Message> {
    let subject = if item.subject.trim().is_empty() {
        "(no subject)"
    } else {
        item.subject.as_str()
    };
    let detail = format!(
        "{} - {} - attempt {}{}",
        item.status,
        send_timing_label(item),
        item.retry_count,
        item.last_error
            .as_ref()
            .map(|error| format!(" - {error}"))
            .unwrap_or_default()
    );

    let mut actions = row![].spacing(6);
    if item.status == "failed" || item.status == "cancelled" {
        actions = actions.push(crate::components::action_bar::button_text(
            "Retry",
            Message::RetrySend(item.draft_id.clone()),
        ));
    }
    if item.status == "pending" || item.status == "failed" {
        let label = if item.status == "pending" {
            "Undo"
        } else {
            "Cancel"
        };
        actions = actions.push(crate::components::action_bar::button_text(
            label,
            Message::CancelSend(item.draft_id.clone()),
        ));
    }

    row![
        column![
            text(subject).size(13).color(crate::theme::TEXT),
            text(format!("To: {}", item.to.join(", ")))
                .size(12)
                .color(crate::theme::TEXT_MUTED),
            text(detail).size(12).color(crate::theme::TEXT_MUTED),
        ]
        .spacing(3)
        .width(Length::Fill),
        actions,
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center)
    .width(Length::Fill)
    .into()
}

fn send_timing_label(item: &SendQueueItem) -> String {
    if item.status == "pending" && item.retry_count == 0 {
        "undo window".to_string()
    } else if item.status == "pending" {
        "scheduled retry".to_string()
    } else {
        "manual action".to_string()
    }
}
