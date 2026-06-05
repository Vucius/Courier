use iced::widget::{column, container, text};
use iced::{Alignment, Background, Border, Element, Length};

use crate::app::Message;

pub fn view<'a>(title: &'a str, detail: &'a str) -> Element<'a, Message> {
    container(
        column![
            container(text("@").size(24).color(crate::theme::ACCENT))
                .width(Length::Fixed(48.0))
                .height(Length::Fixed(48.0))
                .center_x(Length::Fixed(48.0))
                .center_y(Length::Fixed(48.0))
                .style(|_| container::Style {
                    background: Some(Background::Color(crate::theme::ROW_SELECTED)),
                    border: Border {
                        width: 1.0,
                        radius: 24.0.into(),
                        color: crate::theme::ACCENT_MUTED,
                    },
                    ..container::Style::default()
                }),
            text(title).size(16).color(crate::theme::TEXT),
            text(detail).size(13).color(crate::theme::TEXT_MUTED),
        ]
        .align_x(Alignment::Center)
        .spacing(crate::theme::SPACE_SM),
    )
    .center(Length::Fill)
    .into()
}
