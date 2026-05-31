use courier_proto::MailboxSummary;
use iced::Element;
use iced::widget::{column, text};

use crate::app::Message;

pub fn view(mailboxes: &[MailboxSummary]) -> Element<'_, Message> {
    let mut list = column![text("Accounts").size(16), text("Unified Inbox").size(14)].spacing(8);

    for mailbox in mailboxes {
        let label = if mailbox.unread_count == 0 {
            mailbox.name.clone()
        } else {
            format!("{} ({})", mailbox.name, mailbox.unread_count)
        };
        list = list.push(text(label).size(14));
    }

    list.into()
}
