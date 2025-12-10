use anyhow::{anyhow, Result};
use rover_lua::Value;
use serde::{Deserialize, Serialize};
use skia_safe::gpu::{self, backend_render_targets, mtl, SurfaceOrigin};
use skia_safe::surfaces;
use skia_safe::{textlayout, Canvas, Color, ColorType, FontMgr, Paint, Point, Rect};
use std::ffi::c_void;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum Dimension {
    Auto,
    Full,
    Px(f32),
    Flex(f32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewNode {
    pub kind: String,
    #[serde(default)]
    pub children: Vec<ViewNode>,
    pub text: Option<String>,
    pub width: Option<Dimension>,
    pub height: Option<Dimension>,
    pub action: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionHit {
    pub action: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone)]
pub struct LayerNode {
    pub kind: String,
    pub bounds: Rect,
    pub text: Option<String>,
    pub action: Option<String>,
    pub children: Vec<LayerNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderResult {
    pub buffer: Vec<u8>,
    pub width: i32,
    pub height: i32,
    pub row_bytes: usize,
    pub hits_json: String,
    pub hits: Vec<ActionHit>,
    pub layer_tree: Option<LayerTreeSerialized>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerTreeSerialized {
    pub root: LayerNodeSerialized,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerNodeSerialized {
    pub kind: String,
    pub bounds: (f32, f32, f32, f32),
    pub text: Option<String>,
    pub action: Option<String>,
    pub children: Vec<LayerNodeSerialized>,
}

impl From<&LayerNode> for LayerNodeSerialized {
    fn from(node: &LayerNode) -> Self {
        Self {
            kind: node.kind.clone(),
            bounds: (node.bounds.left(), node.bounds.top(), node.bounds.width(), node.bounds.height()),
            text: node.text.clone(),
            action: node.action.clone(),
            children: node.children.iter().map(|c| c.into()).collect(),
        }
    }
}

pub struct RenderSurface {
    backend: RenderSurfaceBackend,
}

pub struct SurfaceSnapshot {
    pub buffer: Vec<u8>,
    pub width: i32,
    pub height: i32,
    pub row_bytes: usize,
}

enum RenderSurfaceBackend {
    CpuRaster(skia_safe::Surface),
    Metal {
        surface: skia_safe::Surface,
        context: gpu::DirectContext,
    },
}

impl RenderSurface {
    pub fn cpu_rgba(width: i32, height: i32) -> Result<Self> {
        let surface =
            surfaces::raster_n32_premul((width, height)).ok_or_else(|| anyhow!("surface"))?;
        Ok(Self {
            backend: RenderSurfaceBackend::CpuRaster(surface),
        })
    }

    pub unsafe fn metal(
        device: *mut c_void,
        queue: *mut c_void,
        texture: *mut c_void,
        width: i32,
        height: i32,
    ) -> Result<Self> {
        let backend = mtl::BackendContext::new(device as mtl::Handle, queue as mtl::Handle);
        let mut context = gpu::direct_contexts::make_metal(&backend, None)
            .ok_or_else(|| anyhow!("metal context"))?;
        let texture_info = mtl::TextureInfo::new(texture as mtl::Handle);
        let backend_render_target =
            backend_render_targets::make_mtl((width, height), &texture_info);
        let surface = gpu::surfaces::wrap_backend_render_target(
            &mut context,
            &backend_render_target,
            SurfaceOrigin::TopLeft,
            ColorType::BGRA8888,
            None,
            None,
        )
        .ok_or_else(|| anyhow!("wrap metal surface"))?;
        Ok(Self {
            backend: RenderSurfaceBackend::Metal { surface, context },
        })
    }

    pub fn canvas(&mut self) -> &mut Canvas {
        match &mut self.backend {
            RenderSurfaceBackend::CpuRaster(surface) | RenderSurfaceBackend::Metal { surface, .. } => {
                #[allow(invalid_reference_casting)]
                unsafe { &mut *(surface.canvas() as *const Canvas as *mut Canvas) }
            }
        }
    }

    pub fn size(&mut self) -> (i32, i32) {
        let info = match &mut self.backend {
            RenderSurfaceBackend::CpuRaster(surface) | RenderSurfaceBackend::Metal { surface, .. } => {
                surface.image_info()
            }
        };
        (info.width(), info.height())
    }

    pub fn snapshot_rgba(&mut self) -> Option<SurfaceSnapshot> {
        match &mut self.backend {
            RenderSurfaceBackend::CpuRaster(surface) => {
                let image = surface.image_snapshot();
                let pixmap = image.peek_pixels()?;
                let row_bytes = pixmap.row_bytes() as usize;
                let bytes = pixmap.bytes().unwrap_or(&[]).to_vec();
                let info = pixmap.info();
                Some(SurfaceSnapshot {
                    buffer: bytes,
                    width: info.width(),
                    height: info.height(),
                    row_bytes,
                })
            }
            RenderSurfaceBackend::Metal { .. } => None,
        }
    }

    pub fn flush(&mut self) {
        match &mut self.backend {
            RenderSurfaceBackend::CpuRaster(_) => {}
            RenderSurfaceBackend::Metal { context, .. } => {
                context.flush_and_submit();
            }
        }
    }
}

pub struct SkiaRenderer {
    font_collection: textlayout::FontCollection,
    scale_factor: f32,
}

impl SkiaRenderer {
    pub fn new() -> Self {
        let mut font_collection = textlayout::FontCollection::new();
        font_collection.set_default_font_manager(FontMgr::default(), None);
        Self { font_collection, scale_factor: 1.0 }
    }

    pub fn load_custom_fonts(&mut self, fonts_dir: &std::path::Path) -> Result<()> {
        if !fonts_dir.exists() {
            return Ok(());
        }
        
        let font_mgr = FontMgr::default();
        for entry in std::fs::read_dir(fonts_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("ttf") 
                || path.extension().and_then(|s| s.to_str()) == Some("otf") {
                if let Ok(data) = std::fs::read(&path) {
                    if let Some(_typeface) = font_mgr.new_from_data(&data, None) {
                        self.font_collection.set_default_font_manager(font_mgr.clone(), None);
                    }
                }
            }
        }
        Ok(())
    }

    pub fn set_scale_factor(&mut self, scale: f32) {
        self.scale_factor = scale;
    }

    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    pub fn build_layer_tree(&self, view: &ViewNode, bounds: Rect) -> Result<LayerNode> {
        self.layout_node(view, bounds)
    }

    pub fn render_layer_tree(
        &self,
        layer: &LayerNode,
        surface: &mut RenderSurface,
    ) -> Result<RenderResult> {
        let mut hits = Vec::new();
        let (width, height) = surface.size();
        let canvas = surface.canvas();
        canvas.clear(Color::WHITE);
        self.draw_layer(layer, canvas, &mut hits)?;
        let snapshot = surface.snapshot_rgba();
        surface.flush();
        let hits_json = serde_json::to_string(&hits)?;
        let (buffer, row_bytes) = match snapshot {
            Some(s) => (s.buffer, s.row_bytes),
            None => (Vec::new(), 0),
        };
        Ok(RenderResult {
            buffer,
            width,
            height,
            row_bytes,
            hits_json,
            hits,
            layer_tree: Some(LayerTreeSerialized {
                root: layer.into(),
            }),
        })
    }

    pub fn render_into_surface(
        &self,
        view: &ViewNode,
        surface: &mut RenderSurface,
    ) -> Result<RenderResult> {
        let (width, height) = surface.size();
        let bounds = Rect::from_xywh(0.0, 0.0, width as f32, height as f32);
        let layer = self.build_layer_tree(view, bounds)?;
        self.render_layer_tree(&layer, surface)
    }

    pub fn render_rgba(&self, view: &ViewNode, width: i32, height: i32) -> Result<RenderResult> {
        let mut surface = RenderSurface::cpu_rgba(width, height)?;
        self.render_into_surface(view, &mut surface)
    }

    fn layout_node(&self, node: &ViewNode, bounds: Rect) -> Result<LayerNode> {
        let mut children = Vec::new();
        match node.kind.as_str() {
            "col" => {
                let sizes = compute_flex_sizes(&node.children, bounds.height(), false);
                let mut y = bounds.top();
                for (child, size) in node.children.iter().zip(sizes.iter()) {
                    let rect = Rect::from_xywh(bounds.left(), y, bounds.width(), *size);
                    children.push(self.layout_node(child, rect)?);
                    y += size + 8.0;
                }
            }
            "row" => {
                let sizes = compute_flex_sizes(&node.children, bounds.width(), true);
                let mut x = bounds.left();
                for (child, size) in node.children.iter().zip(sizes.iter()) {
                    let rect = Rect::from_xywh(x, bounds.top(), *size, bounds.height());
                    children.push(self.layout_node(child, rect)?);
                    x += size + 8.0;
                }
            }
            _ => {}
        }
        Ok(LayerNode {
            kind: node.kind.clone(),
            bounds,
            text: node.text.clone(),
            action: node.action.clone(),
            children,
        })
    }

    fn draw_layer(&self, layer: &LayerNode, canvas: &mut Canvas, hits: &mut Vec<ActionHit>) -> Result<()> {
        match layer.kind.as_str() {
            "text" => {
                if let Some(ref text) = layer.text {
                    self.draw_text(canvas, text, Point::new(layer.bounds.left(), layer.bounds.top()));
                }
            }
            "button" => {
                let mut paint = Paint::default();
                paint.set_color(Color::from_rgb(50, 90, 240));
                canvas.draw_rect(layer.bounds, &paint);
                if let Some(ref text) = layer.text {
                    self.draw_text_colored(
                        canvas,
                        text,
                        Point::new(layer.bounds.left() + 8.0, layer.bounds.top() + 26.0),
                        Color::WHITE,
                    );
                }
                if let Some(ref action) = layer.action {
                    hits.push(ActionHit {
                        action: action.clone(),
                        x: layer.bounds.left(),
                        y: layer.bounds.top(),
                        w: layer.bounds.width(),
                        h: layer.bounds.height(),
                    });
                }
            }
            _ => {}
        }
        for child in &layer.children {
            self.draw_layer(child, canvas, hits)?;
        }
        Ok(())
    }

    fn draw_text(&self, canvas: &mut Canvas, text: &str, at: Point) {
        self.draw_text_colored(canvas, text, at, Color::BLACK);
    }

    fn draw_text_colored(&self, canvas: &mut Canvas, text: &str, at: Point, color: Color) {
        let mut builder =
            textlayout::ParagraphBuilder::new(&textlayout::ParagraphStyle::default(), self.font_collection.clone());
        let mut text_style = textlayout::TextStyle::new();
        text_style.set_font_size(16.0 * self.scale_factor);
        text_style.set_color(color);
        builder.push_style(&text_style);
        builder.add_text(text);
        let mut paragraph = builder.build();
        paragraph.layout(1000.0);
        canvas.save();
        canvas.translate((at.x, at.y));
        paragraph.paint(canvas, (0, 0));
        canvas.restore();
    }
}

impl ViewNode {
    pub fn from_value(value: &Value) -> Result<Self> {
        match value.clone() {
            Value::Table(table) => {
                let kind: String = table
                    .get("kind")
                    .map_err(|_| anyhow!("view table missing kind"))?;

                let width = parse_dimension(table.get::<_, Option<Value>>("width")?)?;
                let height = parse_dimension(table.get::<_, Option<Value>>("height")?)?;

                let mut children = Vec::new();
                for val in table.clone().sequence_values::<Value>() {
                    let child = val?;
                    if let Value::Table(_) = child {
                        children.push(ViewNode::from_value(&child)?);
                    }
                }

                let text = if kind == "text" || kind == "button" {
                    table.get::<_, Option<String>>(1).ok().flatten()
                } else {
                    None
                };

                let action = match table.get::<_, Option<Value>>("on_click")? {
                    Some(Value::String(s)) => Some(s.to_string_lossy().into_owned()),
                    Some(Value::Integer(i)) => Some(i.to_string()),
                    Some(Value::Number(n)) => Some(n.to_string()),
                    Some(Value::Boolean(b)) => Some(b.to_string()),
                    _ => None,
                };

                Ok(ViewNode {
                    kind,
                    children,
                    text,
                    width,
                    height,
                    action,
                })
            }
            _ => Err(anyhow!("expected render to return table")),
        }
    }

}

fn compute_flex_sizes(children: &[ViewNode], available: f32, is_horizontal: bool) -> Vec<f32> {
    let spacing = 8.0;
    let total_spacing = spacing * (children.len().saturating_sub(1) as f32);
    let available_for_content = (available - total_spacing).max(0.0);
    
    let mut fixed_total = 0.0;
    let mut flex_total = 0.0;
    
    for child in children {
        let dim = if is_horizontal { &child.width } else { &child.height };
        match dim {
            Some(Dimension::Px(v)) => fixed_total += v,
            Some(Dimension::Flex(weight)) => flex_total += weight,
            _ => {}
        }
    }
    
    let remaining = (available_for_content - fixed_total).max(0.0);
    let flex_unit = if flex_total > 0.0 { remaining / flex_total } else { 0.0 };
    let default_size = if children.is_empty() { 0.0 } else { available_for_content / children.len() as f32 };
    
    children.iter().map(|child| {
        let dim = if is_horizontal { &child.width } else { &child.height };
        match dim {
            Some(Dimension::Px(v)) => *v,
            Some(Dimension::Full) => available_for_content,
            Some(Dimension::Flex(weight)) => flex_unit * weight,
            Some(Dimension::Auto) | None => default_size,
        }
    }).collect()
}

fn parse_dimension(value: Option<Value>) -> Result<Option<Dimension>> {
    match value {
        None | Some(Value::Nil) => Ok(None),
        Some(Value::String(s)) => {
            let txt = s.to_string_lossy();
            if txt == "full" {
                Ok(Some(Dimension::Full))
            } else if txt == "auto" {
                Ok(Some(Dimension::Auto))
            } else {
                Err(anyhow!("unknown dimension string {txt}"))
            }
        }
        Some(Value::Table(t)) => {
            if let Ok(kind) = t.get::<_, String>("kind") {
                if kind == "flex" {
                    let weight = t.get::<_, Option<f32>>("value")?.unwrap_or(1.0);
                    return Ok(Some(Dimension::Flex(weight)));
                }
            }
            Err(anyhow!("invalid dimension table"))
        }
        Some(Value::Integer(i)) => Ok(Some(Dimension::Px(i as f32))),
        Some(Value::Number(n)) => Ok(Some(Dimension::Px(n as f32))),
        _ => Err(anyhow!("invalid dimension value")),
    }
}
