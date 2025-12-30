use tokio::sync::mpsc::Receiver;
use rover_types::Task;
use mlua::Lua;

/// Generic event loop that processes async tasks
/// This is the core async runtime for Rover, handling any type of task
/// (HTTP requests, timers, file I/O, etc.) in a unified way
pub struct EventLoop {
    lua: Lua,
    rx: Receiver<Box<dyn Task>>,
}

impl EventLoop {
    /// Create a new event loop
    pub fn new(lua: Lua, rx: Receiver<Box<dyn Task>>) -> Self {
        Self { lua, rx }
    }

    /// Spawn the event loop in a background task
    /// Returns when the channel is closed
    pub fn spawn(lua: Lua, rx: Receiver<Box<dyn Task>>) {
        tokio::spawn(async move {
            let event_loop = EventLoop::new(lua, rx);
            event_loop.run().await;
        });
    }

    /// Run the event loop, processing tasks until the channel closes
    async fn run(mut self) {
        while let Some(task) = self.rx.recv().await {
            // Execute the task
            if let Err(e) = task.execute() {
                tracing::error!("Task execution failed: {}", e);
            }
        }
    }
}
