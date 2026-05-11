#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NativeStyle {
    pub flags: u32,
    pub bg_rgba: u32,
    pub border_rgba: u32,
    pub text_rgba: u32,
    pub border_width: u16,
}

impl NativeStyle {
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
        let mut native_style = Self::default();

        for op in &style.ops {
            match op {
                rover_ui::ui::StyleOp::BgColor(value) => {
                    if let Some(rgba) = parse_hex_rgba(value) {
                        native_style = native_style.with_bg(rgba);
                    }
                }
                rover_ui::ui::StyleOp::BorderColor(value) => {
                    if let Some(rgba) = parse_hex_rgba(value) {
                        native_style = native_style.with_border(rgba);
                    }
                }
                rover_ui::ui::StyleOp::BorderWidth(value) => {
                    native_style = native_style.with_border_width(*value);
                }
                rover_ui::ui::StyleOp::Padding(_) => {}
            }
        }

        if let Some(value) = style.color.as_deref().and_then(parse_hex_rgba) {
            native_style = native_style.with_text(value);
        }

        native_style
    }
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

        let native_style = NativeStyle::from_node_style(&style);

        assert_eq!(native_style.bg_rgba, 0x112233ff);
        assert_eq!(native_style.text_rgba, 0xaabbccff);
        assert_eq!(
            native_style.flags & NativeStyle::HAS_BG,
            NativeStyle::HAS_BG
        );
        assert_eq!(
            native_style.flags & NativeStyle::HAS_TEXT,
            NativeStyle::HAS_TEXT
        );
    }
}
