use anyhow::{anyhow, Result};
use rover_lua::Value;
use serde::{Deserialize, Serialize};
use skia_safe::{textlayout, Color, Canvas, Font, FontMgr, Paint, Point, Rect};
use skia_safe::surfaces;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum Dimension {
    Auto,
    Full,
    Px(f32),
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderResult {
    pub buffer: Vec<u8>,
    pub width: i32,
    pub height: i32,
    pub row_bytes: usize,
    pub hits_json: String,
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

pub struct SkiaRenderer {
    font: Font,
}

impl SkiaRenderer {
    pub fn new() -> Self {
        let mut font = Font::default();
        font.set_size(18.0);
        Self { font }
    }

    #[allow(invalid_reference_casting)]
    pub fn render(&self, view: &ViewNode, width: i32, height: i32) -> Result<RenderResult> {
        let mut hits = Vec::new();
        let mut surface = surfaces::raster_n32_premul((width, height)).ok_or_else(|| anyhow!("surface"))?;
        let canvas_ptr = surface.canvas() as *const Canvas as *mut Canvas;
        let canvas = unsafe { &mut *canvas_ptr };
        canvas.clear(Color::WHITE);
        self.draw_node(view, canvas, Rect::from_xywh(0.0, 0.0, width as f32, height as f32), &mut hits)?;
        let image = surface.image_snapshot();
        let pixmap = image.peek_pixels().ok_or_else(|| anyhow!("peek pixels"))?;
        let row_bytes = pixmap.row_bytes() as usize;
        let bytes = pixmap.bytes().unwrap_or(&[]).to_vec();
        let hits_json = serde_json::to_string(&hits)?;
        Ok(RenderResult {
            buffer: bytes,
            width,
            height,
            row_bytes,
            hits_json,
        })
    }

    fn draw_node(
        &self,
        node: &ViewNode,
        canvas: &mut Canvas,
        bounds: Rect,
        hits: &mut Vec<ActionHit>,
    ) -> Result<()> {
        match node.kind.as_str() {
            "col" => {
                let mut y = bounds.top();
                for child in &node.children {
                    let h = child.height_px(bounds.height() / node.children.len().max(1) as f32);
                    let rect = Rect::from_xywh(bounds.left(), y, bounds.width(), h);
                    self.draw_node(child, canvas, rect, hits)?;
                    y += h + 8.0;
                }
            }
            "row" => {
                let mut x = bounds.left();
                for child in &node.children {
                    let w = child.width_px(bounds.width() / node.children.len().max(1) as f32);
                    let rect = Rect::from_xywh(x, bounds.top(), w, bounds.height());
                    self.draw_node(child, canvas, rect, hits)?;
                    x += w + 8.0;
                }
            }
            "text" => {
                let text = node.text.clone().unwrap_or_default();
                self.draw_text(canvas, &text, Point::new(bounds.left(), bounds.top()));
            }
            "button" => {
                let mut paint = Paint::default();
                paint.set_color(Color::from_rgb(50, 90, 240));
                canvas.draw_rect(bounds, &paint);
                let text = node.text.clone().unwrap_or_else(|| "Button".into());
                self.draw_text(canvas, &text, Point::new(bounds.left() + 8.0, bounds.top() + 26.0));
                if let Some(action) = node.action.clone() {
                    hits.push(ActionHit {
                        action,
                        x: bounds.left(),
                        y: bounds.top(),
                        w: bounds.width(),
                        h: bounds.height(),
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn draw_text(&self, canvas: &mut Canvas, text: &str, at: Point) {
        let mut paint = Paint::default();
        paint.set_color(Color::BLACK);
        let mut collection = textlayout::FontCollection::new();
        collection.set_default_font_manager(FontMgr::default(), None);
        let mut builder = textlayout::ParagraphBuilder::new(&textlayout::ParagraphStyle::default(), collection);
        builder.push_style(&textlayout::TextStyle::new());
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
    fn width_px(&self, default: f32) -> f32 {
        match self.width {
            Some(Dimension::Px(v)) => v,
            Some(Dimension::Full) => default,
            _ => default,
        }
    }

    fn height_px(&self, default: f32) -> f32 {
        match self.height {
            Some(Dimension::Px(v)) => v,
            Some(Dimension::Full) => default,
            _ => default,
        }
    }
}

fn parse_dimension(value: Option<Value>) -> Result<Option<Dimension>> {
    match value {
        None | Some(Value::Nil) => Ok(None),
        Some(Value::String(s)) => {
            let txt = s.to_string_lossy();
            if txt == "full" {
                Ok(Some(Dimension::Full))
            } else {
                Err(anyhow!("unknown dimension string {txt}"))
            }
        }
        Some(Value::Integer(i)) => Ok(Some(Dimension::Px(i as f32))),
        Some(Value::Number(n)) => Ok(Some(Dimension::Px(n as f32))),
        _ => Err(anyhow!("invalid dimension value")),
    }
}
