use crate::renderer::TuiRenderer;
use rover_ui::app::App;
use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};

/// Runs an `App<TuiRenderer>` with a combined timer + terminal-input event loop.
///
/// This replaces `App::run()` with a loop that polls for both terminal key events
/// and scheduled timer wake-ups, whichever comes first.
pub struct TuiRunner {
    app: App<TuiRenderer>,
}

impl TuiRunner {
    pub fn new(app: App<TuiRenderer>) -> Self {
        Self { app }
    }

    /// Access the inner App (for loading scripts, registering modules, etc.)
    pub fn app(&self) -> &App<TuiRenderer> {
        &self.app
    }

    /// Mutable access to the inner App.
    pub fn app_mut(&mut self) -> &mut App<TuiRenderer> {
        &mut self.app
    }

    /// Run the TUI event loop. Blocks until quit or no pending work.
    ///
    /// Polls for terminal input between ticks. `Ctrl+C` or `q` exits cleanly.
    pub fn run(&mut self) -> Result<(), RunError> {
        // Mount the UI
        self.app.mount().map_err(RunError::Lua)?;

        while self.app.is_running() {
            // Determine how long to wait for the next tick
            let timeout = self
                .app
                .next_wake_time()
                .map(|wake| {
                    let now = std::time::Instant::now();
                    if wake > now {
                        wake.saturating_duration_since(now)
                    } else {
                        Duration::ZERO
                    }
                })
                .unwrap_or(Duration::from_millis(16)); // ~60 FPS fallback

            // Poll for terminal events within the timeout window
            if event::poll(timeout).map_err(RunError::Io)? {
                match event::read().map_err(RunError::Io)? {
                    Event::Key(KeyEvent {
                        code: KeyCode::Char('c'),
                        modifiers: KeyModifiers::CONTROL,
                        ..
                    }) => {
                        self.app.stop();
                        break;
                    }
                    Event::Key(KeyEvent {
                        code: KeyCode::Char('q'),
                        modifiers: KeyModifiers::NONE,
                        ..
                    }) => {
                        self.app.stop();
                        break;
                    }
                    Event::Resize(_cols, _rows) => {
                        self.app.renderer().refresh_size();
                        // TODO: re-layout on resize
                    }
                    _ => {
                        // Future: map key/mouse events to UiEvent
                    }
                }
            }

            // Run one tick of the app loop (timers, effects, rendering)
            self.app.tick().map_err(RunError::Lua)?;

            // Exit if nothing pending
            if !self.app.scheduler().borrow().has_pending() {
                break;
            }
        }

        Ok(())
    }
}

/// Errors that can occur during a TUI run.
#[derive(Debug)]
pub enum RunError {
    Io(io::Error),
    Lua(mlua::Error),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::Io(e) => write!(f, "IO error: {}", e),
            RunError::Lua(e) => write!(f, "Lua error: {}", e),
        }
    }
}

impl std::error::Error for RunError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RunError::Io(e) => Some(e),
            RunError::Lua(e) => Some(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_error_display() {
        let io_err = RunError::Io(io::Error::new(io::ErrorKind::Other, "test"));
        assert!(io_err.to_string().contains("IO error"));

        let lua_err = RunError::Lua(mlua::Error::RuntimeError("test".into()));
        assert!(lua_err.to_string().contains("Lua error"));
    }

    #[test]
    fn test_runner_creation() {
        let renderer = TuiRenderer::new().unwrap();
        let app = App::new(renderer).unwrap();
        let runner = TuiRunner::new(app);
        assert!(!runner.app().is_running());
    }
}
