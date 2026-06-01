use courier_proto::{AccountState, ProviderKind};
use iced::widget::{column, container, row, text};
use iced::{Alignment, Element, Length};

use crate::app::Message;

pub fn view<'a>(
    accounts: &'a [AccountState],
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
            crate::components::surface::divider(),
            accounts_view(accounts),
        ]
        .spacing(0),
    )
    .height(Length::Fill)
    .into()
}

fn accounts_view<'a>(accounts: &'a [AccountState]) -> Element<'a, Message> {
    let mut content = column![crate::components::list::section_label("ACCOUNTS")]
        .spacing(8)
        .padding([10, 12]);

    if accounts.is_empty() {
        content = content.push(
            text("No accounts configured")
                .size(13)
                .color(crate::theme::TEXT_MUTED),
        );
    }

    for account in accounts {
        content = content.push(account_row(account));
    }

    content.into()
}

fn account_row<'a>(account: &'a AccountState) -> Element<'a, Message> {
    let status = if account.enabled {
        "Enabled"
    } else {
        "Disabled"
    };
    let toggle_label = if account.enabled { "Disable" } else { "Enable" };
    let toggle_value = !account.enabled;

    row![
        crate::components::badge::role(provider_code(&account.provider)),
        column![
            text(&account.email).size(14).color(crate::theme::TEXT),
            text(status).size(11).color(crate::theme::TEXT_MUTED),
        ]
        .spacing(2)
        .width(Length::Fill),
        crate::components::action_bar::button_text(
            toggle_label,
            Message::ToggleAccountEnabled(account.id.clone(), toggle_value),
        ),
        crate::components::action_bar::button_text(
            "Delete",
            Message::DeleteAccount(account.id.clone()),
        ),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .width(Length::Fill)
    .into()
}

fn provider_code(provider: &ProviderKind) -> &'static str {
    match provider {
        ProviderKind::GenericImap => "IMAP",
        ProviderKind::Gmail => "GML",
        ProviderKind::Outlook => "OUT",
        ProviderKind::Jmap => "JMAP",
    }
}
