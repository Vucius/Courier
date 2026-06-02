use courier_proto::{AccountId, AccountState, AuthType, IdentitySummary, ProviderKind};
use iced::widget::{column, container, row, text};
use iced::{Alignment, Element, Length};

use crate::app::Message;

pub struct AccountSetupViewState<'a> {
    pub accounts: &'a [AccountState],
    pub identities: &'a [IdentitySummary],
    pub editing_account_id: Option<&'a AccountId>,
    pub email: &'a str,
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
    let title = if state.editing_account_id.is_some() {
        "Edit Account"
    } else {
        "Account Setup"
    };

    container(
        column![
            crate::components::surface::header(
                title,
                row![
                    crate::components::action_bar::button_toolbar("New", Message::AddAccount),
                    crate::components::action_bar::button_toolbar(
                        "Test",
                        Message::TestAccountConnection,
                    ),
                    crate::components::action_bar::button_primary("Save", Message::SaveAccount),
                ]
                .spacing(6),
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
                state.email,
                Message::AccountEmailChanged,
            ),
            crate::components::form::labeled_input(
                "IMAP",
                "imap.example.com",
                state.imap_host,
                Message::AccountImapHostChanged,
            ),
            crate::components::form::labeled_input(
                "Port",
                "993",
                state.imap_port,
                Message::AccountImapPortChanged,
            ),
            crate::components::form::labeled_input(
                "SMTP",
                "smtp.example.com",
                state.smtp_host,
                Message::AccountSmtpHostChanged,
            ),
            crate::components::form::labeled_input(
                "Port",
                "587",
                state.smtp_port,
                Message::AccountSmtpPortChanged,
            ),
            crate::components::form::labeled_input(
                "Password",
                "stored in OS keyring",
                state.password,
                Message::AccountPasswordChanged,
            ),
            connection_status_view(state.connection_status),
            crate::components::surface::divider(),
            identities_view(
                state.identities,
                state.editing_account_id,
                state.identity_name,
                state.identity_email,
            ),
            crate::components::surface::divider(),
            accounts_view(state.accounts, state.editing_account_id),
        ]
        .spacing(0),
    )
    .height(Length::Fill)
    .into()
}

fn connection_status_view<'a>(connection_status: &'a str) -> Element<'a, Message> {
    if connection_status.trim().is_empty() {
        return column![].into();
    }

    let kind = if connection_status.contains("failed") {
        crate::components::notice::NoticeKind::Error
    } else if connection_status.contains("reachable") {
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
                text("Edit an account to manage its sending identities.")
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

fn accounts_view<'a>(
    accounts: &'a [AccountState],
    editing_account_id: Option<&'a AccountId>,
) -> Element<'a, Message> {
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
        content = content.push(account_row(
            account,
            editing_account_id == Some(&account.id),
        ));
    }

    content.into()
}

fn account_row<'a>(account: &'a AccountState, editing: bool) -> Element<'a, Message> {
    let status = if account.enabled {
        if editing {
            "Enabled - Editing"
        } else {
            "Enabled"
        }
    } else if editing {
        "Disabled - Editing"
    } else {
        "Disabled"
    };
    let toggle_label = if account.enabled { "Disable" } else { "Enable" };
    let toggle_value = !account.enabled;

    let mut actions = row![
        crate::components::badge::role(provider_code(&account.provider)),
        column![
            text(&account.email).size(14).color(crate::theme::TEXT),
            text(status).size(11).color(crate::theme::TEXT_MUTED),
        ]
        .spacing(2)
        .width(Length::Fill),
        crate::components::action_bar::button_text(
            "Edit",
            Message::EditAccount(account.id.clone()),
        ),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .width(Length::Fill);

    if matches!(account.auth_type, AuthType::OAuth2) {
        actions = actions.push(crate::components::action_bar::button_text(
            "OAuth2",
            Message::BeginOAuth2(account.id.clone()),
        ));
    }

    actions
        .push(crate::components::action_bar::button_text(
            toggle_label,
            Message::ToggleAccountEnabled(account.id.clone(), toggle_value),
        ))
        .push(crate::components::action_bar::button_text(
            "Delete",
            Message::DeleteAccount(account.id.clone()),
        ))
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
