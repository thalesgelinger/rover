use skia_safe::Color;

#[derive(Debug, Clone)]
pub struct Palette {
    pub primary: Color,
    pub primary_foreground: Color,
    pub secondary: Color,
    pub secondary_foreground: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub background: Color,
    pub foreground: Color,
    pub muted: Color,
    pub muted_foreground: Color,
    pub border: Color,
    pub input: Color,
    pub ring: Color,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            primary: Color::from_rgb(59, 130, 246),
            primary_foreground: Color::WHITE,
            secondary: Color::from_rgb(100, 116, 139),
            secondary_foreground: Color::WHITE,
            success: Color::from_rgb(34, 197, 94),
            warning: Color::from_rgb(251, 146, 60),
            error: Color::from_rgb(239, 68, 68),
            background: Color::WHITE,
            foreground: Color::from_rgb(15, 23, 42),
            muted: Color::from_rgb(241, 245, 249),
            muted_foreground: Color::from_rgb(100, 116, 139),
            border: Color::from_rgb(226, 232, 240),
            input: Color::from_rgb(226, 232, 240),
            ring: Color::from_rgb(59, 130, 246),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Radii {
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
    pub full: f32,
}

impl Default for Radii {
    fn default() -> Self {
        Self {
            sm: 4.0,
            md: 6.0,
            lg: 8.0,
            full: 9999.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Spacing {
    pub xs: f32,
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
    pub xl: f32,
}

impl Default for Spacing {
    fn default() -> Self {
        Self {
            xs: 4.0,
            sm: 8.0,
            md: 16.0,
            lg: 24.0,
            xl: 32.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Typography {
    pub xs: f32,
    pub sm: f32,
    pub base: f32,
    pub lg: f32,
    pub xl: f32,
}

impl Default for Typography {
    fn default() -> Self {
        Self {
            xs: 12.0,
            sm: 14.0,
            base: 16.0,
            lg: 18.0,
            xl: 20.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub palette: Palette,
    pub radii: Radii,
    pub spacing: Spacing,
    pub typography: Typography,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            palette: Palette::default(),
            radii: Radii::default(),
            spacing: Spacing::default(),
            typography: Typography::default(),
        }
    }
}

impl Theme {
    pub fn resolve_color(&self, name: &str) -> Color {
        match name {
            "primary" => self.palette.primary,
            "primary_foreground" => self.palette.primary_foreground,
            "secondary" => self.palette.secondary,
            "secondary_foreground" => self.palette.secondary_foreground,
            "success" => self.palette.success,
            "warning" => self.palette.warning,
            "error" => self.palette.error,
            "background" => self.palette.background,
            "foreground" => self.palette.foreground,
            "muted" => self.palette.muted,
            "muted_foreground" => self.palette.muted_foreground,
            "border" => self.palette.border,
            "input" => self.palette.input,
            "ring" => self.palette.ring,
            _ => Color::BLACK,
        }
    }
}
