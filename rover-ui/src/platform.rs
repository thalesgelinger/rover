use mlua::Function;
use std::cell::RefCell;
use std::rc::Rc;

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
    warning_handler: Rc<RefCell<Option<Function>>>,
}

impl UiRuntimeConfig {
    pub fn new(target: UiTarget) -> Self {
        Self {
            target,
            warning_handler: Rc::new(RefCell::new(None)),
        }
    }

    pub fn target(&self) -> UiTarget {
        self.target
    }

    pub fn set_warning_handler(&self, handler: Option<Function>) {
        *self.warning_handler.borrow_mut() = handler;
    }

    pub fn emit_warning(&self, message: &str) {
        eprintln!("[rover-ui] {}", message);

        let handler = self.warning_handler.borrow().clone();
        if let Some(handler) = handler {
            if let Err(err) = handler.call::<()>(message.to_string()) {
                eprintln!("[rover-ui] warning handler error: {}", err);
            }
        }
    }
}
