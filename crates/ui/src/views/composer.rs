use iced::Element;
use iced::widget::text;

use crate::app::Message;

pub fn view() -> Element<'static, Message> {
    text("Composer placeholder").into()
}
