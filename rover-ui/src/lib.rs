pub mod lua;
pub mod signal;

use std::rc::Rc;

// Re-export key types
pub use lua::register_ui_module;

/// Shared reference to SignalRuntime (interior mutability handled internally)
pub type SharedSignalRuntime = Rc<signal::SignalRuntime>;

// For convenience, also re-export the inner type
pub use signal::SignalRuntime;
