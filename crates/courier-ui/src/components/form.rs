use iced::widget::{column, row, text, text_input};
use iced::{Alignment, Element, Length};

use crate::app::Message;

pub fn labeled_input<'a>(
    label: &'a str,
    placeholder: &'a str,
    value: &'a str,
    on_input: fn(String) -> Message,
) -> Element<'a, Message> {
    row![
        text(label)
            .size(12)
            .color(crate::theme::TEXT_MUTED)
            .width(Length::Fixed(58.0)),
        text_input(placeholder, value)
            .on_input(on_input)
            .size(13)
            .padding(7)
            .width(Length::Fill),
    ]
    .spacing(10)
    .padding([8, 10])
    .align_y(Alignment::Center)
    .into()
}

pub fn body_input<'a>(
    placeholder: &'a str,
    value: &'a str,
    on_input: fn(String) -> Message,
) -> Element<'a, Message> {
    column![
        text("Body").size(12).color(crate::theme::TEXT_MUTED),
        text_input(placeholder, value)
            .on_input(on_input)
            .size(13)
            .padding(8)
            .width(Length::Fill),
    ]
    .spacing(6)
    .padding([8, 10])
    .width(Length::Fill)
    .into()
}
