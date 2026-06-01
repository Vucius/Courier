use courier_proto::{MailboxId, MailboxRole, MailboxSummary};
use iced::Element;
use iced::widget::{column, text};

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
        crate::components::list::section_label("MAILBOXES"),
        mailbox_row("Unified Inbox", "ALL", None, 0, selected_mailbox.is_none(),),
    ]
    .spacing(8)
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
        MailboxRole::Inbox => "IN",
        MailboxRole::Sent => "SE",
        MailboxRole::Drafts => "DR",
        MailboxRole::Archive => "AR",
        MailboxRole::Trash => "TR",
        MailboxRole::Spam => "SP",
        MailboxRole::Custom => "FO",
    }
}
