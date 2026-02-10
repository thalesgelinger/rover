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
    /// Ordered list of focusable nodes (Input + KeyArea).
    focusable_nodes: Vec<NodeId>,
    /// Current focus index inside `focusable_nodes`.
    focus_index: Option<usize>,
    /// Keyboard input buffer for the focused input.
    input_buffer: String,
}

impl TuiRunner {
    pub fn new(app: App<TuiRenderer>) -> Self {
        Self {
            app,
            focusable_nodes: Vec::new(),
            focus_index: None,
            input_buffer: String::new(),
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

        // Scan focusable nodes and auto-focus the first one
        self.scan_focusables();
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
            self.scan_focusables();
            self.update_cursor();

            if !self.app.scheduler().borrow().has_pending() && self.focused_node().is_none() {
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

        if key.code == KeyCode::Tab {
            self.focus_next();
            return Ok(false);
        }

        if key.code == KeyCode::BackTab {
            self.focus_prev();
            return Ok(false);
        }

        // If focused node is an input, keep native text editing behavior
        if let Some(node_id) = self.focused_input_node() {
            match key.code {
                KeyCode::Char(c)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT) =>
                {
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
                }
                KeyCode::Esc => {
                    if let Some(token) = key_event_token(key) {
                        self.app.push_event(UiEvent::Key {
                            node_id,
                            key: token,
                        });
                    }
                }
                _ => {
                    if let Some(token) = key_event_token(key) {
                        self.app.push_event(UiEvent::Key {
                            node_id,
                            key: token,
                        });
                    }
                }
            }
            return Ok(false);
        }

        // Focused non-input node: forward key tokens to app callback
        if let Some(node_id) = self.focused_node() {
            if let Some(token) = key_event_token(key) {
                self.app.push_event(UiEvent::Key {
                    node_id,
                    key: token,
                });
            }
        } else {
            // No focused node — q or Esc exits
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

    /// Scan the tree for focusable nodes. Auto-focus the first one found.
    fn scan_focusables(&mut self) {
        let previous_focused = self.focused_node();
        let focusables = {
            let registry = self.app.registry();
            let reg = registry.borrow();
            if let Some(root) = reg.root() {
                let mut nodes = Vec::new();
                collect_focusable_nodes(&reg, root, &mut nodes);
                Some(nodes)
            } else {
                None
            }
        };

        if let Some(focusables) = focusables {
            self.focusable_nodes = focusables;
            if self.focusable_nodes.is_empty() {
                self.focus_index = None;
                self.input_buffer.clear();
            } else {
                let mut next_focus = previous_focused
                    .and_then(|node_id| self.focusable_nodes.iter().position(|id| *id == node_id))
                    .or(Some(0));

                // If an input is present and current focus is not input, prefer input.
                // This makes typing work immediately when an input appears dynamically.
                let preferred_input_idx = {
                    let registry = self.app.registry();
                    let reg = registry.borrow();
                    let focused_is_input = next_focus
                        .and_then(|idx| self.focusable_nodes.get(idx).copied())
                        .and_then(|node_id| reg.get_node(node_id))
                        .map(|node| matches!(node, UiNode::Input { .. }))
                        .unwrap_or(false);

                    if focused_is_input {
                        None
                    } else {
                        self.focusable_nodes
                            .iter()
                            .position(|id| matches!(reg.get_node(*id), Some(UiNode::Input { .. })))
                    }
                };

                if let Some(input_idx) = preferred_input_idx {
                    next_focus = Some(input_idx);
                }

                self.focus_index = next_focus;
                self.sync_input_buffer_from_focus();
            }
        } else {
            self.focusable_nodes.clear();
            self.focus_index = None;
            self.input_buffer.clear();
        }
    }

    /// Position the terminal cursor at the focused input's edit position.
    fn update_cursor(&mut self) {
        if let Some(node_id) = self.focused_input_node() {
            let col_offset = self.input_buffer.len() as u16;
            let _ = self.app.renderer().show_cursor_at(node_id, col_offset);
        } else {
            let _ = self.app.renderer().hide_cursor();
        }
    }

    #[inline]
    fn focused_node(&self) -> Option<NodeId> {
        self.focus_index
            .and_then(|idx| self.focusable_nodes.get(idx).copied())
    }

    fn focused_input_node(&self) -> Option<NodeId> {
        let node_id = self.focused_node()?;
        let registry = self.app.registry();
        let reg = registry.borrow();
        match reg.get_node(node_id) {
            Some(UiNode::Input { .. }) => Some(node_id),
            _ => None,
        }
    }

    fn focus_next(&mut self) {
        if self.focusable_nodes.is_empty() {
            return;
        }
        let next = match self.focus_index {
            Some(idx) => (idx + 1) % self.focusable_nodes.len(),
            None => 0,
        };
        self.focus_index = Some(next);
        self.sync_input_buffer_from_focus();
    }

    fn focus_prev(&mut self) {
        if self.focusable_nodes.is_empty() {
            return;
        }
        let prev = match self.focus_index {
            Some(0) => self.focusable_nodes.len() - 1,
            Some(idx) => idx - 1,
            None => 0,
        };
        self.focus_index = Some(prev);
        self.sync_input_buffer_from_focus();
    }

    fn sync_input_buffer_from_focus(&mut self) {
        if let Some(node_id) = self.focused_node() {
            let registry = self.app.registry();
            let reg = registry.borrow();
            if let Some(UiNode::Input { value, .. }) = reg.get_node(node_id) {
                self.input_buffer = value.value().to_string();
                return;
            }
        }
        self.input_buffer.clear();
    }
}

/// Recursively collect all focusable node IDs from the tree.
fn collect_focusable_nodes(registry: &UiRegistry, node_id: NodeId, out: &mut Vec<NodeId>) {
    let node = match registry.get_node(node_id) {
        Some(n) => n,
        None => return,
    };

    match node {
        UiNode::Input { .. } => {
            out.push(node_id);
        }
        UiNode::KeyArea { child, .. } => {
            out.push(node_id);
            if let Some(child_id) = child {
                collect_focusable_nodes(registry, *child_id, out);
            }
        }
        UiNode::Column { children }
        | UiNode::Row { children }
        | UiNode::View { children }
        | UiNode::List { children, .. } => {
            let children = children.clone();
            for child_id in children {
                collect_focusable_nodes(registry, child_id, out);
            }
        }
        UiNode::Conditional { child, .. } => {
            if let Some(child_id) = child {
                collect_focusable_nodes(registry, *child_id, out);
            }
        }
        _ => {}
    }
}

fn key_event_token(key: KeyEvent) -> Option<String> {
    match key.code {
        KeyCode::Up => Some("up".to_string()),
        KeyCode::Down => Some("down".to_string()),
        KeyCode::Left => Some("left".to_string()),
        KeyCode::Right => Some("right".to_string()),
        KeyCode::Home => Some("home".to_string()),
        KeyCode::End => Some("end".to_string()),
        KeyCode::PageUp => Some("page_up".to_string()),
        KeyCode::PageDown => Some("page_down".to_string()),
        KeyCode::Enter => Some("enter".to_string()),
        KeyCode::Esc => Some("esc".to_string()),
        KeyCode::Tab => Some("tab".to_string()),
        KeyCode::BackTab => Some("backtab".to_string()),
        KeyCode::Backspace => Some("backspace".to_string()),
        KeyCode::Delete => Some("delete".to_string()),
        KeyCode::Char(' ') => Some("space".to_string()),
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                Some(format!("ctrl+{}", c))
            } else if key.modifiers.contains(KeyModifiers::ALT) {
                Some(format!("alt+{}", c))
            } else {
                Some(format!("char:{}", c))
            }
        }
        _ => None,
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
        assert!(runner.focus_index.is_none());
        assert!(runner.input_buffer.is_empty());
    }

    #[test]
    fn test_collect_focusable_nodes_empty_tree() {
        let registry = UiRegistry::new();
        let mut nodes = Vec::new();
        // No root → no crash
        if let Some(root) = registry.root() {
            collect_focusable_nodes(&registry, root, &mut nodes);
        }
        assert!(nodes.is_empty());
    }

    #[test]
    fn test_collect_focusable_nodes_finds_inputs() {
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

        let mut nodes = Vec::new();
        collect_focusable_nodes(&registry, col, &mut nodes);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0], input);
    }

    #[test]
    fn test_collect_focusable_nodes_nested() {
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

        let mut nodes = Vec::new();
        collect_focusable_nodes(&registry, col, &mut nodes);
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0], input1);
        assert_eq!(nodes[1], input2);
    }

    #[test]
    fn test_no_inputs_means_no_focus() {
        let mut registry = UiRegistry::new();
        let text = registry.create_node(UiNode::Text {
            content: TextContent::Static("no inputs here".into()),
        });
        registry.set_root(text);

        let mut nodes = Vec::new();
        collect_focusable_nodes(&registry, text, &mut nodes);
        assert!(nodes.is_empty());
    }

    #[test]
    fn test_collect_focusable_nodes_includes_key_area() {
        let mut registry = UiRegistry::new();
        let child = registry.create_node(UiNode::Text {
            content: TextContent::Static("x".into()),
        });
        let key_area = registry.create_node(UiNode::KeyArea {
            child: Some(child),
            on_key: None,
        });
        registry.set_root(key_area);

        let mut nodes = Vec::new();
        collect_focusable_nodes(&registry, key_area, &mut nodes);
        assert_eq!(nodes, vec![key_area]);
    }

    #[test]
    fn test_key_event_token_maps_navigation_keys() {
        let up = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let page_down = KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE);

        assert_eq!(key_event_token(up).as_deref(), Some("up"));
        assert_eq!(key_event_token(enter).as_deref(), Some("enter"));
        assert_eq!(key_event_token(page_down).as_deref(), Some("page_down"));
    }

    #[test]
    fn test_key_event_token_maps_chars_and_modifiers() {
        let plain = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        let ctrl = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL);
        let alt = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT);

        assert_eq!(key_event_token(plain).as_deref(), Some("char:j"));
        assert_eq!(key_event_token(ctrl).as_deref(), Some("ctrl+k"));
        assert_eq!(key_event_token(alt).as_deref(), Some("alt+x"));
    }
}
