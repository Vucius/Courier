use courier_proto::MessageBody;
use iced::Element;
use iced::widget::{column, text};

use crate::app::Message;

pub fn view(body: Option<&MessageBody>) -> Element<'_, Message> {
    match body {
        Some(body) => column![
            text(&body.subject).size(20),
            text(format!("From: {}", body.from)).size(13),
            text(&body.body).size(14),
        ]
        .spacing(10)
        .into(),
        None => column![text("Reader").size(16), text("Select a message").size(14)]
            .spacing(8)
            .into(),
    }
}
