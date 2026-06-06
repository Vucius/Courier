use courier_proto::{MailboxId, MailboxRole, MailboxSummary};
use iced::Element;
use iced::widget::column;

use crate::app::Message;
use crate::components::icon::Icon;

pub fn view<'a>(
    mailboxes: &'a [MailboxSummary],
    selected_mailbox: Option<&MailboxId>,
    selected_mailbox_name: &str,
) -> Element<'a, Message> {
    let inbox_unread: u32 = mailboxes
        .iter()
        .filter(|m| matches!(m.role, MailboxRole::Inbox))
        .map(|m| m.unread_count)
        .sum();

    let sent_mailbox = mailboxes.iter().find(|m| matches!(m.role, MailboxRole::Sent));
    let drafts_mailbox = mailboxes.iter().find(|m| matches!(m.role, MailboxRole::Drafts));
    let archive_mailbox = mailboxes.iter().find(|m| matches!(m.role, MailboxRole::Archive));
    let trash_mailbox = mailboxes.iter().find(|m| matches!(m.role, MailboxRole::Trash));

    let mut list = column![
        crate::components::list::section_label("UNIFIED"),
        mailbox_row(
            "Inbox",
            Icon::Inbox,
            None,
            inbox_unread,
            selected_mailbox.is_none() && (selected_mailbox_name == "Inbox" || selected_mailbox_name == "Unified Inbox"),
        ),
        mailbox_row(
            "Starred",
            Icon::Star,
            None,
            0,
            selected_mailbox_name == "Starred",
        ),
        mailbox_row(
            "Sent",
            Icon::Send,
            sent_mailbox.map(|m| &m.id),
            0,
            selected_mailbox_name == "Sent" || (sent_mailbox.is_some() && selected_mailbox == sent_mailbox.map(|m| &m.id)),
        ),
        mailbox_row(
            "Drafts",
            Icon::Drafts,
            drafts_mailbox.map(|m| &m.id),
            0,
            selected_mailbox_name == "Drafts" || (drafts_mailbox.is_some() && selected_mailbox == drafts_mailbox.map(|m| &m.id)),
        ),
        mailbox_row(
            "Archive",
            Icon::Archive,
            archive_mailbox.map(|m| &m.id),
            0,
            selected_mailbox_name == "Archive" || (archive_mailbox.is_some() && selected_mailbox == archive_mailbox.map(|m| &m.id)),
        ),
        mailbox_row(
            "Trash",
            Icon::Delete,
            trash_mailbox.map(|m| &m.id),
            0,
            selected_mailbox_name == "Trash" || (trash_mailbox.is_some() && selected_mailbox == trash_mailbox.map(|m| &m.id)),
        ),
    ]
    .spacing(crate::theme::SPACE_SM)
    .padding(8);

    let custom_mailboxes: Vec<&MailboxSummary> = mailboxes
        .iter()
        .filter(|m| matches!(m.role, MailboxRole::Custom))
        .collect();

    if !custom_mailboxes.is_empty() {
        list = list.push(crate::components::surface::divider());
        list = list.push(crate::components::list::section_label("FOLDERS"));
        for mailbox in custom_mailboxes {
            let selected = selected_mailbox == Some(&mailbox.id);
            list = list.push(mailbox_row(
                &mailbox.name,
                Icon::Folder,
                Some(&mailbox.id),
                mailbox.unread_count,
                selected,
            ));
        }
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


