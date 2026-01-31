use crate::renderer::TuiRenderer;
use rover_ui::app::App;
use rover_ui::events::UiEvent;
use rover_ui::ui::{NodeId, UiNode, UiRegistry};
use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};

/// Runs an `App<TuiRenderer>` with a combined timer + terminal-input event loop.
///
/// Manages focus tracking for Input nodes and routes keyboard events
/// to the focused input's buffer, dispatching Change/Submit events.
pub struct TuiRunner {
    app: App<TuiRenderer>,
    /// Currently focused Input node.
    focused: Option<NodeId>,
    /// Keyboard input buffer for the focused input.
    input_buffer: String,
    /// Whether we have any Input nodes (avoids 'q' quitting when typing).
    has_inputs: bool,
}

impl TuiRunner {
    pub fn new(app: App<TuiRenderer>) -> Self {
        Self {
            app,
            focused: None,
            input_buffer: String::new(),
            has_inputs: false,
        }
    }

    pub fn app(&self) -> &App<TuiRenderer> {
        &self.app
    }

    pub fn app_mut(&mut self) -> &mut App<TuiRenderer> {
        &mut self.app
    }

    /// Run the TUI event loop. Blocks until quit or no pending work.
    pub fn run(&mut self) -> Result<(), RunError> {
        self.app.mount().map_err(RunError::Lua)?;

        // Scan for Input nodes and auto-focus the first one
        self.scan_inputs();
        self.update_cursor();

        while self.app.is_running() {
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
                .unwrap_or(Duration::from_millis(16));

            if event::poll(timeout).map_err(RunError::Io)? {
                match event::read().map_err(RunError::Io)? {
                    Event::Key(key_event) => {
                        if self.handle_key(key_event)? {
                            break;
                        }
                    }
                    Event::Resize(_cols, _rows) => {
                        self.app.renderer().refresh_size();
                    }
                    _ => {}
                }
            }

            self.app.tick().map_err(RunError::Lua)?;
            self.update_cursor();

            if !self.app.scheduler().borrow().has_pending() && self.focused.is_none() {
                break;
            }
        }

        Ok(())
    }

    /// Handle a key event. Returns `true` if the app should exit.
    fn handle_key(&mut self, key: KeyEvent) -> Result<bool, RunError> {
        // Ctrl+C always exits
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.app.stop();
            return Ok(true);
        }

        // If we have a focused input, route keystrokes to it
        if let Some(node_id) = self.focused {
            match key.code {
                KeyCode::Char(c) => {
                    self.input_buffer.push(c);
                    self.app.push_event(UiEvent::Change {
                        node_id,
                        value: self.input_buffer.clone(),
                    });
                }
                KeyCode::Backspace => {
                    self.input_buffer.pop();
                    self.app.push_event(UiEvent::Change {
                        node_id,
                        value: self.input_buffer.clone(),
                    });
                }
                KeyCode::Enter => {
                    self.app.push_event(UiEvent::Submit {
                        node_id,
                        value: self.input_buffer.clone(),
                    });
                    self.input_buffer.clear();
                    self.app.push_event(UiEvent::Change {
                        node_id,
                        value: self.input_buffer.clone(),
                    });
                }
                KeyCode::Esc => {
                    self.app.stop();
                    return Ok(true);
                }
                _ => {}
            }
        } else {
            // No focused input — q or Esc exits
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    self.app.stop();
                    return Ok(true);
                }
                _ => {}
            }
        }

        Ok(false)
    }

    /// Scan the tree for Input nodes. Auto-focus the first one found.
    fn scan_inputs(&mut self) {
        let registry = self.app.registry();
        let reg = registry.borrow();
        if let Some(root) = reg.root() {
            let mut inputs = Vec::new();
            collect_input_nodes(&reg, root, &mut inputs);
            self.has_inputs = !inputs.is_empty();
            if let Some(&first) = inputs.first() {
                self.focused = Some(first);
                // Initialize input_buffer from the input's current value
                if let Some(UiNode::Input { value, .. }) = reg.get_node(first) {
                    self.input_buffer = value.value().to_string();
                }
            }
        }
    }

    /// Position the terminal cursor at the focused input's edit position.
    fn update_cursor(&mut self) {
        if let Some(node_id) = self.focused {
            let col_offset = self.input_buffer.len() as u16;
            let _ = self.app.renderer().show_cursor_at(node_id, col_offset);
        }
    }
}

/// Recursively collect all Input node IDs from the tree.
fn collect_input_nodes(registry: &UiRegistry, node_id: NodeId, out: &mut Vec<NodeId>) {
    let node = match registry.get_node(node_id) {
        Some(n) => n,
        None => return,
    };

    match node {
        UiNode::Input { .. } => {
            out.push(node_id);
        }
        UiNode::Column { children }
        | UiNode::Row { children }
        | UiNode::View { children }
        | UiNode::List { children, .. } => {
            let children = children.clone();
            for child_id in children {
                collect_input_nodes(registry, child_id, out);
            }
        }
        UiNode::Conditional { child, .. } => {
            if let Some(child_id) = child {
                collect_input_nodes(registry, *child_id, out);
            }
        }
        _ => {}
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
    use rover_ui::ui::TextContent;

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
        assert!(runner.focused.is_none());
        assert!(runner.input_buffer.is_empty());
    }

    #[test]
    fn test_collect_input_nodes_empty_tree() {
        let registry = UiRegistry::new();
        let mut inputs = Vec::new();
        // No root → no crash
        if let Some(root) = registry.root() {
            collect_input_nodes(&registry, root, &mut inputs);
        }
        assert!(inputs.is_empty());
    }

    #[test]
    fn test_collect_input_nodes_finds_inputs() {
        let mut registry = UiRegistry::new();
        let text = registry.create_node(UiNode::Text {
            content: TextContent::Static("hello".into()),
        });
        let input = registry.create_node(UiNode::Input {
            value: TextContent::Static("".into()),
            on_change: None,
            on_submit: None,
        });
        let col = registry.create_node(UiNode::Column {
            children: vec![text, input],
        });
        registry.set_root(col);

        let mut inputs = Vec::new();
        collect_input_nodes(&registry, col, &mut inputs);
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0], input);
    }

    #[test]
    fn test_collect_input_nodes_nested() {
        let mut registry = UiRegistry::new();
        let input1 = registry.create_node(UiNode::Input {
            value: TextContent::Static("".into()),
            on_change: None,
            on_submit: None,
        });
        let input2 = registry.create_node(UiNode::Input {
            value: TextContent::Static("".into()),
            on_change: None,
            on_submit: None,
        });
        let row = registry.create_node(UiNode::Row {
            children: vec![input1],
        });
        let col = registry.create_node(UiNode::Column {
            children: vec![row, input2],
        });
        registry.set_root(col);

        let mut inputs = Vec::new();
        collect_input_nodes(&registry, col, &mut inputs);
        assert_eq!(inputs.len(), 2);
        assert_eq!(inputs[0], input1);
        assert_eq!(inputs[1], input2);
    }

    #[test]
    fn test_no_inputs_means_no_focus() {
        let mut registry = UiRegistry::new();
        let text = registry.create_node(UiNode::Text {
            content: TextContent::Static("no inputs here".into()),
        });
        registry.set_root(text);

        let mut inputs = Vec::new();
        collect_input_nodes(&registry, text, &mut inputs);
        assert!(inputs.is_empty());
    }
}
