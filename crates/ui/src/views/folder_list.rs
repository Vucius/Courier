use iced::Element;
use iced::widget::{column, text};
use mailproto::FolderSummary;

use crate::app::Message;

pub fn view(folders: &[FolderSummary]) -> Element<'_, Message> {
    let mut list = column![text("Folders").size(16)].spacing(8);

    for folder in folders {
        let label = if folder.unread_count == 0 {
            folder.name.clone()
        } else {
            format!("{} ({})", folder.name, folder.unread_count)
        };
        list = list.push(text(label).size(14));
    }

    list.into()
}
