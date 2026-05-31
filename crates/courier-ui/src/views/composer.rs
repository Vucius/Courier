use iced::Element;
use iced::Length;
use iced::widget::{column, container, row, text, text_input};

use crate::app::Message;

pub fn view<'a>(to: &'a str, subject: &'a str, body: &'a str) -> Element<'a, Message> {
    container(
        column![
            crate::components::surface::header(
                "Compose",
                crate::components::action_bar::button_primary("Send", Message::SendDraft),
            ),
            crate::components::surface::divider(),
            row![
                text("To").size(12).color(crate::theme::TEXT_MUTED),
                text_input("name@example.com", to)
                    .on_input(Message::DraftToChanged)
                    .size(13)
                    .padding(6)
                    .width(Length::Fill),
            ]
            .spacing(12)
            .padding(8),
            row![
                text("Subject").size(12).color(crate::theme::TEXT_MUTED),
                text_input("Subject", subject)
                    .on_input(Message::DraftSubjectChanged)
                    .size(13)
                    .padding(6)
                    .width(Length::Fill),
            ]
            .spacing(12)
            .padding(8),
            text_input("Write a reply or new message", body)
                .on_input(Message::DraftBodyChanged)
                .size(13)
                .padding(8)
                .width(Length::Fill),
        ]
        .spacing(0),
    )
    .height(Length::FillPortion(2))
    .into()
}
