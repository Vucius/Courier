use iced::widget::{container, text};
use iced::{Background, Border, Element, Length};

use crate::app::Message;

pub fn view(name: &str, selected: bool) -> Element<'_, Message> {
    let color = if selected {
        crate::theme::ACCENT
    } else {
        crate::theme::AVATAR
    };
    let text_color = if selected {
        crate::theme::SURFACE
    } else {
        crate::theme::TEXT
    };

    container(text(initials(name)).size(12).color(text_color))
        .width(Length::Fixed(32.0))
        .height(Length::Fixed(32.0))
        .center_x(Length::Fixed(32.0))
        .center_y(Length::Fixed(32.0))
        .style(move |_| container::Style {
            background: Some(Background::Color(color)),
            border: Border {
                width: 1.0,
                radius: 16.0.into(),
                color: crate::theme::BORDER,
            },
            ..container::Style::default()
        })
        .into()
}

fn initials(value: &str) -> String {
    let mut letters = value
        .split(|ch: char| ch.is_whitespace() || matches!(ch, '<' | '@' | '.' | '-' | '_'))
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.chars().next())
        .take(2)
        .collect::<String>()
        .to_ascii_uppercase();

    if letters.is_empty() {
        letters.push('?');
    }

    letters
}
