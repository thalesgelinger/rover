use crate::layout::LayoutEngine;
use crate::node::{NodeArena, NodeId, RenderCommand};
use crate::SharedSignalRuntime;
use super::traits::Renderer;
use std::io;

/// Mock terminal backend for testing without a real terminal
pub struct MockTerminal {
    buffer: Vec<Vec<char>>,
    width: u16,
    height: u16,
}

impl MockTerminal {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            buffer: vec![vec![' '; width as usize]; height as usize],
            width,
            height,
        }
    }

    pub fn get_text_at(&self, x: u16, y: u16, len: usize) -> String {
        if y >= self.height || x >= self.width {
            return String::new();
        }
        let row = &self.buffer[y as usize];
        let start = x as usize;
        let end = (start + len).min(row.len());
        row[start..end].iter().collect()
    }

    pub fn write_text(&mut self, x: u16, y: u16, text: &str) {
        if y >= self.height || x >= self.width {
            return;
        }
        let row = &mut self.buffer[y as usize];
        for (i, ch) in text.chars().enumerate() {
            let pos = x as usize + i;
            if pos < row.len() {
                row[pos] = ch;
            }
        }
    }

    pub fn clear(&mut self) {
        for row in &mut self.buffer {
            for cell in row {
                *cell = ' ';
            }
        }
    }
}

/// Test renderer that captures render commands for verification
pub struct TestRenderer {
    commands_received: Vec<RenderCommand>,
    mock_terminal: MockTerminal,
}

impl TestRenderer {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            commands_received: Vec::new(),
            mock_terminal: MockTerminal::new(width, height),
        }
    }

    pub fn take_commands(&mut self) -> Vec<RenderCommand> {
        std::mem::take(&mut self.commands_received)
    }

    pub fn commands(&self) -> &[RenderCommand] {
        &self.commands_received
    }

    pub fn clear_commands(&mut self) {
        self.commands_received.clear();
    }

    pub fn terminal(&self) -> &MockTerminal {
        &self.mock_terminal
    }

    pub fn terminal_mut(&mut self) -> &mut MockTerminal {
        &mut self.mock_terminal
    }
}

impl Renderer for TestRenderer {
    fn apply(&mut self, cmd: &RenderCommand, _arena: &NodeArena, _layout: &LayoutEngine) {
        self.commands_received.push(cmd.clone());

        // Also apply basic rendering to mock terminal for visual verification
        match cmd {
            RenderCommand::UpdateText { value, .. } => {
                // For simplicity, just write to top-left
                self.mock_terminal.write_text(0, 0, value);
            }
            _ => {}
        }
    }

    fn render_frame(
        &mut self,
        _root: NodeId,
        _arena: &NodeArena,
        _layout: &LayoutEngine,
        _runtime: &SharedSignalRuntime,
    ) -> io::Result<()> {
        // No-op for test renderer
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_terminal_write_and_read() {
        let mut terminal = MockTerminal::new(80, 24);
        terminal.write_text(0, 0, "Hello");
        assert_eq!(terminal.get_text_at(0, 0, 5), "Hello");
    }

    #[test]
    fn test_mock_terminal_clear() {
        let mut terminal = MockTerminal::new(80, 24);
        terminal.write_text(0, 0, "Hello");
        terminal.clear();
        assert_eq!(terminal.get_text_at(0, 0, 5), "     ");
    }

    #[test]
    fn test_renderer_captures_commands() {
        use crate::node::NodeId;

        let mut renderer = TestRenderer::new(80, 24);
        let node = NodeId(0);

        let cmd = RenderCommand::UpdateText {
            node,
            value: "Test".to_string(),
        };

        renderer.apply(&cmd, &NodeArena::new(), &LayoutEngine::new());

        let commands = renderer.take_commands();
        assert_eq!(commands.len(), 1);
        match &commands[0] {
            RenderCommand::UpdateText { node: n, value } => {
                assert_eq!(*n, node);
                assert_eq!(value, "Test");
            }
            _ => panic!("Expected UpdateText command"),
        }
    }
}
