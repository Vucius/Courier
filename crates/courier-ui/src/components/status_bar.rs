use iced::Element;
use iced::widget::text;

use crate::app::Message;

pub fn view(status: &str) -> Element<'_, Message> {
    text(status).size(12).into()
}
