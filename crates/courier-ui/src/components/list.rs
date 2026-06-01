use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Background, Border, Element, Length};

use crate::app::Message;

pub fn section_label<'a>(label: &'a str) -> Element<'a, Message> {
    text(label)
        .size(11)
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
        text(label).size(14).color(crate::theme::TEXT)
    ]
    .align_y(Alignment::Center)
    .spacing(8)
    .width(Length::Fill);

    if let Some(trailing) = trailing {
        content = content
            .push(iced::widget::horizontal_space())
            .push(trailing);
    }

    button(row_frame(content, selected))
        .style(button::text)
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
        .style(button::text)
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
    let background = if selected {
        crate::theme::ROW_SELECTED
    } else {
        crate::theme::SURFACE
    };

    container(content)
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(background)),
            border: Border {
                width: 1.0,
                radius: 0.0.into(),
                color: crate::theme::BORDER,
            },
            ..container::Style::default()
        })
        .into()
}
