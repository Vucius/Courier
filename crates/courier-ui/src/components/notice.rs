use iced::widget::{container, row, text};
use iced::{Alignment, Background, Border, Element, Length};

use crate::app::Message;

#[derive(Debug, Clone, Copy)]
pub enum NoticeKind {
    Info,
    Warning,
    Success,
    Error,
}

pub fn inline<'a>(kind: NoticeKind, message: impl Into<String>) -> Element<'a, Message> {
    let message = message.into();
    let (accent, label) = match kind {
        NoticeKind::Info => (crate::theme::ACCENT, "Info"),
        NoticeKind::Warning => (crate::theme::WARNING, "Warning"),
        NoticeKind::Success => (crate::theme::SUCCESS, "Done"),
        NoticeKind::Error => (crate::theme::DANGER, "Error"),
    };

    container(
        row![
            text(label).size(11).color(accent),
            text(message).size(12).color(crate::theme::TEXT_MUTED),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([7, 10])
    .width(Length::Fill)
    .style(move |_| container::Style {
        background: Some(Background::Color(crate::theme::SURFACE_ALT)),
        border: Border {
            width: 1.0,
            radius: 6.0.into(),
            color: accent,
        },
        ..container::Style::default()
    })
    .into()
}
