use crate::SharedSignalRuntime;
use crate::layout::LayoutEngine;
use crate::node::{Node, NodeArena, NodeId, RenderCommand, TextContent};
use crate::renderer::Renderer;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::Alignment,
    style::Modifier,
    style::Style,
    widgets::{Block, Borders, Paragraph},
};
use std::collections::HashMap;
use std::io::{self, Stdout};

pub struct TuiRenderer {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    visible_nodes: HashMap<NodeId, bool>,
    node_text: HashMap<NodeId, String>,
}

impl TuiRenderer {
    pub fn new() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(Self {
            terminal,
            visible_nodes: HashMap::new(),
            node_text: HashMap::new(),
        })
    }

    pub fn run<F>(&mut self, mut render_fn: F) -> io::Result<bool>
    where
        F: FnMut(&mut Frame) -> bool,
    {
        loop {
            let mut should_continue = false;
            self.terminal.draw(|f| {
                should_continue = render_fn(f);
            })?;

            if !should_continue {
                return Ok(false);
            }

            if event::poll(std::time::Duration::from_millis(100))? {
                if let Event::Key(KeyEvent { code, .. }) = event::read()? {
                    if code == KeyCode::Char('q') || code == KeyCode::Esc {
                        return Ok(false);
                    }
                }
            }
        }
    }

    fn render_node(&self, node: NodeId, arena: &NodeArena, layout: &LayoutEngine, f: &mut Frame) {
        if let Some(node_layout) = layout.get_layout(node) {
            if !self.visible_nodes.get(&node).copied().unwrap_or(true) {
                return;
            }

            if let Some(n) = arena.get(node) {
                match n {
                    Node::Text(text_node) => {
                        let text =
                            self.node_text
                                .get(&node)
                                .cloned()
                                .unwrap_or_else(|| match &text_node.content {
                                    TextContent::Static(s) => s.to_string(),
                                    TextContent::Signal(_) => "[Signal]".to_string(),
                                    TextContent::Derived(_) => "[Derived]".to_string(),
                                });

                        let style = if text_node.style.as_ref().map(|s| s.bold).unwrap_or(false) {
                            Style::default().add_modifier(Modifier::BOLD)
                        } else {
                            Style::default()
                        };

                        let paragraph = Paragraph::new(text.as_str())
                            .style(style)
                            .alignment(Alignment::Left)
                            .block(Block::default().borders(Borders::ALL));

                        let rect = ratatui::layout::Rect::new(
                            node_layout.rect.x,
                            node_layout.rect.y,
                            node_layout.rect.width,
                            node_layout.rect.height,
                        );

                        f.render_widget(paragraph, rect);
                    }
                    Node::Column(_) | Node::Row(_) => {
                        let children = arena.children(node);
                        for child in children {
                            self.render_node(child, arena, layout, f);
                        }
                    }
                    Node::Conditional(_) => {
                        let children = arena.children(node);
                        for child in children {
                            self.render_node(child, arena, layout, f);
                        }
                    }
                    Node::Each(_) => {
                        let children = arena.children(node);
                        for child in children {
                            self.render_node(child, arena, layout, f);
                        }
                    }
                }
            }
        }
    }
}

impl Renderer for TuiRenderer {
    fn apply(&mut self, cmd: &RenderCommand, _arena: &NodeArena, _layout: &LayoutEngine) {
        match cmd {
            RenderCommand::UpdateText { node, value } => {
                self.node_text.insert(*node, value.clone());
            }
            RenderCommand::Show { node } => {
                self.visible_nodes.insert(*node, true);
            }
            RenderCommand::Hide { node } => {
                self.visible_nodes.insert(*node, false);
            }
            RenderCommand::InsertChild { .. } | RenderCommand::RemoveChild { .. } => {}
            RenderCommand::MountTree { .. } => {}
            RenderCommand::ReplaceEach { .. } => {}
        }
    }

    fn render_frame(
        &mut self,
        root: NodeId,
        arena: &NodeArena,
        layout: &LayoutEngine,
    ) -> io::Result<()> {
        let visible_nodes = self.visible_nodes.clone();
        let node_text = self.node_text.clone();

        self.terminal.draw(|f| {
            Self::render_node_static(&visible_nodes, &node_text, root, arena, layout, f);
        })?;
        Ok(())
    }
}

impl TuiRenderer {
    fn render_node_static(
        visible_nodes: &HashMap<NodeId, bool>,
        node_text: &HashMap<NodeId, String>,
        node: NodeId,
        arena: &NodeArena,
        layout: &LayoutEngine,
        f: &mut Frame,
    ) {
        if let Some(node_layout) = layout.get_layout(node) {
            if !visible_nodes.get(&node).copied().unwrap_or(true) {
                return;
            }

            if let Some(n) = arena.get(node) {
                match n {
                    Node::Text(text_node) => {
                        let text =
                            node_text.get(&node).cloned().unwrap_or_else(|| {
                                match &text_node.content {
                                    TextContent::Static(s) => s.to_string(),
                                    TextContent::Signal(_) => "[Signal]".to_string(),
                                    TextContent::Derived(_) => "[Derived]".to_string(),
                                }
                            });

                        let style = if text_node.style.as_ref().map(|s| s.bold).unwrap_or(false) {
                            Style::default().add_modifier(Modifier::BOLD)
                        } else {
                            Style::default()
                        };

                        let paragraph = Paragraph::new(text.as_str())
                            .style(style)
                            .alignment(Alignment::Left)
                            .block(Block::default().borders(Borders::ALL));

                        let rect = ratatui::layout::Rect::new(
                            node_layout.rect.x,
                            node_layout.rect.y,
                            node_layout.rect.width,
                            node_layout.rect.height,
                        );

                        f.render_widget(paragraph, rect);
                    }
                    Node::Column(_) | Node::Row(_) => {
                        let children = arena.children(node);
                        for child in children {
                            Self::render_node_static(
                                visible_nodes,
                                node_text,
                                child,
                                arena,
                                layout,
                                f,
                            );
                        }
                    }
                    Node::Conditional(_) => {
                        let children = arena.children(node);
                        for child in children {
                            Self::render_node_static(
                                visible_nodes,
                                node_text,
                                child,
                                arena,
                                layout,
                                f,
                            );
                        }
                    }
                    Node::Each(_) => {
                        let children = arena.children(node);
                        for child in children {
                            Self::render_node_static(
                                visible_nodes,
                                node_text,
                                child,
                                arena,
                                layout,
                                f,
                            );
                        }
                    }
                }
            }
        }
    }
}

impl Drop for TuiRenderer {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
    }
}

pub fn run_tui(root: NodeId, runtime: &SharedSignalRuntime) -> io::Result<()> {
    let mut renderer = match TuiRenderer::new() {
        Ok(r) => r,
        Err(e) => {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to initialize TUI: {}. Are you running in an interactive terminal?", e),
            ));
        }
    };
    let mut layout = crate::layout::LayoutEngine::new();

    // Get terminal size for layout
    let size = renderer.terminal.size()?;

    // Compute layouts
    {
        let arena = runtime.node_arena.borrow();
        layout.compute(root, &arena, crate::layout::Size {
            width: size.width,
            height: size.height,
        });
    }

    // Initial render
    {
        let arena = runtime.node_arena.borrow();
        renderer.render_frame(root, &arena, &layout)?;
    }

    // Wait for 'q' to exit
    loop {
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(KeyEvent {
                code: KeyCode::Char('q'),
                ..
            }) = event::read()?
            {
                break;
            }
        }
    }

    Ok(())
}
