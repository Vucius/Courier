use iced::widget::{container, text};
use iced::{Background, Border, Element, Length};

use crate::app::Message;

pub fn view(name: &str, selected: bool) -> Element<'_, Message> {
    let color = if selected {
        crate::theme::ACCENT
    } else {
        avatar_color(name)
    };
    let text_color = crate::theme::SURFACE;

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

fn avatar_color(value: &str) -> iced::Color {
    const PALETTE: [iced::Color; 6] = [
        iced::Color::from_rgb(0.32, 0.52, 0.86),
        iced::Color::from_rgb(0.12, 0.55, 0.42),
        iced::Color::from_rgb(0.69, 0.42, 0.18),
        iced::Color::from_rgb(0.53, 0.38, 0.73),
        iced::Color::from_rgb(0.72, 0.31, 0.36),
        iced::Color::from_rgb(0.20, 0.48, 0.58),
    ];
    let hash = value.bytes().fold(0usize, |acc, byte| {
        acc.wrapping_mul(31).wrapping_add(byte as usize)
    });
    PALETTE[hash % PALETTE.len()]
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
