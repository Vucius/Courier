use courier_proto::ThreadSummary;
use iced::Element;
use iced::widget::{column, text};

use crate::app::Message;

pub fn view(threads: &[ThreadSummary]) -> Element<'_, Message> {
    let mut list = column![text("Threads").size(16)].spacing(8);

    if threads.is_empty() {
        return list.push(text("No messages yet").size(14)).into();
    }

    for thread in threads {
        list = list.push(column![
            text(&thread.subject).size(14),
            text(&thread.snippet).size(12)
        ]);
    }

    list.into()
}
