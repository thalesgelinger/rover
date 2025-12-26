/// Represents an async task that can be executed by the event loop
/// This trait allows different types of tasks (HTTP, timers, file I/O, etc.)
/// to be processed by the same event loop infrastructure
pub trait Task: Send + 'static {
    /// Execute the task and return a result
    /// This is called by the event loop in the context of the Lua VM
    fn execute(self: Box<Self>) -> anyhow::Result<()>;
}
