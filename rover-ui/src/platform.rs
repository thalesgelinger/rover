use crate::signal::SignalId;
use std::collections::HashSet;

/// Execution target for the active renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiTarget {
    Tui,
    Web,
    Macos,
    Ios,
    Mobile,
    Unknown,
}

impl UiTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            UiTarget::Tui => "tui",
            UiTarget::Web => "web",
            UiTarget::Macos => "macos",
            UiTarget::Ios => "ios",
            UiTarget::Mobile => "mobile",
            UiTarget::Unknown => "unknown",
        }
    }
}

/// Runtime capability gates checked at module/runtime boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiCapability {
    TuiNamespace,
    MacosNamespace,
}

impl UiCapability {
    pub fn as_str(self) -> &'static str {
        match self {
            UiCapability::TuiNamespace => "tui_namespace",
            UiCapability::MacosNamespace => "macos_namespace",
        }
    }

    fn allowed_by_default(self, target: UiTarget) -> bool {
        match self {
            UiCapability::TuiNamespace => target == UiTarget::Tui,
            UiCapability::MacosNamespace => target == UiTarget::Macos,
        }
    }
}

/// Runtime config shared with Lua through app_data.
#[derive(Clone)]
pub struct UiRuntimeConfig {
    target: UiTarget,
    allow: HashSet<UiCapability>,
    deny: HashSet<UiCapability>,
}

impl UiRuntimeConfig {
    pub fn new(target: UiTarget) -> Self {
        Self {
            target,
            allow: HashSet::new(),
            deny: HashSet::new(),
        }
    }

    pub fn target(&self) -> UiTarget {
        self.target
    }

    pub fn allow_capability(mut self, capability: UiCapability) -> Self {
        self.allow.insert(capability);
        self
    }

    pub fn deny_capability(mut self, capability: UiCapability) -> Self {
        self.deny.insert(capability);
        self
    }

    pub fn is_capability_allowed(&self, capability: UiCapability) -> bool {
        if self.deny.contains(&capability) {
            return false;
        }
        if self.allow.contains(&capability) {
            return true;
        }
        capability.allowed_by_default(self.target)
    }
}

pub const DEFAULT_VIEWPORT_WIDTH: u16 = 80;
pub const DEFAULT_VIEWPORT_HEIGHT: u16 = 24;

#[derive(Debug, Clone, Copy)]
pub struct ViewportSignals {
    pub width: SignalId,
    pub height: SignalId,
}
