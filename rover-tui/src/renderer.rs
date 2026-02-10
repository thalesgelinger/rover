use crate::layout::{
    LayoutMap, LayoutRect, compute_layout, node_content, resolve_alignment, resolve_full_sizes,
    style_inset,
};
use crate::terminal::Terminal;
use rover_ui::platform::UiTarget;
use rover_ui::ui::{NodeId, NodeStyle, Renderer, StyleOp, StyleSize, UiNode, UiRegistry};
use std::io;

/// TUI renderer — draws the UI node tree to the terminal.
///
/// Renders **inline** by default: content appears at the current cursor
/// position, like a CLI progress bar. No alternate screen, no fullscreen.
///
/// Uses a Vec-indexed layout map for O(1) position lookups and tracks
/// previous content widths to clear stale characters when content shrinks.
/// All writes are queued per frame and flushed once.
pub struct TuiRenderer {
    terminal: Terminal,
    layout: LayoutMap,
    /// Previous rendered width per node, indexed by NodeId.
    previous_widths: Vec<u16>,
    /// Origin row offset — layout positions are relative (starting at 0),
    /// so we add origin_row to get absolute screen positions.
    origin_row: u16,
    /// Whether current root is mounted in fullscreen mode.
    mounted_fullscreen: bool,
}

impl TuiRenderer {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            terminal: Terminal::new()?,
            layout: LayoutMap::new(),
            previous_widths: Vec::new(),
            origin_row: 0,
            mounted_fullscreen: false,
        })
    }

    #[inline]
    fn set_prev_width(&mut self, id: NodeId, width: u16) {
        let idx = id.index();
        if idx >= self.previous_widths.len() {
            self.previous_widths.resize(idx + 1, 0);
        }
        self.previous_widths[idx] = width;
    }

    #[inline]
    fn get_prev_width(&self, id: NodeId) -> u16 {
        let idx = id.index();
        if idx < self.previous_widths.len() {
            self.previous_widths[idx]
        } else {
            0
        }
    }

    /// Render a single leaf node at its layout position + origin offset.
    fn render_leaf(
        &mut self,
        id: NodeId,
        content: &str,
        rect: &LayoutRect,
        inset: u16,
    ) -> io::Result<()> {
        let old_width = self.get_prev_width(id);
        let new_width = content.len() as u16;
        let abs_row = self.origin_row + rect.row + inset;
        let abs_col = rect.col + inset;

        if old_width > new_width {
            self.terminal.queue_clear_region(
                abs_row,
                abs_col + new_width,
                old_width - new_width,
            )?;
        }

        self.terminal.queue_write_at(abs_row, abs_col, content)?;
        self.set_prev_width(id, new_width);
        Ok(())
    }

    fn draw_style_ops(&mut self, rect: &LayoutRect, style: &NodeStyle) -> io::Result<()> {
        let mut layer = *rect;
        let mut border_color: Option<String> = None;

        for op in &style.ops {
            match op {
                StyleOp::BgColor(color) => {
                    self.draw_filled_rect(&layer, Some(color.as_str()), None)?;
                }
                StyleOp::BorderColor(color) => {
                    border_color = Some(color.clone());
                }
                StyleOp::BorderWidth(width) => {
                    self.draw_border_rect(&layer, *width, border_color.as_deref())?;
                    inset_rect(&mut layer, *width);
                }
                StyleOp::Padding(width) => {
                    inset_rect(&mut layer, *width);
                }
            }
        }

        Ok(())
    }

    fn draw_filled_rect(
        &mut self,
        rect: &LayoutRect,
        bg_hex: Option<&str>,
        fg_hex: Option<&str>,
    ) -> io::Result<()> {
        if rect.width == 0 || rect.height == 0 {
            return Ok(());
        }

        let style_prefix = ansi_prefix(bg_hex, fg_hex);
        let style_reset = if style_prefix.is_empty() {
            ""
        } else {
            "\x1b[0m"
        };
        let spaces = " ".repeat(rect.width as usize);

        for dy in 0..rect.height {
            let row = self.origin_row + rect.row + dy;
            if style_prefix.is_empty() {
                self.terminal.queue_write_at(row, rect.col, &spaces)?;
            } else {
                let line = format!("{}{}{}", style_prefix, spaces, style_reset);
                self.terminal.queue_write_at(row, rect.col, &line)?;
            }
        }

        Ok(())
    }

    fn draw_border_rect(
        &mut self,
        rect: &LayoutRect,
        border_width: u16,
        border_color: Option<&str>,
    ) -> io::Result<()> {
        if border_width == 0 || rect.width == 0 || rect.height == 0 {
            return Ok(());
        }

        let bw = border_width.min(rect.width / 2).min(rect.height / 2);
        if bw == 0 {
            return Ok(());
        }

        let style_prefix = ansi_prefix(None, border_color);
        let style_reset = if style_prefix.is_empty() {
            ""
        } else {
            "\x1b[0m"
        };

        for i in 0..bw {
            let top_row = self.origin_row + rect.row + i;
            let bottom_row = self.origin_row + rect.row + rect.height - 1 - i;
            let left_col = rect.col + i;
            let right_col = rect.col + rect.width - 1 - i;
            let horiz_width = rect.width.saturating_sub(i.saturating_mul(2));
            if horiz_width == 0 {
                continue;
            }

            let top_line = format!(
                "{}{}{}",
                style_prefix,
                "#".repeat(horiz_width as usize),
                style_reset
            );
            self.terminal.queue_write_at(top_row, left_col, &top_line)?;
            if bottom_row != top_row {
                let bottom_line = format!(
                    "{}{}{}",
                    style_prefix,
                    "#".repeat(horiz_width as usize),
                    style_reset
                );
                self.terminal
                    .queue_write_at(bottom_row, left_col, &bottom_line)?;
            }

            if rect.height > i.saturating_mul(2).saturating_add(2) {
                for dy in (top_row + 1)..bottom_row {
                    let side = format!("{}#{}", style_prefix, style_reset);
                    self.terminal.queue_write_at(dy, left_col, &side)?;
                    if right_col != left_col {
                        self.terminal.queue_write_at(dy, right_col, &side)?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Walk the tree and render all leaf nodes.
    fn render_tree(&mut self, registry: &UiRegistry, node_id: NodeId) -> io::Result<()> {
        let node = match registry.get_node(node_id) {
            Some(n) => n,
            None => return Ok(()),
        };

        if let Some(content) = node_content(node) {
            if let Some(rect) = self.layout.get(node_id) {
                let rect = *rect;
                let inset = registry
                    .get_node_style(node_id)
                    .map(style_inset)
                    .unwrap_or(0);
                if let Some(style) = registry.get_node_style(node_id) {
                    self.draw_style_ops(&rect, style)?;
                }
                self.render_leaf(node_id, &content, &rect, inset)?;
            }
            return Ok(());
        }

        if let Some(rect) = self.layout.get(node_id) {
            let rect = *rect;
            if let Some(style) = registry.get_node_style(node_id) {
                self.draw_style_ops(&rect, style)?;
            }
        }

        // Container: recurse into children
        let children: Vec<NodeId> = match node {
            UiNode::Column { children }
            | UiNode::Row { children }
            | UiNode::View { children }
            | UiNode::Stack { children }
            | UiNode::List { children, .. } => children.clone(),
            UiNode::Conditional { child, .. } => child.iter().copied().collect(),
            UiNode::KeyArea { child, .. } => child.iter().copied().collect(),
            UiNode::FullScreen { child, .. } => child.iter().copied().collect(),
            _ => vec![],
        };

        for child_id in children {
            self.render_tree(registry, child_id)?;
        }

        Ok(())
    }

    /// Refresh cached terminal size (call on resize events).
    pub fn refresh_size(&mut self) {
        self.terminal.refresh_size();
    }

    /// Read current terminal viewport size (cols, rows).
    pub fn viewport_size(&mut self) -> (u16, u16) {
        self.terminal.refresh_size();
        (self.terminal.cols(), self.terminal.rows())
    }

    /// Position the terminal cursor at a node's location + column offset.
    /// Used by the runner to show a blinking cursor inside the focused input.
    pub fn show_cursor_at(&mut self, node_id: NodeId, col_offset: u16) -> io::Result<()> {
        if let Some(rect) = self.layout.get(node_id) {
            let abs_row = self.origin_row + rect.row;
            let abs_col = rect.col + col_offset;
            self.terminal.show_cursor_at(abs_row, abs_col)?;
        }
        Ok(())
    }

    /// Hide the terminal cursor.
    pub fn hide_cursor(&mut self) -> io::Result<()> {
        self.terminal.hide_cursor()
    }
}

impl Default for TuiRenderer {
    fn default() -> Self {
        Self::new().expect("failed to initialize terminal")
    }
}

impl Renderer for TuiRenderer {
    fn mount(&mut self, registry: &UiRegistry) {
        let root = match registry.root() {
            Some(id) => id,
            None => return,
        };

        // Compute layout first to know the content height
        self.layout.clear();
        let (_width, height) = compute_layout(registry, root, 0, 0, &mut self.layout);

        self.mounted_fullscreen =
            matches!(registry.get_node(root), Some(UiNode::FullScreen { .. }));

        if self.mounted_fullscreen {
            if let Err(e) = self.terminal.enter_fullscreen() {
                eprintln!("rover-tui: failed to enter fullscreen terminal: {}", e);
                return;
            }
            self.origin_row = 0;
            if let Err(e) = self.terminal.clear() {
                eprintln!("rover-tui: clear error: {}", e);
                return;
            }
            if let Some(mut root_rect) = self.layout.get(root).copied() {
                let root_style = registry.get_node_style(root);
                root_rect.width = match root_style.and_then(|s| s.width) {
                    Some(StyleSize::Px(v)) => v.max(root_rect.width),
                    Some(StyleSize::Full) | None => self.terminal.cols(),
                };
                root_rect.height = match root_style.and_then(|s| s.height) {
                    Some(StyleSize::Px(v)) => v.max(root_rect.height),
                    Some(StyleSize::Full) | None => self.terminal.rows(),
                };
                self.layout.set(root, root_rect);
            }
            resolve_full_sizes(registry, root, &mut self.layout);
            resolve_alignment(registry, root, &mut self.layout);
        } else {
            // Enter inline mode — reserves space and sets origin_row
            if let Err(e) = self.terminal.enter_inline(height) {
                eprintln!("rover-tui: failed to enter terminal: {}", e);
                return;
            }
            self.origin_row = self.terminal.origin_row();
            resolve_full_sizes(registry, root, &mut self.layout);
            resolve_alignment(registry, root, &mut self.layout);
        }

        // Render all nodes
        if let Err(e) = self.render_tree(registry, root) {
            eprintln!("rover-tui: render error: {}", e);
            return;
        }

        if let Err(e) = self.terminal.flush() {
            eprintln!("rover-tui: flush error: {}", e);
        }
    }

    fn update(&mut self, registry: &UiRegistry, dirty_nodes: &[NodeId]) {
        if dirty_nodes.is_empty() {
            return;
        }

        // Check if any dirty node is a container (structural change)
        let structural_change = dirty_nodes.iter().any(|&id| {
            let has_style = registry
                .get_node_style(id)
                .is_some_and(|style| !style.ops.is_empty());
            registry
                .get_node(id)
                .is_some_and(|n| node_content(n).is_none() || has_style)
        });

        if structural_change {
            // Structural change: re-layout the whole tree and re-render
            if let Some(root) = registry.root() {
                self.layout.clear();
                let (_w, new_height) = compute_layout(registry, root, 0, 0, &mut self.layout);
                if self.mounted_fullscreen {
                    if let Some(mut root_rect) = self.layout.get(root).copied() {
                        let root_style = registry.get_node_style(root);
                        root_rect.width = match root_style.and_then(|s| s.width) {
                            Some(StyleSize::Px(v)) => v.max(root_rect.width),
                            Some(StyleSize::Full) | None => self.terminal.cols(),
                        };
                        root_rect.height = match root_style.and_then(|s| s.height) {
                            Some(StyleSize::Px(v)) => v.max(root_rect.height),
                            Some(StyleSize::Full) | None => self.terminal.rows(),
                        };
                        self.layout.set(root, root_rect);
                    }
                }
                resolve_full_sizes(registry, root, &mut self.layout);
                resolve_alignment(registry, root, &mut self.layout);

                if self.mounted_fullscreen {
                    if let Err(e) = self.terminal.clear() {
                        eprintln!("rover-tui: clear error: {}", e);
                        return;
                    }
                } else {
                    let old_height = self.terminal.content_height();
                    // If the content grew, reserve additional lines
                    if new_height > old_height {
                        if let Err(e) = self.terminal.grow_inline(new_height) {
                            eprintln!("rover-tui: grow error: {}", e);
                            return;
                        }
                        self.origin_row = self.terminal.origin_row();
                    }

                    if let Err(e) = self.terminal.clear_inline_region() {
                        eprintln!("rover-tui: clear error: {}", e);
                        return;
                    }
                }
                if let Err(e) = self.render_tree(registry, root) {
                    eprintln!("rover-tui: render error: {}", e);
                    return;
                }
            }
        } else {
            // Content-only changes: update individual leaf nodes
            for &node_id in dirty_nodes {
                let node = match registry.get_node(node_id) {
                    Some(n) => n,
                    None => continue,
                };

                let content = match node_content(node) {
                    Some(c) => c,
                    None => continue,
                };

                let rect = match self.layout.get(node_id) {
                    Some(r) => *r,
                    None => continue,
                };

                let inset = registry
                    .get_node_style(node_id)
                    .map(style_inset)
                    .unwrap_or(0);
                if let Some(style) = registry.get_node_style(node_id) {
                    if let Err(e) = self.draw_style_ops(&rect, style) {
                        eprintln!("rover-tui: style draw error for node {:?}: {}", node_id, e);
                        continue;
                    }
                }

                if let Err(e) = self.render_leaf(node_id, &content, &rect, inset) {
                    eprintln!("rover-tui: update error for node {:?}: {}", node_id, e);
                }
            }
        }

        if let Err(e) = self.terminal.flush() {
            eprintln!("rover-tui: flush error: {}", e);
        }
    }

    fn node_added(&mut self, registry: &UiRegistry, _node_id: NodeId) {
        let root = match registry.root() {
            Some(id) => id,
            None => return,
        };

        self.layout.clear();
        compute_layout(registry, root, 0, 0, &mut self.layout);
        resolve_full_sizes(registry, root, &mut self.layout);
        resolve_alignment(registry, root, &mut self.layout);

        // Clear and redraw
        let clear_result = if self.mounted_fullscreen {
            self.terminal.clear()
        } else {
            self.terminal.clear_inline_region()
        };
        if let Err(e) = clear_result {
            eprintln!("rover-tui: clear error: {}", e);
            return;
        }

        if let Err(e) = self.render_tree(registry, root) {
            eprintln!("rover-tui: render error: {}", e);
            return;
        }

        if let Err(e) = self.terminal.flush() {
            eprintln!("rover-tui: flush error: {}", e);
        }
    }

    fn node_removed(&mut self, node_id: NodeId) {
        self.layout.remove(node_id);
        let idx = node_id.index();
        if idx < self.previous_widths.len() {
            self.previous_widths[idx] = 0;
        }
    }

    fn target(&self) -> UiTarget {
        UiTarget::Tui
    }
}

impl Drop for TuiRenderer {
    fn drop(&mut self) {
        let _ = self.terminal.leave();
    }
}

fn parse_hex_color(hex: &str) -> Option<(u8, u8, u8)> {
    let raw = hex.trim();
    let s = raw.strip_prefix('#').unwrap_or(raw);
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some((r, g, b))
}

fn ansi_prefix(bg_hex: Option<&str>, fg_hex: Option<&str>) -> String {
    let mut parts = Vec::new();
    if let Some((r, g, b)) = bg_hex.and_then(parse_hex_color) {
        parts.push(format!("48;2;{};{};{}", r, g, b));
    }
    if let Some((r, g, b)) = fg_hex.and_then(parse_hex_color) {
        parts.push(format!("38;2;{};{};{}", r, g, b));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("\x1b[{}m", parts.join(";"))
    }
}

fn inset_rect(rect: &mut LayoutRect, amount: u16) {
    if amount == 0 {
        return;
    }
    let delta = amount.saturating_mul(2);
    rect.row = rect.row.saturating_add(amount);
    rect.col = rect.col.saturating_add(amount);
    rect.width = rect.width.saturating_sub(delta);
    rect.height = rect.height.saturating_sub(delta);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rover_ui::ui::TextContent;

    #[test]
    fn test_prev_width_tracking() {
        let mut widths: Vec<u16> = Vec::new();
        let id = NodeId::from_u32(3);
        let idx = id.index();

        if idx >= widths.len() {
            widths.resize(idx + 1, 0);
        }
        widths[idx] = 10;
        assert_eq!(widths[idx], 10);
    }

    #[test]
    fn test_node_content_for_update_decisions() {
        let text = UiNode::Text {
            content: TextContent::Static("hello".into()),
        };
        assert!(node_content(&text).is_some());

        let col = UiNode::Column { children: vec![] };
        assert!(node_content(&col).is_none());
    }

    #[test]
    fn test_layout_drives_rendering_position() {
        let mut registry = UiRegistry::new();
        let t1 = registry.create_node(UiNode::Text {
            content: TextContent::Static("Hello".into()),
        });
        let t2 = registry.create_node(UiNode::Text {
            content: TextContent::Static("World".into()),
        });
        let col = registry.create_node(UiNode::Column {
            children: vec![t1, t2],
        });
        registry.set_root(col);

        let mut layout = LayoutMap::new();
        compute_layout(&registry, col, 0, 0, &mut layout);

        let r1 = layout.get(t1).unwrap();
        assert_eq!((r1.row, r1.col), (0, 0));

        let r2 = layout.get(t2).unwrap();
        assert_eq!((r2.row, r2.col), (1, 0));
    }

    #[test]
    fn test_row_content_positions_for_counter_pattern() {
        let mut registry = UiRegistry::new();
        let label = registry.create_node(UiNode::Text {
            content: TextContent::Static("Count: ".into()),
        });
        let value = registry.create_node(UiNode::Text {
            content: TextContent::Static("0".into()),
        });
        let row = registry.create_node(UiNode::Row {
            children: vec![label, value],
        });
        registry.set_root(row);

        let mut layout = LayoutMap::new();
        compute_layout(&registry, row, 0, 0, &mut layout);

        let r_label = layout.get(label).unwrap();
        assert_eq!((r_label.row, r_label.col), (0, 0));
        assert_eq!(r_label.width, 7);

        let r_value = layout.get(value).unwrap();
        assert_eq!((r_value.row, r_value.col), (0, 7));
        assert_eq!(r_value.width, 1);
    }

    #[test]
    fn test_origin_offset_applied_to_render() {
        // Layout is relative (starts at 0,0), origin_row offsets it.
        // Verify that a renderer with origin_row=5 would write to row 5+0=5
        // for the first node, 5+1=6 for the second, etc.
        let mut registry = UiRegistry::new();
        let t1 = registry.create_node(UiNode::Text {
            content: TextContent::Static("A".into()),
        });
        let t2 = registry.create_node(UiNode::Text {
            content: TextContent::Static("B".into()),
        });
        let col = registry.create_node(UiNode::Column {
            children: vec![t1, t2],
        });
        registry.set_root(col);

        let mut layout = LayoutMap::new();
        compute_layout(&registry, col, 0, 0, &mut layout);

        let origin_row: u16 = 5;

        let r1 = layout.get(t1).unwrap();
        assert_eq!(origin_row + r1.row, 5);

        let r2 = layout.get(t2).unwrap();
        assert_eq!(origin_row + r2.row, 6);
    }
}
