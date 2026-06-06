use iced::Color;

pub const APP_PADDING: u16 = 10;
pub const SIDEBAR_WIDTH: f32 = 260.0;
pub const THREAD_LIST_WIDTH: f32 = 336.0;

pub const SPACE_XS: u16 = 4;
pub const SPACE_SM: u16 = 8;
pub const FONT_CAPTION: u16 = 11;
pub const FONT_BODY: u16 = 14;
pub const FONT_TITLE: u16 = 16;
pub const RADIUS_SM: f32 = 4.0;
pub const RADIUS_MD: f32 = 6.0;
pub const RADIUS_LG: f32 = 8.0;

pub const BACKGROUND: Color = rgb(236, 238, 241);
pub const SURFACE: Color = rgb(250, 251, 252);
pub const SURFACE_ALT: Color = rgb(244, 246, 248);
pub const SURFACE_HOVER: Color = rgb(239, 244, 251);
pub const ROW_SELECTED: Color = rgb(218, 233, 252);
pub const BORDER: Color = rgb(210, 216, 224);
pub const TEXT: Color = rgb(31, 35, 41);
pub const TEXT_MUTED: Color = rgb(99, 108, 120);
pub const TEXT_SUBTLE: Color = rgb(126, 136, 149);
pub const ACCENT: Color = rgb(37, 99, 235);
pub const ACCENT_MUTED: Color = rgb(159, 190, 246);
pub const SUCCESS: Color = rgb(22, 125, 74);
pub const WARNING: Color = rgb(171, 104, 10);
pub const DANGER: Color = rgb(191, 45, 45);

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    }
}
