use iced::widget::{column, container, text};
use iced::{Alignment, Element, Length};

use crate::app::Message;

pub fn view<'a>(title: &'a str, detail: &'a str) -> Element<'a, Message> {
    container(
        column![
            text(title).size(16).color(crate::theme::TEXT),
            text(detail).size(13).color(crate::theme::TEXT_MUTED),
        ]
        .align_x(Alignment::Center)
        .spacing(6),
    )
    .center(Length::Fill)
    .into()
}
