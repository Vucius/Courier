use courier_proto::{AccountId, AuthType, IdentitySummary};
use iced::widget::{button, checkbox, column, container, row, scrollable, text};
use iced::{Alignment, Element, Length};

use crate::app::Message;

pub struct AccountSetupViewState<'a> {
    pub identities: &'a [IdentitySummary],
    pub editing_account_id: Option<&'a AccountId>,
    pub editing_account_enabled: Option<bool>,
    pub email: &'a str,
    pub provider_label: &'a str,
    pub smart_config_active: bool,
    pub manual_config: bool,
    pub auth_type: AuthType,
    pub imap_host: &'a str,
    pub imap_port: &'a str,
    pub smtp_host: &'a str,
    pub smtp_port: &'a str,
    pub password: &'a str,
    pub identity_name: &'a str,
    pub identity_email: &'a str,
    pub connection_status: &'a str,
}

pub fn view<'a>(state: AccountSetupViewState<'a>) -> Element<'a, Message> {
    let mut content = column![
        provider_summary(&state),
        crate::components::form::labeled_input(
            "Email",
            "name@example.com",
            state.email,
            Message::AccountEmailChanged,
        ),
        manual_config_toggle(state.manual_config),
    ]
    .spacing(0);

    if !state.smart_config_active || state.manual_config {
        content = content
            .push(crate::components::form::labeled_input(
                "IMAP",
                "imap.example.com",
                state.imap_host,
                Message::AccountImapHostChanged,
            ))
            .push(crate::components::form::labeled_input(
                "Port",
                "993",
                state.imap_port,
                Message::AccountImapPortChanged,
            ))
            .push(crate::components::form::labeled_input(
                "SMTP",
                "smtp.example.com",
                state.smtp_host,
                Message::AccountSmtpHostChanged,
            ))
            .push(crate::components::form::labeled_input(
                "Port",
                "587",
                state.smtp_port,
                Message::AccountSmtpPortChanged,
            ));
    } else {
        content = content.push(automatic_config_notice(&state));
    }

    if matches!(&state.auth_type, AuthType::Password) {
        content = content.push(crate::components::form::labeled_input(
            "Password",
            "Enter or replace the mailbox password",
            state.password,
            Message::AccountPasswordChanged,
        ));
    } else {
        content = content.push(crate::components::notice::inline(
            crate::components::notice::NoticeKind::Info,
            "This provider uses secure OAuth2 sign-in. Save the account, then choose OAuth2 from the account card if authorization is required.",
        ));
    }

    content = content
        .push(connection_status_view(state.connection_status))
        .push(crate::components::surface::divider())
        .push(identities_view(
            state.identities,
            state.editing_account_id,
            state.identity_name,
            state.identity_email,
        ));

    let body = scrollable(content.spacing(0))
        .height(Length::Fill)
        .width(Length::Fill);

    container(
        column![
            body,
            crate::components::surface::divider(),
            footer_actions(&state)
        ]
        .spacing(0),
    )
    .width(Length::Fixed(560.0))
    .height(Length::Fill)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(crate::theme::SURFACE)),
        border: iced::Border {
            width: 1.0,
            radius: crate::theme::RADIUS_LG.into(),
            color: crate::theme::BORDER,
        },
        shadow: iced::Shadow {
            color: iced::Color {
                a: 0.16,
                ..iced::Color::BLACK
            },
            offset: iced::Vector::new(0.0, 8.0),
            blur_radius: 24.0,
        },
        text_color: Some(crate::theme::TEXT),
    })
    .into()
}

fn footer_actions<'a>(state: &AccountSetupViewState<'a>) -> Element<'a, Message> {
    let mut destructive_actions = row![].spacing(8).align_y(Alignment::Center);
    if let Some(account_id) = state.editing_account_id {
        if let Some(enabled) = state.editing_account_enabled {
            destructive_actions =
                destructive_actions.push(crate::components::action_bar::button_text(
                    if enabled { "Disable" } else { "Enable" },
                    Message::ToggleAccountEnabled(account_id.clone(), !enabled),
                ));
        }
        if matches!(&state.auth_type, AuthType::OAuth2) {
            destructive_actions =
                destructive_actions.push(crate::components::action_bar::button_text(
                    "OAuth2",
                    Message::BeginOAuth2(account_id.clone()),
                ));
        }
        destructive_actions = destructive_actions.push(destructive_text_button(
            "Delete",
            Message::DeleteAccount(account_id.clone()),
        ));
    }

    row![
        crate::components::action_bar::button_text("Cancel", Message::CancelActivePanel),
        destructive_actions,
        iced::widget::horizontal_space(),
        crate::components::action_bar::button_toolbar("Test", Message::TestAccountConnection),
        crate::components::action_bar::button_primary("Save", Message::SaveAccount),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .padding([10, 12])
    .into()
}

