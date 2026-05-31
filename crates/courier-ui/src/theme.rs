use iced::Color;

pub const APP_PADDING: u16 = 10;
pub const TOOLBAR_HEIGHT: f32 = 44.0;
pub const SIDEBAR_WIDTH: f32 = 224.0;
pub const THREAD_LIST_WIDTH: f32 = 336.0;

pub const BACKGROUND: Color = rgb(236, 238, 241);
pub const SURFACE: Color = rgb(250, 251, 252);
pub const SURFACE_ALT: Color = rgb(244, 246, 248);
pub const ROW_SELECTED: Color = rgb(218, 233, 252);
pub const BORDER: Color = rgb(210, 216, 224);
pub const TEXT: Color = rgb(31, 35, 41);
pub const TEXT_MUTED: Color = rgb(99, 108, 120);
pub const ACCENT: Color = rgb(37, 99, 235);

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    }
}
