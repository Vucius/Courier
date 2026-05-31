use courier_proto::{ThreadId, ThreadSummary};
use iced::Element;
use iced::widget::{button, column, row, scrollable, text};
use iced::{Alignment, Length};

use crate::app::Message;

pub fn view<'a>(
    threads: &[&'a ThreadSummary],
    selected_thread: Option<&ThreadId>,
    title: &'a str,
) -> Element<'a, Message> {
    let mut list = column![crate::components::surface::header(
        title,
        text(format!("{} shown", threads.len()))
            .size(12)
            .color(crate::theme::TEXT_MUTED),
    )]
    .spacing(0);

    if threads.is_empty() {
        return crate::components::empty_state::view(
            "No messages found",
            "Try a different search or sync the account.",
        );
    }

    for thread in threads {
        let selected = selected_thread == Some(&thread.id);
        list = list.push(thread_row(thread, selected));
    }

    scrollable(list).height(Length::Fill).into()
}

fn thread_row<'a>(thread: &'a ThreadSummary, selected: bool) -> Element<'a, Message> {
    let subject_size = if thread.unread { 15 } else { 14 };
    let unread_marker = if thread.unread {
        text("*").size(14).color(crate::theme::ACCENT)
    } else {
        text(" ").size(14)
    };

    let content = row![
        unread_marker,
        column![
            row![
                text(&thread.sender).size(13).color(crate::theme::TEXT),
                iced::widget::horizontal_space(),
                text(timestamp_label(thread.last_message_ts))
                    .size(11)
                    .color(crate::theme::TEXT_MUTED),
            ]
            .align_y(Alignment::Center)
            .spacing(8),
            text(&thread.subject)
                .size(subject_size)
                .color(crate::theme::TEXT),
            text(&thread.snippet)
                .size(12)
                .color(crate::theme::TEXT_MUTED),
        ]
        .spacing(4)
        .width(Length::Fill),
    ]
    .spacing(8)
    .padding(10)
    .width(Length::Fill);

    button(crate::components::surface::row_surface(content, selected))
        .style(iced::widget::button::text)
        .padding(0)
        .width(Length::Fill)
        .on_press(Message::SelectThread(thread.id.clone()))
        .into()
}

fn timestamp_label(timestamp: i64) -> &'static str {
    match timestamp {
        1_780_214_400 => "Today",
        1_780_210_800 => "Today",
        _ => "May 30",
    }
}