fn destructive_text_button<'a>(label: &'a str, message: Message) -> Element<'a, Message> {
    button(text(label).size(13).color(crate::theme::DANGER))
        .height(Length::Fixed(30.0))
        .padding(8)
        .style(button::text)
        .on_press(message)
        .into()
}

fn provider_summary<'a>(state: &AccountSetupViewState<'a>) -> Element<'a, Message> {
    row![
        crate::components::badge::role(if state.smart_config_active {
            "AUTO"
        } else {
            "IMAP"
        }),
        column![
            text(state.provider_label)
                .size(13)
                .color(crate::theme::TEXT),
            text(if state.smart_config_active {
                "Courier will use the official provider settings by default."
            } else {
                "Manual IMAP/SMTP configuration is required for this domain."
            })
            .size(11)
            .color(crate::theme::TEXT_MUTED),
        ]
        .spacing(2)
        .width(Length::Fill),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .padding([10, 12])
    .into()
}

fn manual_config_toggle<'a>(manual_config: bool) -> Element<'a, Message> {
    container(
        checkbox("Show advanced server settings", manual_config)
            .on_toggle(Message::AccountManualConfigToggled)
            .size(13),
    )
    .padding([6, 12])
    .width(Length::Fill)
    .into()
}

fn automatic_config_notice<'a>(state: &AccountSetupViewState<'a>) -> Element<'a, Message> {
    let message = format!(
        "Using {} automatically: IMAP {}:{}, SMTP {}:{}.",
        state.provider_label, state.imap_host, state.imap_port, state.smtp_host, state.smtp_port
    );
    crate::components::notice::inline(crate::components::notice::NoticeKind::Info, message)
}

fn connection_status_view<'a>(connection_status: &'a str) -> Element<'a, Message> {
    if connection_status.trim().is_empty() {
        return column![].into();
    }

    let lower = connection_status.to_ascii_lowercase();
    let kind = if lower.contains("incorrect")
        || lower.contains("unable")
        || lower.contains("failed")
        || lower.contains("cannot")
    {
        crate::components::notice::NoticeKind::Error
    } else if lower.contains("verified") || lower.contains("ready") {
        crate::components::notice::NoticeKind::Success
    } else {
        crate::components::notice::NoticeKind::Info
    };

    crate::components::notice::inline(kind, connection_status)
}

fn identities_view<'a>(
    identities: &'a [IdentitySummary],
    editing_account_id: Option<&'a AccountId>,
    identity_name: &'a str,
    identity_email: &'a str,
) -> Element<'a, Message> {
    let mut content = column![
        crate::components::surface::header(
            "Sending Identities",
            crate::components::action_bar::button_primary("Add", Message::SaveIdentity),
        ),
        crate::components::form::labeled_input(
            "Name",
            "Display name",
            identity_name,
            Message::IdentityNameChanged,
        ),
        crate::components::form::labeled_input(
            "Email",
            "alias@example.com",
            identity_email,
            Message::IdentityEmailChanged,
        ),
    ]
    .spacing(0);

    let Some(account_id) = editing_account_id else {
        return content
            .push(
                text("Save the account before adding sending identities.")
                    .size(13)
                    .color(crate::theme::TEXT_MUTED),
            )
            .padding([8, 10])
            .into();
    };

    let mut found = false;
    for identity in identities
        .iter()
        .filter(|identity| identity.account_id == *account_id)
    {
        found = true;
        content = content.push(identity_row(identity));
    }

    if !found {
        content = content.push(
            text("No identities for this account")
                .size(13)
                .color(crate::theme::TEXT_MUTED)
                .width(Length::Fill),
        );
    }

    content.padding([8, 10]).into()
}

fn identity_row<'a>(identity: &'a IdentitySummary) -> Element<'a, Message> {
    row![
        crate::components::badge::role("ID"),
        column![
            text(&identity.name).size(14).color(crate::theme::TEXT),
            text(&identity.email)
                .size(11)
                .color(crate::theme::TEXT_MUTED),
        ]
        .spacing(2)
        .width(Length::Fill),
        crate::components::action_bar::button_text(
            "Delete",
            Message::DeleteIdentity(identity.id.clone()),
        ),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .width(Length::Fill)
    .into()
}
