use std::ffi::{c_char, c_void};

pub type AppleViewHandle = *mut c_void;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppleViewKind {
    Window = 0,
    View = 1,
    Column = 2,
    Row = 3,
    Text = 4,
    Button = 5,
    Input = 6,
    Checkbox = 7,
    Image = 8,
    ScrollView = 9,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AppleStyle {
    pub flags: u32,
    pub bg_rgba: u32,
    pub border_rgba: u32,
    pub text_rgba: u32,
    pub border_width: u16,
}

impl AppleStyle {
    pub const HAS_BG: u32 = 1 << 0;
    pub const HAS_BORDER: u32 = 1 << 1;
    pub const HAS_TEXT: u32 = 1 << 2;
    pub const HAS_BORDER_WIDTH: u32 = 1 << 3;

    pub fn with_bg(mut self, rgba: u32) -> Self {
        self.flags |= Self::HAS_BG;
        self.bg_rgba = rgba;
        self
    }

    pub fn with_border(mut self, rgba: u32) -> Self {
        self.flags |= Self::HAS_BORDER;
        self.border_rgba = rgba;
        self
    }

    pub fn with_text(mut self, rgba: u32) -> Self {
        self.flags |= Self::HAS_TEXT;
        self.text_rgba = rgba;
        self
    }

    pub fn with_border_width(mut self, width: u16) -> Self {
        self.flags |= Self::HAS_BORDER_WIDTH;
        self.border_width = width;
        self
    }

    pub fn from_node_style(style: &rover_ui::ui::NodeStyle) -> Self {
        let mut apple_style = Self::default();

        for op in &style.ops {
            match op {
                rover_ui::ui::StyleOp::BgColor(value) => {
                    if let Some(rgba) = parse_hex_rgba(value) {
                        apple_style = apple_style.with_bg(rgba);
                    }
                }
                rover_ui::ui::StyleOp::BorderColor(value) => {
                    if let Some(rgba) = parse_hex_rgba(value) {
                        apple_style = apple_style.with_border(rgba);
                    }
                }
                rover_ui::ui::StyleOp::BorderWidth(value) => {
                    apple_style = apple_style.with_border_width(*value);
                }
                rover_ui::ui::StyleOp::Padding(_) => {}
            }
        }

        if let Some(value) = style.color.as_deref().and_then(parse_hex_rgba) {
            apple_style = apple_style.with_text(value);
        }

        apple_style
    }
}

pub type CreateViewFn = extern "C" fn(node_id: u32, kind: AppleViewKind) -> AppleViewHandle;
pub type AppendChildFn = extern "C" fn(parent: AppleViewHandle, child: AppleViewHandle);
pub type RemoveViewFn = extern "C" fn(view: AppleViewHandle);
pub type SetFrameFn = extern "C" fn(view: AppleViewHandle, x: f32, y: f32, width: f32, height: f32);
pub type SetTextFn = extern "C" fn(view: AppleViewHandle, ptr: *const c_char, len: usize);
pub type SetBoolFn = extern "C" fn(view: AppleViewHandle, value: bool);
pub type SetStyleFn = extern "C" fn(
    view: AppleViewHandle,
    flags: u32,
    bg_rgba: u32,
    border_rgba: u32,
    text_rgba: u32,
    border_width: u16,
);
pub type SetWindowFn =
    extern "C" fn(view: AppleViewHandle, title: *const c_char, len: usize, width: f32, height: f32);
pub type StopAppFn = extern "C" fn();

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct AppleHostCallbacks {
    pub create_view: Option<CreateViewFn>,
    pub append_child: Option<AppendChildFn>,
    pub remove_view: Option<RemoveViewFn>,
    pub set_frame: Option<SetFrameFn>,
    pub set_text: Option<SetTextFn>,
    pub set_bool: Option<SetBoolFn>,
    pub set_style: Option<SetStyleFn>,
    pub set_window: Option<SetWindowFn>,
    pub stop_app: Option<StopAppFn>,
}

fn parse_hex_rgba(raw: &str) -> Option<u32> {
    let hex = raw.strip_prefix('#')?;
    if hex.len() != 6 && hex.len() != 8 {
        return None;
    }

    let value = u32::from_str_radix(hex, 16).ok()?;
    if hex.len() == 6 {
        Some((value << 8) | 0xff)
    } else {
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rover_ui::ui::{NodeStyle, StyleOp};

    #[test]
    fn parses_hex_colors_to_rgba() {
        let style = NodeStyle {
            ops: vec![StyleOp::BgColor("#112233".to_string())],
            color: Some("#aabbcc".to_string()),
            ..Default::default()
        };

        let apple_style = AppleStyle::from_node_style(&style);

        assert_eq!(apple_style.bg_rgba, 0x112233ff);
        assert_eq!(apple_style.text_rgba, 0xaabbccff);
        assert_eq!(apple_style.flags & AppleStyle::HAS_BG, AppleStyle::HAS_BG);
        assert_eq!(
            apple_style.flags & AppleStyle::HAS_TEXT,
            AppleStyle::HAS_TEXT
        );
    }
}
