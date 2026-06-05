use courier_proto::{MailboxId, MailboxRole, MailboxSummary};
use iced::Element;
use iced::widget::column;

use crate::app::Message;

pub fn view<'a>(
    mailboxes: &'a [MailboxSummary],
    selected_mailbox: Option<&MailboxId>,
) -> Element<'a, Message> {
    let mut list = column![
        crate::components::list::section_label("MAILBOXES"),
        mailbox_row(
            "Unified Inbox",
            "\u{25ce}",
            None,
            0,
            selected_mailbox.is_none(),
        ),
    ]
    .spacing(crate::theme::SPACE_SM)
    .padding(8);

    for mailbox in mailboxes {
        let selected = selected_mailbox == Some(&mailbox.id);
        list = list.push(mailbox_row(
            &mailbox.name,
            role_code(&mailbox.role),
            Some(&mailbox.id),
            mailbox.unread_count,
            selected,
        ));
    }

    list.into()
}

fn mailbox_row<'a>(
    name: &'a str,
    role: &'a str,
    mailbox_id: Option<&'a MailboxId>,
    unread_count: u32,
    selected: bool,
) -> Element<'a, Message> {
    let trailing = if unread_count == 0 {
        None
    } else {
        Some(crate::components::badge::count(unread_count))
    };

    crate::components::list::outline_row(
        crate::components::badge::role(role),
        name,
        trailing,
        selected,
        Message::MailboxSelected(mailbox_id.cloned(), name.to_string()),
    )
}

fn role_code(role: &MailboxRole) -> &'static str {
    match role {
        MailboxRole::Inbox => "\u{21e3}",
        MailboxRole::Sent => "\u{2197}",
        MailboxRole::Drafts => "\u{270e}",
        MailboxRole::Archive => "\u{25a3}",
        MailboxRole::Trash => "\u{232b}",
        MailboxRole::Spam => "!",
        MailboxRole::Custom => "\u{25a1}",
    }
}
