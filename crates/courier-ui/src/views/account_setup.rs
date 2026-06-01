use iced::widget::{column, container, row, text};
use iced::{Element, Length};

use crate::app::Message;

pub fn view<'a>(
    email: &'a str,
    imap_host: &'a str,
    imap_port: &'a str,
    smtp_host: &'a str,
    smtp_port: &'a str,
) -> Element<'a, Message> {
    container(
        column![
            crate::components::surface::header(
                "Account Setup",
                crate::components::action_bar::button_primary("Save", Message::SaveAccount),
            ),
            crate::components::surface::divider(),
            row![
                crate::components::badge::role("IMAP"),
                text("Generic IMAP/SMTP").size(13).color(crate::theme::TEXT),
            ]
            .spacing(8)
            .padding([10, 12]),
            crate::components::form::labeled_input(
                "Email",
                "name@example.com",
                email,
                Message::AccountEmailChanged,
            ),
            crate::components::form::labeled_input(
                "IMAP",
                "imap.example.com",
                imap_host,
                Message::AccountImapHostChanged,
            ),
            crate::components::form::labeled_input(
                "Port",
                "993",
                imap_port,
                Message::AccountImapPortChanged,
            ),
            crate::components::form::labeled_input(
                "SMTP",
                "smtp.example.com",
                smtp_host,
                Message::AccountSmtpHostChanged,
            ),
            crate::components::form::labeled_input(
                "Port",
                "587",
                smtp_port,
                Message::AccountSmtpPortChanged,
            ),
        ]
        .spacing(0),
    )
    .height(Length::Fill)
    .into()
}
