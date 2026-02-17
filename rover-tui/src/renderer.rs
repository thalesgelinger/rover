use crate::layout::{
    LayoutMap, LayoutRect, compute_layout, node_content, resolve_alignment,
    resolve_fixed_positions, resolve_full_sizes, style_inset,
};
use crate::terminal::Terminal;
use rover_ui::platform::UiTarget;
use rover_ui::ui::{NodeId, NodeStyle, Renderer, StyleOp, StyleSize, UiNode, UiRegistry};
use std::io;
use unicode_width::UnicodeWidthStr;

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
    /// Previous rendered size per node, indexed by NodeId.
    previous_sizes: Vec<(u16, u16)>,
    /// Effective inherited background per node.
    effective_bgs: Vec<Option<String>>,
    /// Origin row offset — layout positions are relative (starting at 0),
    /// so we add origin_row to get absolute screen positions.
    origin_row: u16,
    /// Whether current root is mounted in fullscreen mode.
    mounted_fullscreen: bool,
    /// Frame-level row shift for inline bottom anchoring.
    inline_row_shift: i32,
}

impl TuiRenderer {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            terminal: Terminal::new()?,
            layout: LayoutMap::new(),
            previous_sizes: Vec::new(),
            effective_bgs: Vec::new(),
            origin_row: 0,
            mounted_fullscreen: false,
            inline_row_shift: 0,
        })
    }

    fn compute_inline_row_shift(&self, root: NodeId) -> i32 {
        if self.mounted_fullscreen {
            return 0;
        }
        let content_height = self
            .layout
            .get(root)
            .map(|r| r.height)
            .unwrap_or(self.terminal.content_height())
            .max(1);
        let visible_height = self.terminal.visible_height().max(1);
        // Only shift when content exceeds visible viewport
        // Clamp shift to <= 0 (never push down, only pull up)
        if content_height <= visible_height {
            return 0;
        }
        let shift = -(i32::from(content_height) - i32::from(visible_height));
        shift.min(0)
    }

    fn current_row_shift(&self) -> i32 {
        if self.mounted_fullscreen {
            0
        } else {
            self.inline_row_shift
        }
    }

    #[inline]
    fn set_prev_size(&mut self, id: NodeId, width: u16, height: u16) {
        let idx = id.index();
        if idx >= self.previous_sizes.len() {
            self.previous_sizes.resize(idx + 1, (0, 0));
        }
        self.previous_sizes[idx] = (width, height);
    }

    #[inline]
    fn get_prev_size(&self, id: NodeId) -> (u16, u16) {
        let idx = id.index();
        if idx < self.previous_sizes.len() {
            self.previous_sizes[idx]
        } else {
            (0, 0)
        }
    }

    #[inline]
    fn set_effective_bg(&mut self, id: NodeId, bg: Option<&str>) {
        let idx = id.index();
        if idx >= self.effective_bgs.len() {
            self.effective_bgs.resize(idx + 1, None);
        }
        self.effective_bgs[idx] = bg.map(str::to_string);
    }

    #[inline]
    fn get_effective_bg(&self, id: NodeId) -> Option<String> {
        let idx = id.index();
        if idx < self.effective_bgs.len() {
            self.effective_bgs[idx].clone()
        } else {
            None
        }
    }

    /// Render a single leaf node at its layout position + origin offset.
    fn render_leaf(
        &mut self,
        id: NodeId,
        content: &str,
        rect: &LayoutRect,
        inset: u16,
        fg_hex: Option<&str>,
        bg_hex: Option<&str>,
        row_shift: i32,
        clip_rows: Option<(u16, u16)>,
    ) -> io::Result<()> {
        let (old_width, old_height) = self.get_prev_size(id);
        let lines: Vec<&str> = if content.is_empty() {
            Vec::new()
        } else {
            content.split('\n').collect()
        };
        let new_height = lines.len().min(u16::MAX as usize) as u16;
        let new_width = lines
            .iter()
            .map(|line| UnicodeWidthStr::width(*line))
            .max()
            .unwrap_or(0)
            .min(u16::MAX as usize) as u16;
        let abs_row = i32::from(self.origin_row + rect.row + inset) + row_shift;
        let abs_col = rect.col + inset;

        let clear_width = old_width.max(new_width);
        let clear_height = old_height.max(new_height);
        for dy in 0..clear_height {
            let Some(row) = row_with_offset(abs_row, dy as i32) else {
                continue;
            };
            if !row_visible(row, clip_rows) {
                continue;
            }
            if clear_width == 0 {
                continue;
            }
            if let Some(bg) = bg_hex {
                let style_prefix = ansi_prefix(Some(bg), None);
                if style_prefix.is_empty() {
                    self.terminal
                        .queue_clear_region(row, abs_col, clear_width)?;
                } else {
                    let spaces = " ".repeat(clear_width as usize);
                    let line = format!("{}{}\x1b[0m", style_prefix, spaces);
                    self.terminal.queue_write_at(row, abs_col, &line)?;
                }
            } else {
                self.terminal
                    .queue_clear_region(row, abs_col, clear_width)?;
            }
        }

        for (dy, line) in lines.iter().enumerate() {
            let Some(row) = row_with_offset(abs_row, dy as i32) else {
                continue;
            };
            if !row_visible(row, clip_rows) {
                continue;
            }
            let style_prefix = ansi_prefix(bg_hex, fg_hex);
            if style_prefix.is_empty() {
                self.terminal.queue_write_at(row, abs_col, line)?;
            } else {
                let rendered = format!("{}{}\x1b[0m", style_prefix, line);
                self.terminal.queue_write_at(row, abs_col, &rendered)?;
            }
        }

        self.set_prev_size(id, new_width, new_height);
        Ok(())
    }

    fn draw_style_ops_with_clip(
        &mut self,
        rect: &LayoutRect,
        style: &NodeStyle,
        row_shift: i32,
        clip_rows: Option<(u16, u16)>,
    ) -> io::Result<()> {
        let mut layer = *rect;
        let mut border_color: Option<String> = None;

        for op in &style.ops {
            match op {
                StyleOp::BgColor(color) => {
                    self.draw_filled_rect(
                        &layer,
                        Some(color.as_str()),
                        None,
                        row_shift,
                        clip_rows,
                    )?;
                }
                StyleOp::BorderColor(color) => {
                    border_color = Some(color.clone());
                }
                StyleOp::BorderWidth(width) => {
                    self.draw_border_rect(
                        &layer,
                        *width,
                        border_color.as_deref(),
                        row_shift,
                        clip_rows,
                    )?;
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
        row_shift: i32,
        clip_rows: Option<(u16, u16)>,
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
            let base_row = i32::from(self.origin_row + rect.row + dy) + row_shift;
            let Some(row) = row_with_offset(base_row, 0) else {
                continue;
            };
            if !row_visible(row, clip_rows) {
                continue;
            }
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
        row_shift: i32,
        clip_rows: Option<(u16, u16)>,
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
            let top_row_i32 = i32::from(self.origin_row + rect.row + i) + row_shift;
            let bottom_row_i32 =
                i32::from(self.origin_row + rect.row + rect.height - 1 - i) + row_shift;
            let top_row = row_with_offset(top_row_i32, 0);
            let bottom_row = row_with_offset(bottom_row_i32, 0);
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
            if let Some(row) = top_row
                && row_visible(row, clip_rows)
            {
                self.terminal.queue_write_at(row, left_col, &top_line)?;
            }
            if bottom_row != top_row {
                let bottom_line = format!(
                    "{}{}{}",
                    style_prefix,
                    "#".repeat(horiz_width as usize),
                    style_reset
                );
                if let Some(row) = bottom_row
                    && row_visible(row, clip_rows)
                {
                    self.terminal.queue_write_at(row, left_col, &bottom_line)?;
                }
            }

            if rect.height > i.saturating_mul(2).saturating_add(2) {
                for logical_row in (top_row_i32 + 1)..bottom_row_i32 {
                    let Some(dy) = row_with_offset(logical_row, 0) else {
                        continue;
                    };
                    if !row_visible(dy, clip_rows) {
                        continue;
                    }
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

    fn render_tree_with_clip(
        &mut self,
        registry: &UiRegistry,
        node_id: NodeId,
        inherited_bg: Option<&str>,
        row_shift: i32,
        clip_rows: Option<(u16, u16)>,
    ) -> io::Result<()> {
        let node = match registry.get_node(node_id) {
            Some(n) => n,
            None => return Ok(()),
        };

        let mut effective_bg = inherited_bg.map(str::to_string);
        if let Some(style) = registry.get_node_style(node_id)
            && let Some(node_bg) = style_bg_color(style)
        {
            effective_bg = Some(node_bg.to_string());
        }
        self.set_effective_bg(node_id, effective_bg.as_deref());

        if let Some(content) = node_content(node) {
            if let Some(rect) = self.layout.get(node_id) {
                let rect = *rect;
                let inset = registry
                    .get_node_style(node_id)
                    .map(style_inset)
                    .unwrap_or(0);
                if let Some(style) = registry.get_node_style(node_id) {
                    self.draw_style_ops_with_clip(&rect, style, row_shift, clip_rows)?;
                }
                let fg = registry
                    .get_node_style(node_id)
                    .and_then(|style| style.color.as_deref());
                self.render_leaf(
                    node_id,
                    &content,
                    &rect,
                    inset,
                    fg,
                    effective_bg.as_deref(),
                    row_shift,
                    clip_rows,
                )?;
            }
            return Ok(());
        }

        if let Some(rect) = self.layout.get(node_id) {
            let rect = *rect;
            if let Some(style) = registry.get_node_style(node_id) {
                self.draw_style_ops_with_clip(&rect, style, row_shift, clip_rows)?;
            }
        }

        if let UiNode::ScrollBox {
            child,
            stick_bottom,
        } = node
        {
            if let Some(scroll_rect) = self.layout.get(node_id).copied() {
                let inset = registry
                    .get_node_style(node_id)
                    .map(style_inset)
                    .unwrap_or(0);
                let clip_start = row_with_offset(
                    i32::from(self.origin_row + scroll_rect.row.saturating_add(inset)) + row_shift,
                    0,
                )
                .unwrap_or(0);
                let clip_height = scroll_rect.height.saturating_sub(inset.saturating_mul(2));
                let clip_end = clip_start.saturating_add(clip_height);
                let next_clip = intersect_clip_rows(clip_rows, Some((clip_start, clip_end)));
                if let Some(child_id) = child {
                    let child_height = self.layout.get(*child_id).map(|r| r.height).unwrap_or(0);
                    let offset = if *stick_bottom && child_height > clip_height {
                        child_height - clip_height
                    } else {
                        0
                    };
                    self.render_tree_with_clip(
                        registry,
                        *child_id,
                        effective_bg.as_deref(),
                        row_shift - offset as i32,
                        next_clip,
                    )?;
                }
            }
            return Ok(());
        }

        // Container: recurse into children
        let children: Vec<NodeId> = match node {
            UiNode::Column { children }
            | UiNode::Row { children }
            | UiNode::View { children }
            | UiNode::Stack { children }
            | UiNode::List { children, .. } => flatten_list_nodes(registry, children),
            UiNode::ScrollBox { child, .. } => child.iter().copied().collect(),
            UiNode::Conditional { child, .. } => {
                let raw: Vec<NodeId> = child.iter().copied().collect();
                flatten_list_nodes(registry, &raw)
            }
            UiNode::KeyArea { child, .. } => {
                let raw: Vec<NodeId> = child.iter().copied().collect();
                flatten_list_nodes(registry, &raw)
            }
            UiNode::FullScreen { child, .. } => {
                let raw: Vec<NodeId> = child.iter().copied().collect();
                flatten_list_nodes(registry, &raw)
            }
            _ => vec![],
        };

        for child_id in children {
            self.render_tree_with_clip(
                registry,
                child_id,
                effective_bg.as_deref(),
                row_shift,
                clip_rows,
            )?;
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
            let Some(abs_row) = row_with_offset(
                i32::from(self.origin_row + rect.row) + self.current_row_shift(),
                0,
            ) else {
                return Ok(());
            };
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

fn row_visible(row: u16, clip_rows: Option<(u16, u16)>) -> bool {
    match clip_rows {
        Some((start, end)) => row >= start && row < end,
        None => true,
    }
}

fn intersect_clip_rows(a: Option<(u16, u16)>, b: Option<(u16, u16)>) -> Option<(u16, u16)> {
    match (a, b) {
        (None, x) => x,
        (x, None) => x,
        (Some((a0, a1)), Some((b0, b1))) => {
            let start = a0.max(b0);
            let end = a1.min(b1);
            if end <= start {
                Some((start, start))
            } else {
                Some((start, end))
            }
        }
    }
}

fn row_with_offset(base_row: i32, delta: i32) -> Option<u16> {
    let row = base_row.saturating_add(delta);
    if row < 0 || row > i32::from(u16::MAX) {
        None
    } else {
        Some(row as u16)
    }
}

fn flatten_list_nodes(registry: &UiRegistry, children: &[NodeId]) -> Vec<NodeId> {
    let mut flattened = Vec::new();
    flatten_list_nodes_into(registry, children, &mut flattened);
    flattened
}

fn flatten_list_nodes_into(registry: &UiRegistry, children: &[NodeId], out: &mut Vec<NodeId>) {
    for child_id in children {
        match registry.get_node(*child_id) {
            Some(UiNode::List { children, .. }) => flatten_list_nodes_into(registry, children, out),
            _ => out.push(*child_id),
        }
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
            resolve_fixed_positions(registry, root, &mut self.layout);
            self.inline_row_shift = 0;
        } else {
            // Enter inline mode — reserves space and sets origin_row
            if let Err(e) = self.terminal.enter_inline(height) {
                eprintln!("rover-tui: failed to enter terminal: {}", e);
                return;
            }
            self.origin_row = self.terminal.origin_row();
            resolve_full_sizes(registry, root, &mut self.layout);
            resolve_alignment(registry, root, &mut self.layout);
            resolve_fixed_positions(registry, root, &mut self.layout);
            self.inline_row_shift = self.compute_inline_row_shift(root);
        }

        // Render all nodes
        if let Err(e) =
            self.render_tree_with_clip(registry, root, None, self.current_row_shift(), None)
        {
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
                resolve_fixed_positions(registry, root, &mut self.layout);
                self.inline_row_shift = self.compute_inline_row_shift(root);

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
                if let Err(e) =
                    self.render_tree_with_clip(registry, root, None, self.current_row_shift(), None)
                {
                    eprintln!("rover-tui: render error: {}", e);
                    return;
                }
            }
        } else {
            // Content-only changes: update individual leaf nodes
            let row_shift = self.current_row_shift();
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
                    if let Err(e) = self.draw_style_ops_with_clip(&rect, style, row_shift, None) {
                        eprintln!("rover-tui: style draw error for node {:?}: {}", node_id, e);
                        continue;
                    }
                }

                let fg = registry
                    .get_node_style(node_id)
                    .and_then(|style| style.color.as_deref());
                let bg = self.get_effective_bg(node_id);
                if let Err(e) = self.render_leaf(
                    node_id,
                    &content,
                    &rect,
                    inset,
                    fg,
                    bg.as_deref(),
                    row_shift,
                    None,
                ) {
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
        resolve_fixed_positions(registry, root, &mut self.layout);
        self.inline_row_shift = self.compute_inline_row_shift(root);

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

        if let Err(e) =
            self.render_tree_with_clip(registry, root, None, self.current_row_shift(), None)
        {
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
        if idx < self.previous_sizes.len() {
            self.previous_sizes[idx] = (0, 0);
        }
        if idx < self.effective_bgs.len() {
            self.effective_bgs[idx] = None;
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

fn style_bg_color(style: &NodeStyle) -> Option<&str> {
    let mut bg: Option<&str> = None;
    for op in &style.ops {
        if let StyleOp::BgColor(color) = op {
            bg = Some(color.as_str());
        }
    }
    bg
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
    fn test_prev_size_tracking() {
        let mut sizes: Vec<(u16, u16)> = Vec::new();
        let id = NodeId::from_u32(3);
        let idx = id.index();

        if idx >= sizes.len() {
            sizes.resize(idx + 1, (0, 0));
        }
        sizes[idx] = (10, 2);
        assert_eq!(sizes[idx], (10, 2));
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
