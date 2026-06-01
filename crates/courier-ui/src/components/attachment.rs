use iced::widget::{container, row, text};
use iced::{Alignment, Background, Border, Element, Length};

use crate::app::Message;

pub fn chip<'a>(name: impl Into<String>, detail: impl Into<String>) -> Element<'a, Message> {
    container(
        row![
            crate::components::badge::role("FILE"),
            text(name.into()).size(13).color(crate::theme::TEXT),
            text(detail.into()).size(12).color(crate::theme::TEXT_MUTED),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([7, 9])
    .width(Length::Shrink)
    .style(|_| container::Style {
        background: Some(Background::Color(crate::theme::SURFACE_ALT)),
        border: Border {
            width: 1.0,
            radius: 6.0.into(),
            color: crate::theme::BORDER,
        },
        ..container::Style::default()
    })
    .into()
}

pub fn image_placeholder<'a>(label: &'a str) -> Element<'a, Message> {
    container(
        row![
            crate::components::badge::role("IMG"),
            text(label).size(12).color(crate::theme::TEXT_MUTED),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([7, 9])
    .style(|_| container::Style {
        background: Some(Background::Color(crate::theme::SURFACE_ALT)),
        border: Border {
            width: 1.0,
            radius: 6.0.into(),
            color: crate::theme::BORDER,
        },
        ..container::Style::default()
    })
    .into()
}
