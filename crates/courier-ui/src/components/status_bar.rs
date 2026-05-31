use iced::Element;
use iced::widget::{container, text};

use crate::app::Message;

pub fn view(status: &str) -> Element<'_, Message> {
    container(text(status).size(12).color(crate::theme::TEXT_MUTED))
        .padding(6)
        .into()
}
