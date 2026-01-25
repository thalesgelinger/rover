pub mod app;
pub mod coroutine;
pub mod events;
pub mod lua;
pub mod scheduler;
pub mod signal;
pub mod task;
pub mod ui;

use std::rc::Rc;

// Re-export key types
pub use lua::register_ui_module;
pub use scheduler::SharedScheduler;

/// Shared reference to SignalRuntime (interior mutability handled internally)
pub type SharedSignalRuntime = Rc<signal::SignalRuntime>;

// For convenience, also re-export the inner type
pub use signal::SignalRuntime;
