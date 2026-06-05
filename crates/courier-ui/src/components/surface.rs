use iced::widget::{Container, container, horizontal_rule, row, text};
use iced::{Alignment, Background, Border, Element, Length, Shadow, Vector};

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
                radius: crate::theme::RADIUS_LG.into(),
                color: crate::theme::BORDER,
            },
            shadow: Shadow {
                color: iced::Color {
                    a: 0.08,
                    ..iced::Color::BLACK
                },
                offset: Vector::new(0.0, 1.0),
                blur_radius: 4.0,
            },
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
                radius: crate::theme::RADIUS_LG.into(),
                color: crate::theme::BORDER,
            },
            ..container::Style::default()
        })
}

pub fn header<'a>(
    title: &'a str,
    trailing: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    row![
        text(title)
            .size(crate::theme::FONT_TITLE)
            .color(crate::theme::TEXT),
        iced::widget::horizontal_space(),
        trailing.into()
    ]
    .align_y(Alignment::Center)
    .spacing(crate::theme::SPACE_SM)
    .padding(crate::theme::SPACE_SM)
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
