use courier_proto::{MailboxId, MailboxRole, MailboxSummary};
use iced::Element;
use iced::widget::column;

use crate::app::Message;
use crate::components::icon::Icon;

pub fn view<'a>(
    mailboxes: &'a [MailboxSummary],
    selected_mailbox: Option<&MailboxId>,
) -> Element<'a, Message> {
    let mut list = column![
        crate::components::list::section_label("MAILBOXES"),
        mailbox_row(
            "Unified Inbox",
            Icon::Inbox,
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
            role_icon(&mailbox.role),
            Some(&mailbox.id),
            mailbox.unread_count,
            selected,
        ));
    }

    list.into()
}

fn mailbox_row<'a>(
    name: &'a str,
    icon: Icon,
    mailbox_id: Option<&'a MailboxId>,
    unread_count: u32,
    selected: bool,
) -> Element<'a, Message> {
    let trailing = if unread_count == 0 {
        None
    } else {
        Some(crate::components::badge::count(unread_count))
    };

    let icon_color = if selected {
        crate::theme::ACCENT
    } else {
        crate::theme::TEXT_MUTED
    };

    crate::components::list::outline_row(
        icon.view_styled(16.0, icon_color),
        name,
        trailing,
        selected,
        Message::MailboxSelected(mailbox_id.cloned(), name.to_string()),
    )
}

fn role_icon(role: &MailboxRole) -> Icon {
    match role {
        MailboxRole::Inbox => Icon::Inbox,
        MailboxRole::Sent => Icon::Send,
        MailboxRole::Drafts => Icon::Drafts,
        MailboxRole::Archive => Icon::Archive,
        MailboxRole::Trash => Icon::Delete,
        MailboxRole::Spam => Icon::Warning,
        MailboxRole::Custom => Icon::Folder,
    }
}

