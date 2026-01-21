pub mod layout;
pub mod lua;
pub mod node;
pub mod platform;
pub mod renderer;
pub mod signal;

use std::rc::Rc;

// Re-export key types
pub use lua::register_ui_module;
pub use signal::SignalRuntime;

/// Shared reference to SignalRuntime (interior mutability handled internally)
pub type SharedSignalRuntime = Rc<signal::SignalRuntime>;
