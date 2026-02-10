/// Execution target for the active renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiTarget {
    Tui,
    Web,
    Mobile,
    Unknown,
}

impl UiTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            UiTarget::Tui => "tui",
            UiTarget::Web => "web",
            UiTarget::Mobile => "mobile",
            UiTarget::Unknown => "unknown",
        }
    }
}

/// Runtime config shared with Lua through app_data.
#[derive(Clone)]
pub struct UiRuntimeConfig {
    target: UiTarget,
}

impl UiRuntimeConfig {
    pub fn new(target: UiTarget) -> Self {
        Self { target }
    }

    pub fn target(&self) -> UiTarget {
        self.target
    }
}
