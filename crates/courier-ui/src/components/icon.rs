#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Compose,
    Settings,
    AccountManage,
    AccountAdd,
    Sync,
    Inbox,
    Send,
    Drafts,
    Archive,
    Delete,
    Search,
    Filter,
    Reply,
    Forward,
    More,
    Error,
    Warning,
    CheckCircle,
    Star,
    Folder,
    FolderOpen,
    Bell,
    BellOff,
    Wifi,
    WifiOff,
    Paperclip,
}

#[allow(dead_code)]
impl Icon {
    pub fn handle(self) -> iced::widget::svg::Handle {
        let bytes: &'static [u8] = match self {
            Icon::Compose => include_bytes!("../../icons/compose.svg"),
            Icon::Settings => include_bytes!("../../icons/settings.svg"),
            Icon::AccountManage => include_bytes!("../../icons/account_manage.svg"),
            Icon::AccountAdd => include_bytes!("../../icons/account_add.svg"),
            Icon::Sync => include_bytes!("../../icons/sync.svg"),
            Icon::Inbox => include_bytes!("../../icons/inbox.svg"),
            Icon::Send => include_bytes!("../../icons/send.svg"),
            Icon::Drafts => include_bytes!("../../icons/drafts.svg"),
            Icon::Archive => include_bytes!("../../icons/archive.svg"),
            Icon::Delete => include_bytes!("../../icons/delete.svg"),
            Icon::Search => include_bytes!("../../icons/search.svg"),
            Icon::Filter => include_bytes!("../../icons/filter.svg"),
            Icon::Reply => include_bytes!("../../icons/reply.svg"),
            Icon::Forward => include_bytes!("../../icons/forward.svg"),
            Icon::More => include_bytes!("../../icons/more.svg"),
            Icon::Error => include_bytes!("../../icons/error.svg"),
            Icon::Warning => include_bytes!("../../icons/warning.svg"),
            Icon::CheckCircle => include_bytes!("../../icons/check_circle.svg"),
            Icon::Star => include_bytes!("../../icons/star.svg"),
            Icon::Folder => include_bytes!("../../icons/folder.svg"),
            Icon::FolderOpen => include_bytes!("../../icons/folder_open.svg"),
            Icon::Bell => include_bytes!("../../icons/bell.svg"),
            Icon::BellOff => include_bytes!("../../icons/bell_off.svg"),
            Icon::Wifi => include_bytes!("../../icons/wifi.svg"),
            Icon::WifiOff => include_bytes!("../../icons/wifi_off.svg"),
            Icon::Paperclip => include_bytes!("../../icons/paperclip.svg"),
        };
        iced::widget::svg::Handle::from_memory(bytes)
    }

    pub fn view<'a>(self) -> iced::widget::svg::Svg<'a, iced::Theme> {
        self.view_sized(16.0)
    }

    pub fn view_sized<'a>(self, size: f32) -> iced::widget::svg::Svg<'a, iced::Theme> {
        iced::widget::svg(self.handle())
            .width(iced::Length::Fixed(size))
            .height(iced::Length::Fixed(size))
    }

    pub fn view_styled<'a>(self, size: f32, color: iced::Color) -> iced::widget::svg::Svg<'a, iced::Theme> {
        iced::widget::svg(self.handle())
            .width(iced::Length::Fixed(size))
            .height(iced::Length::Fixed(size))
            .style(move |_, _| iced::widget::svg::Style {
                color: Some(color),
            })
    }
}
