use iced::widget::{button, row, text};
use iced::{Alignment, Element, Length};

use crate::app::Message;

pub fn toolbar<'a>(
    left: impl Into<Element<'a, Message>>,
    right: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    row![left.into(), iced::widget::horizontal_space(), right.into()]
        .height(Length::Fixed(crate::theme::TOOLBAR_HEIGHT))
        .align_y(Alignment::Center)
        .padding(8)
        .spacing(8)
        .into()
}

pub fn button_primary<'a>(label: &'a str, message: Message) -> Element<'a, Message> {
    button(text(label).size(13))
        .height(Length::Fixed(30.0))
        .padding(8)
        .style(button::primary)
        .on_press(message)
        .into()
}

pub fn button_toolbar<'a>(label: &'a str, message: Message) -> Element<'a, Message> {
    button(text(label).size(13))
        .height(Length::Fixed(30.0))
        .padding(8)
        .style(button::secondary)
        .on_press(message)
        .into()
}

pub fn button_text<'a>(label: &'a str, message: Message) -> Element<'a, Message> {
    button(text(label).size(13))
        .height(Length::Fixed(30.0))
        .padding(8)
        .style(button::text)
        .on_press(message)
        .into()
}
