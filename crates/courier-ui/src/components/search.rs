use iced::widget::{container, row, text_input};
use iced::{Alignment, Background, Border, Element, Length};

use crate::app::Message;
use crate::components::icon::Icon;

pub fn view<'a>(query: &'a str) -> Element<'a, Message> {
    use iced::widget::button;

    container(
        row![
            Icon::Search.view_styled(16.0, crate::theme::TEXT_MUTED),
            text_input("Search mail", query)
                .on_input(Message::SearchChanged)
                .size(13)
                .padding(6)
                .width(Length::Fill),
            button(
                row![
                    Icon::Filter.view_styled(14.0, crate::theme::TEXT_MUTED),
                    iced::widget::text("Filter").size(12).color(crate::theme::TEXT_MUTED),
                ]
                .spacing(4)
                .align_y(Alignment::Center)
            )
            .padding(4)
            .style(button::text)
            .on_press(Message::ProbeNetwork),
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

