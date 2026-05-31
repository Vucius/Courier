use courier_proto::{MailboxId, MailboxSummary};
use iced::Element;
use iced::widget::{button, column, row, text};
use iced::{Alignment, Length};

use crate::app::Message;

pub fn view<'a>(
    mailboxes: &'a [MailboxSummary],
    selected_mailbox: Option<&MailboxId>,
) -> Element<'a, Message> {
    let mut list = column![
        crate::components::surface::header(
            "Courier",
            text("Local").size(12).color(crate::theme::TEXT_MUTED),
        ),
        crate::components::surface::divider(),
        crate::components::surface::section_title("MAILBOXES"),
        mailbox_row("Unified Inbox", None, 0, selected_mailbox.is_none()),
    ]
    .spacing(8)
    .padding(8);

    for mailbox in mailboxes {
        let selected = selected_mailbox == Some(&mailbox.id);
        list = list.push(mailbox_row(
            &mailbox.name,
            Some(&mailbox.id),
            mailbox.unread_count,
            selected,
        ));
    }

    list.into()
}

fn mailbox_row<'a>(
    name: &'a str,
    mailbox_id: Option<&'a MailboxId>,
    unread_count: u32,
    selected: bool,
) -> Element<'a, Message> {
    let unread = if unread_count == 0 {
        text("").into()
    } else {
        crate::components::surface::badge(unread_count)
    };

    let content = row![
        text(name).size(14).color(crate::theme::TEXT),
        iced::widget::horizontal_space(),
        unread,
    ]
    .align_y(Alignment::Center)
    .spacing(8)
    .width(Length::Fill);

    let row = if selected {
        crate::components::surface::row_surface(content, true)
    } else {
        iced::widget::container(content)
    };

    button(row)
        .style(iced::widget::button::text)
        .padding(0)
        .width(Length::Fill)
        .on_press(Message::MailboxSelected(
            mailbox_id.cloned(),
            name.to_string(),
        ))
        .into()
}
