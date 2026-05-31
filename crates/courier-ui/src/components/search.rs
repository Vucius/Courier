use iced::widget::{container, row, text, text_input};
use iced::{Alignment, Background, Border, Element, Length};

use crate::app::Message;

pub fn view<'a>(query: &'a str) -> Element<'a, Message> {
    container(
        row![
            text("Search").size(12).color(crate::theme::TEXT_MUTED),
            text_input("Search mail", query)
                .on_input(Message::SearchChanged)
                .size(13)
                .padding(6)
                .width(Length::Fill),
        ]
        .align_y(Alignment::Center)
        .spacing(8),
    )
    .padding(6)
    .style(|_| container::Style {
        background: Some(Background::Color(crate::theme::SURFACE)),
        border: Border {
            width: 1.0,
            radius: 6.0.into(),
            color: crate::theme::BORDER,
        },
        ..container::Style::default()
    })
    .width(Length::Fill)
    .into()
}
