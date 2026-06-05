use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Background, Border, Element, Length, Shadow};

use crate::app::Message;

pub fn section_label<'a>(label: &'a str) -> Element<'a, Message> {
    text(label)
        .size(crate::theme::FONT_CAPTION)
        .color(crate::theme::TEXT_MUTED)
        .width(Length::Fill)
        .into()
}

pub fn outline_row<'a>(
    leading: impl Into<Element<'a, Message>>,
    label: &'a str,
    trailing: Option<Element<'a, Message>>,
    selected: bool,
    on_press: Message,
) -> Element<'a, Message> {
    let mut content = row![
        leading.into(),
        text(label)
            .size(crate::theme::FONT_BODY)
            .color(crate::theme::TEXT)
    ]
    .align_y(Alignment::Center)
    .spacing(crate::theme::SPACE_SM)
    .width(Length::Fill);

    if let Some(trailing) = trailing {
        content = content
            .push(iced::widget::horizontal_space())
            .push(trailing);
    }

    button(row_frame(content, selected))
        .style(move |_, status| row_button_style(selected, status))
        .padding(0)
        .width(Length::Fill)
        .on_press(on_press)
        .into()
}

pub fn message_row<'a>(
    leading: impl Into<Element<'a, Message>>,
    content: impl Into<Element<'a, Message>>,
    selected: bool,
    on_press: Message,
) -> Element<'a, Message> {
    let row = row![leading.into(), content.into()]
        .align_y(Alignment::Start)
        .spacing(10)
        .padding(10)
        .width(Length::Fill);

    button(row_frame(row, selected))
        .style(move |_, status| row_button_style(selected, status))
        .padding(0)
        .width(Length::Fill)
        .on_press(on_press)
        .into()
}

pub fn metadata_rows<'a>(rows: Vec<(&'a str, String)>) -> Element<'a, Message> {
    let mut content = column![].spacing(6).padding(12);

    for (label, value) in rows {
        content = content.push(
            row![
                text(label)
                    .size(12)
                    .color(crate::theme::TEXT_MUTED)
                    .width(Length::Fixed(52.0)),
                text(value).size(13).color(crate::theme::TEXT),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
    }

    content.into()
}

pub fn row_frame<'a>(
    content: impl Into<Element<'a, Message>>,
    selected: bool,
) -> Element<'a, Message> {
    let accent = if selected {
        crate::theme::ACCENT
    } else {
        iced::Color::TRANSPARENT
    };

    container(
        row![
            container(text(""))
                .width(Length::Fixed(3.0))
                .height(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(Background::Color(accent)),
                    border: Border {
                        width: 0.0,
                        radius: crate::theme::RADIUS_SM.into(),
                        color: iced::Color::TRANSPARENT,
                    },
                    ..container::Style::default()
                }),
            content.into(),
        ]
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Shrink)
    .style(move |_| container::Style {
        border: Border {
            width: 0.0,
            radius: crate::theme::RADIUS_MD.into(),
            color: iced::Color::TRANSPARENT,
        },
        ..container::Style::default()
    })
    .into()
}

fn row_button_style(selected: bool, status: button::Status) -> button::Style {
    let background = match (selected, status) {
        (true, button::Status::Pressed) => crate::theme::ACCENT_MUTED,
        (true, _) => crate::theme::ROW_SELECTED,
        (false, button::Status::Hovered) => crate::theme::SURFACE_HOVER,
        (false, button::Status::Pressed) => crate::theme::ROW_SELECTED,
        (false, _) => crate::theme::SURFACE,
    };

    button::Style {
        background: Some(Background::Color(background)),
        text_color: crate::theme::TEXT,
        border: Border {
            width: 0.0,
            radius: crate::theme::RADIUS_MD.into(),
            color: iced::Color::TRANSPARENT,
        },
        shadow: Shadow::default(),
    }
}
