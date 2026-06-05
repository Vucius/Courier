use iced::widget::{button, text};
use iced::{Element, Length};

use crate::app::Message;

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
