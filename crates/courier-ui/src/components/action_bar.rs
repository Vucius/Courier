use iced::widget::{button, text};
use iced::{Element, Length};

use crate::app::Message;
use crate::components::icon::Icon;

pub fn button_primary<'a>(label: &'a str, message: Message) -> Element<'a, Message> {
    button(text(label).size(13))
        .height(Length::Fixed(30.0))
        .padding(8)
        .style(button::primary)
        .on_press(message)
        .into()
}

pub fn button_primary_with_icon<'a>(
    label: &'a str,
    icon: Icon,
    message: Message,
) -> Element<'a, Message> {
    button(
        iced::widget::row![
            icon.view_styled(14.0, iced::Color::WHITE),
            text(label).size(13)
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center)
    )
    .height(Length::Fixed(30.0))
    .padding([6, 10])
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

pub fn button_toolbar_with_icon<'a>(
    label: &'a str,
    icon: Icon,
    icon_color: iced::Color,
    message: Message,
) -> Element<'a, Message> {
    button(
        iced::widget::row![
            icon.view_styled(14.0, icon_color),
            text(label).size(13)
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center)
    )
    .height(Length::Fixed(30.0))
    .padding([6, 10])
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

pub fn button_text_with_icon<'a>(
    label: &'a str,
    icon: Icon,
    icon_color: iced::Color,
    message: Message,
) -> Element<'a, Message> {
    button(
        iced::widget::row![
            icon.view_styled(14.0, icon_color),
            text(label).size(13)
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center)
    )
    .height(Length::Fixed(30.0))
    .padding([6, 10])
    .style(button::text)
    .on_press(message)
    .into()
}

