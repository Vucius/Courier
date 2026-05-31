use iced::widget::{Container, container, horizontal_rule, row, text};
use iced::{Alignment, Background, Border, Element, Length, Shadow, border};

use crate::app::Message;

pub fn app_background<'a>(content: impl Into<Element<'a, Message>>) -> Container<'a, Message> {
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(crate::theme::BACKGROUND)),
            ..container::Style::default()
        })
}

pub fn pane<'a>(content: impl Into<Element<'a, Message>>) -> Container<'a, Message> {
    container(content)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(crate::theme::SURFACE)),
            border: Border {
                width: 1.0,
                radius: 0.0.into(),
                color: crate::theme::BORDER,
            },
            shadow: Shadow::default(),
            text_color: Some(crate::theme::TEXT),
        })
}

pub fn toolbar_surface<'a>(content: impl Into<Element<'a, Message>>) -> Container<'a, Message> {
    container(content)
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(crate::theme::SURFACE_ALT)),
            border: Border {
                width: 1.0,
                radius: 0.0.into(),
                color: crate::theme::BORDER,
            },
            ..container::Style::default()
        })
}

pub fn row_surface<'a>(
    content: impl Into<Element<'a, Message>>,
    selected: bool,
) -> Container<'a, Message> {
    let background = if selected {
        crate::theme::ROW_SELECTED
    } else {
        crate::theme::SURFACE
    };

    container(content)
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(background)),
            border: Border {
                width: 1.0,
                radius: 0.0.into(),
                color: crate::theme::BORDER,
            },
            ..container::Style::default()
        })
}

pub fn section_title<'a>(label: &'a str) -> Element<'a, Message> {
    text(label)
        .size(12)
        .color(crate::theme::TEXT_MUTED)
        .width(Length::Fill)
        .into()
}

pub fn badge(count: u32) -> Element<'static, Message> {
    container(text(count.to_string()).size(11).color(crate::theme::ACCENT))
        .padding(4)
        .style(|_| container::Style {
            background: Some(Background::Color(crate::theme::ROW_SELECTED)),
            border: border::rounded(6),
            ..container::Style::default()
        })
        .into()
}

pub fn header<'a>(
    title: &'a str,
    trailing: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    row![
        text(title).size(16).color(crate::theme::TEXT),
        iced::widget::horizontal_space(),
        trailing.into()
    ]
    .align_y(Alignment::Center)
    .spacing(8)
    .padding(8)
    .into()
}

pub fn divider<'a>() -> Element<'a, Message> {
    horizontal_rule(1)
        .style(|_| iced::widget::rule::Style {
            color: crate::theme::BORDER,
            width: 1,
            radius: 0.0.into(),
            fill_mode: iced::widget::rule::FillMode::Full,
        })
        .into()
}
