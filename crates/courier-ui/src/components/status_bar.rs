use iced::Element;
use iced::widget::{container, row, text};
use iced::{Alignment, Background, Border};

use crate::app::Message;
use crate::components::notice::NoticeKind;

pub fn view<'a>(status: &'a str, hint: &'a str) -> Element<'a, Message> {
    let kind = status_kind(status);
    let accent = match kind {
        NoticeKind::Info => crate::theme::ACCENT,
        NoticeKind::Warning => crate::theme::WARNING,
        NoticeKind::Success => crate::theme::SUCCESS,
        NoticeKind::Error => crate::theme::DANGER,
    };

    let category_prefix = match kind {
        NoticeKind::Info => "INFO",
        NoticeKind::Warning => "WARN",
        NoticeKind::Success => "OK",
        NoticeKind::Error => "ERR",
    };

    container(
        row![
            text(category_prefix).size(11).color(accent),
            text(status).size(12).color(crate::theme::TEXT_MUTED),
            iced::widget::horizontal_space(),
            text(hint).size(11).color(crate::theme::TEXT_MUTED),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([5, 8])
    .style(move |_| container::Style {
        background: Some(Background::Color(crate::theme::SURFACE)),
        border: Border {
            width: 1.0,
            radius: 6.0.into(),
            color: accent,
        },
        ..container::Style::default()
    })
    .into()
}

fn status_kind(status: &str) -> NoticeKind {
    let value = status.to_ascii_lowercase();

    if value.contains("fail") || value.contains("error") {
        NoticeKind::Error
    } else if value.contains("offline") || value.contains("paused") {
        NoticeKind::Warning
    } else if value.contains("sent")
        || value.contains("ready")
        || value.contains("loaded")
        || value.contains("updated")
    {
        NoticeKind::Success
    } else {
        NoticeKind::Info
    }
}
