use iced::widget::{container, text};
use iced::{Background, Border, Element, Length};

use crate::app::Message;

pub fn count(count: u32) -> Element<'static, Message> {
    container(text(count.to_string()).size(11).color(crate::theme::ACCENT))
        .padding([2, 6])
        .height(Length::Fixed(22.0))
        .style(|_| container::Style {
            background: Some(Background::Color(crate::theme::ROW_SELECTED)),
            border: Border {
                width: 1.0,
                radius: 8.0.into(),
                color: crate::theme::ACCENT_MUTED,
            },
            ..container::Style::default()
        })
        .into()
}

#[allow(dead_code)]
pub fn pill<'a>(label: &'a str) -> Element<'a, Message> {
    container(text(label).size(11).color(crate::theme::TEXT_MUTED))
        .padding([3, 8])
        .height(Length::Fixed(24.0))
        .style(|_| container::Style {
            background: Some(Background::Color(crate::theme::SURFACE_ALT)),
            border: Border {
                width: 1.0,
                radius: 8.0.into(),
                color: crate::theme::BORDER,
            },
            ..container::Style::default()
        })
        .into()
}

pub fn role<'a>(label: &'a str) -> Element<'a, Message> {
    container(text(label).size(10).color(crate::theme::TEXT_MUTED))
        .width(Length::Fixed(28.0))
        .height(Length::Fixed(22.0))
        .center_x(Length::Fixed(28.0))
        .center_y(Length::Fixed(22.0))
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
