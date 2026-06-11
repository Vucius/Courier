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
        NoticeKind::Warning => "WARN",
        NoticeKind::Error => "ERR",
        NoticeKind::Info | NoticeKind::Success => "",
    };

    let mut status_row = row![].spacing(8).align_y(Alignment::Center);
    if !category_prefix.is_empty() {
        status_row = status_row.push(text(category_prefix).size(11).color(accent));
    }
    status_row = status_row.push(text(status).size(11).color(crate::theme::TEXT_MUTED));
    status_row = status_row.push(iced::widget::horizontal_space());
    status_row = status_row.push(text(hint).size(11).color(crate::theme::TEXT_MUTED));

    container(status_row)
        .padding([4, 8])
        .style(move |_| container::Style {
            background: Some(Background::Color(crate::theme::SURFACE_ALT)),
            border: Border {
                width: 0.0,
                radius: crate::theme::RADIUS_MD.into(),
                color: iced::Color::TRANSPARENT,
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
