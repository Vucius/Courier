use courier_proto::{ThreadId, ThreadSummary};
use iced::Element;
use iced::font::Weight;
use iced::widget::{column, container, mouse_area, row, scrollable, text};
use iced::{Alignment, Background, Border, Font, Length};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::app::Message;

pub fn view<'a>(
    threads: &[&'a ThreadSummary],
    selected_thread: Option<&ThreadId>,
    title: &'a str,
) -> Element<'a, Message> {
    let mut list = column![crate::components::surface::header(
        title,
        text(format!(
            "{} {}",
            threads.len(),
            if threads.len() == 1 { "message" } else { "messages" }
        ))
        .size(12)
        .color(crate::theme::TEXT_MUTED),
    )]
    .spacing(0)
    .height(Length::Fill);

    if threads.is_empty() {
        list = list.push(
            container(crate::components::empty_state::view(
                "No messages found",
                "Try syncing your inbox or changing filters.",
            ))
            .height(Length::Fill)
            .width(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill),
        );
    } else {
        let mut scroll_col = column![].spacing(0);
        for thread in threads {
            let selected = selected_thread == Some(&thread.id);
            scroll_col = scroll_col.push(thread_row(thread, selected));
            scroll_col = scroll_col.push(crate::components::surface::divider());
        }
        list = list.push(scrollable(scroll_col).height(Length::Fill));
    }

    list.into()
}

fn thread_row<'a>(thread: &'a ThreadSummary, selected: bool) -> Element<'a, Message> {
    let sender_font = if thread.unread {
        Font {
            weight: Weight::Bold,
            ..Font::DEFAULT
        }
    } else {
        Font::DEFAULT
    };
    let subject_font = if thread.unread {
        Font {
            weight: Weight::Bold,
            ..Font::DEFAULT
        }
    } else {
        Font::DEFAULT
    };
    let date_font = if thread.unread {
        Font {
            weight: Weight::Bold,
            ..Font::DEFAULT
        }
    } else {
        Font::DEFAULT
    };
    let subject_color = if thread.unread {
        crate::theme::TEXT
    } else {
        crate::theme::TEXT_MUTED
    };
    let date_color = if thread.unread {
        crate::theme::ACCENT
    } else {
        crate::theme::TEXT_MUTED
    };

    let content = column![
        row![
            text(&thread.sender)
                .size(13)
                .color(crate::theme::TEXT)
                .font(sender_font),
            text(format!("· {}", thread.account_id.0))
                .size(11)
                .color(crate::theme::TEXT_MUTED),
            iced::widget::horizontal_space(),
            text(timestamp_label(thread.last_message_ts))
                .size(crate::theme::FONT_CAPTION)
                .color(date_color)
                .font(date_font),
        ]
        .align_y(Alignment::Center)
        .spacing(crate::theme::SPACE_SM),
        text(&thread.subject)
            .size(if thread.unread { 15 } else { 14 })
            .color(subject_color)
            .font(subject_font),
        text(&thread.snippet)
            .size(12)
            .color(crate::theme::TEXT_SUBTLE),
    ]
    .spacing(4)
    .width(Length::Fill);

    let row = crate::components::list::message_row(
        crate::components::avatar::view(&thread.sender, selected),
        row![unread_dot(thread.unread), content]
            .spacing(crate::theme::SPACE_SM)
            .align_y(Alignment::Start),
        selected,
        Message::SelectThread(thread.id.clone()),
    );

    mouse_area(row)
        .on_right_press(Message::OpenThreadContext(thread.id.clone()))
        .into()
}

fn unread_dot(unread: bool) -> Element<'static, Message> {
    let color = if unread {
        crate::theme::ACCENT
    } else {
        iced::Color::TRANSPARENT
    };

    container(text(""))
        .width(Length::Fixed(7.0))
        .height(Length::Fixed(7.0))
        .style(move |_| container::Style {
            background: Some(Background::Color(color)),
            border: Border {
                width: 0.0,
                radius: crate::theme::RADIUS_SM.into(),
                color: iced::Color::TRANSPARENT,
            },
            ..container::Style::default()
        })
        .into()
}

fn timestamp_label(timestamp: i64) -> String {
    const SECS_PER_DAY: i64 = 86_400;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(timestamp);
    let timestamp_day = timestamp.div_euclid(SECS_PER_DAY);
    let now_day = now.div_euclid(SECS_PER_DAY);

    if timestamp_day == now_day {
        return format!("{:02}:{:02}", hour(timestamp), minute(timestamp));
    }

    if now_day - timestamp_day < 7 {
        return weekday_label(timestamp_day).to_string();
    }

    let (_, month, day) = date_from_unix_days(timestamp_day);
    format!("{month:02}/{day:02}")
}

fn hour(timestamp: i64) -> i64 {
    timestamp.rem_euclid(86_400) / 3_600
}

fn minute(timestamp: i64) -> i64 {
    timestamp.rem_euclid(3_600) / 60
}

fn weekday_label(day: i64) -> &'static str {
    const WEEKDAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    WEEKDAYS[day.rem_euclid(7) as usize]
}

fn date_from_unix_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if month <= 2 { 1 } else { 0 };
    (year, month, day)
}
