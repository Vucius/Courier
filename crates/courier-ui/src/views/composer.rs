use iced::Element;
use iced::Length;
use iced::widget::{column, container};

use crate::app::Message;

pub fn view<'a>(to: &'a str, subject: &'a str, body: &'a str) -> Element<'a, Message> {
    container(
        column![
            crate::components::surface::header(
                "Compose",
                crate::components::action_bar::button_primary("Send", Message::SendDraft),
            ),
            crate::components::surface::divider(),
            crate::components::form::labeled_input(
                "To",
                "name@example.com",
                to,
                Message::DraftToChanged,
            ),
            crate::components::form::labeled_input(
                "Subject",
                "Subject",
                subject,
                Message::DraftSubjectChanged,
            ),
            crate::components::form::body_input(
                "Write a reply or new message",
                body,
                Message::DraftBodyChanged,
            ),
        ]
        .spacing(0),
    )
    .height(Length::FillPortion(2))
    .into()
}
