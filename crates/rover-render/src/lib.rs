use anyhow::{anyhow, Result};
use rover_lua::Value;
use serde::{Deserialize, Serialize};
#[cfg(any(target_os = "ios", target_os = "android"))]
use skia_safe::gpu::{self, backend_render_targets, SurfaceOrigin};
#[cfg(target_os = "ios")]
use skia_safe::gpu::mtl;
#[cfg(target_os = "android")]
use skia_safe::gpu::vk;
use skia_safe::surfaces;
use skia_safe::{textlayout, Canvas, Color, FontMgr, Paint, Point, Rect};
#[cfg(any(target_os = "ios", target_os = "android"))]
use skia_safe::ColorType;
#[cfg(any(target_os = "ios", target_os = "android"))]
use std::ffi::c_void;
#[cfg(target_os = "android")]
use std::ffi::CStr;
#[cfg(target_os = "android")]
use std::os::raw::c_char;

mod theme;
mod icons;

pub use theme::{Palette, Radii, Spacing, Theme, Typography};
pub use icons::IconPaths;

#[cfg(target_os = "android")]
type VulkanGetInstanceProcAddr = unsafe extern "system" fn(*const c_void, *const c_char) -> *const c_void;
#[cfg(target_os = "android")]
type VulkanGetDeviceProcAddr = unsafe extern "system" fn(*const c_void, *const c_char) -> *const c_void;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum Dimension {
    Auto,
    Full,
    Px(f32),
    Flex(f32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum ColorSpec {
    Named(String),
    Rgba(u8, u8, u8, u8),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Style {
    #[serde(default)]
    pub background: Option<ColorSpec>,
    #[serde(default)]
    pub color: Option<ColorSpec>,
    #[serde(default)]
    pub padding: Option<f32>,
    #[serde(default)]
    pub radius: Option<f32>,
    #[serde(default)]
    pub gap: Option<f32>,
}

impl Style {
    fn merged_with(&self, overrides: &Style) -> Style {
        Style {
            background: overrides.background.clone().or_else(|| self.background.clone()),
            color: overrides.color.clone().or_else(|| self.color.clone()),
            padding: overrides.padding.or(self.padding),
            radius: overrides.radius.or(self.radius),
            gap: overrides.gap.or(self.gap),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedStyle {
    pub background: Color,
    pub color: Color,
    pub radius: f32,
    pub padding: f32,
    pub gap: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleSnapshot {
    pub background: (u8, u8, u8, u8),
    pub color: (u8, u8, u8, u8),
    pub radius: f32,
    pub padding: f32,
    pub gap: f32,
}

impl From<&ResolvedStyle> for StyleSnapshot {
    fn from(style: &ResolvedStyle) -> Self {
        let bg = style.background;
        let fg = style.color;
        Self {
            background: (bg.a(), bg.r(), bg.g(), bg.b()),
            color: (fg.a(), fg.r(), fg.g(), fg.b()),
            radius: style.radius,
            padding: style.padding,
            gap: style.gap,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewNode {
    pub kind: String,
    #[serde(default)]
    pub children: Vec<ViewNode>,
    #[serde(default)]
    pub style: Style,
    pub text: Option<String>,
    pub width: Option<Dimension>,
    pub height: Option<Dimension>,
    pub action: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub change_action: Option<String>,
    #[serde(default)]
    pub placeholder: Option<String>,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub checked: Option<bool>,
    #[serde(default)]
    pub icon: Option<String>,
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
    pub style: ResolvedStyle,
    pub children: Vec<LayerNode>,
    pub icon: Option<String>,
    pub progress: f32,
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
    pub style: StyleSnapshot,
    pub children: Vec<LayerNodeSerialized>,
}

impl From<&LayerNode> for LayerNodeSerialized {
    fn from(node: &LayerNode) -> Self {
        Self {
            kind: node.kind.clone(),
            bounds: (node.bounds.left(), node.bounds.top(), node.bounds.width(), node.bounds.height()),
            text: node.text.clone(),
            action: node.action.clone(),
            style: (&node.style).into(),
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
    #[cfg(target_os = "ios")]
    Metal {
        surface: skia_safe::Surface,
        context: gpu::DirectContext,
    },
    #[cfg(target_os = "android")]
    Vulkan {
        surface: skia_safe::Surface,
        context: gpu::DirectContext,
    },
}

#[cfg(target_os = "android")]
fn color_type_from_vk_format(format: vk::Format) -> Option<ColorType> {
    match format {
        vk::Format::R8G8B8A8_UNORM => Some(ColorType::RGBA8888),
        vk::Format::B8G8R8A8_UNORM => Some(ColorType::BGRA8888),
        _ => None,
    }
}

impl RenderSurface {
    pub fn cpu_rgba(width: i32, height: i32) -> Result<Self> {
        let surface =
            surfaces::raster_n32_premul((width, height)).ok_or_else(|| anyhow!("surface"))?;
        Ok(Self {
            backend: RenderSurfaceBackend::CpuRaster(surface),
        })
    }

    #[cfg(target_os = "ios")]
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

    #[cfg(target_os = "android")]
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn vulkan(
        instance: *const c_void,
        physical_device: *const c_void,
        device: *const c_void,
        queue: *const c_void,
        queue_family_index: u32,
        image: *const c_void,
        format: u32,
        image_layout: u32,
        image_usage_flags: u32,
        width: i32,
        height: i32,
        sample_count: i32,
        vk_get_instance_proc_addr: VulkanGetInstanceProcAddr,
        vk_get_device_proc_addr: VulkanGetDeviceProcAddr,
    ) -> Result<Self> {
        let instance = instance as vk::Instance;
        let physical_device = physical_device as vk::PhysicalDevice;
        let device = device as vk::Device;
        let queue = queue as vk::Queue;
        let image = image as vk::Image;
        let format: vk::Format = unsafe { std::mem::transmute(format as i32) };
        let image_layout: vk::ImageLayout = unsafe { std::mem::transmute(image_layout as i32) };
        let image_usage_flags: vk::ImageUsageFlags = unsafe { std::mem::transmute(image_usage_flags) };
        let sample_count = sample_count.max(1) as u32;

        if instance.is_null()
            || physical_device.is_null()
            || device.is_null()
            || queue.is_null()
            || image.is_null()
        {
            return Err(anyhow!("null vulkan handle"));
        }

        let get_proc = |of: vk::GetProcOf| -> vk::GetProcResult {
            unsafe {
                match of {
                    vk::GetProcOf::Instance(inst, name) => {
                        vk_get_instance_proc_addr(inst as *const c_void, name)
                    }
                    vk::GetProcOf::Device(dev, name) => {
                        vk_get_device_proc_addr(dev as *const c_void, name)
                    }
                }
            }
        };

        let required_instance_procs = [
            CStr::from_bytes_with_nul(b"vkGetPhysicalDeviceProperties\0").unwrap(),
            CStr::from_bytes_with_nul(b"vkGetPhysicalDeviceFeatures\0").unwrap(),
            CStr::from_bytes_with_nul(b"vkGetPhysicalDeviceFormatProperties\0").unwrap(),
            CStr::from_bytes_with_nul(b"vkGetPhysicalDeviceImageFormatProperties\0").unwrap(),
            CStr::from_bytes_with_nul(b"vkGetPhysicalDeviceQueueFamilyProperties\0").unwrap(),
            CStr::from_bytes_with_nul(b"vkGetPhysicalDeviceMemoryProperties\0").unwrap(),
        ];

        for name in required_instance_procs {
            let ptr = get_proc(vk::GetProcOf::Instance(instance, name.as_ptr()));
            if ptr.is_null() {
                return Err(anyhow!(format!("missing instance proc {}", name.to_string_lossy())));
            }
        }

        let required_device_procs = [
            CStr::from_bytes_with_nul(b"vkGetDeviceQueue\0").unwrap(),
            CStr::from_bytes_with_nul(b"vkQueueSubmit\0").unwrap(),
            CStr::from_bytes_with_nul(b"vkQueueWaitIdle\0").unwrap(),
            CStr::from_bytes_with_nul(b"vkDeviceWaitIdle\0").unwrap(),
            CStr::from_bytes_with_nul(b"vkCreateCommandPool\0").unwrap(),
            CStr::from_bytes_with_nul(b"vkAllocateCommandBuffers\0").unwrap(),
            CStr::from_bytes_with_nul(b"vkFreeCommandBuffers\0").unwrap(),
            CStr::from_bytes_with_nul(b"vkCreateSemaphore\0").unwrap(),
            CStr::from_bytes_with_nul(b"vkCreateFence\0").unwrap(),
            CStr::from_bytes_with_nul(b"vkWaitForFences\0").unwrap(),
            CStr::from_bytes_with_nul(b"vkResetFences\0").unwrap(),
            CStr::from_bytes_with_nul(b"vkCreateBuffer\0").unwrap(),
            CStr::from_bytes_with_nul(b"vkDestroyBuffer\0").unwrap(),
            CStr::from_bytes_with_nul(b"vkCreateImage\0").unwrap(),
            CStr::from_bytes_with_nul(b"vkDestroyImage\0").unwrap(),
            CStr::from_bytes_with_nul(b"vkAllocateMemory\0").unwrap(),
            CStr::from_bytes_with_nul(b"vkFreeMemory\0").unwrap(),
        ];

        for name in required_device_procs {
            let ptr = get_proc(vk::GetProcOf::Device(device, name.as_ptr()));
            if ptr.is_null() {
                return Err(anyhow!(format!("missing device proc {}", name.to_string_lossy())));
            }
        }

        let backend = vk::BackendContext::new(
            instance,
            physical_device,
            device,
            (queue, queue_family_index as usize),
            &get_proc,
        );
        let mut context = gpu::direct_contexts::make_vulkan(&backend, None)
            .ok_or_else(|| anyhow!("vulkan context"))?;

        let mut image_info = unsafe {
            vk::ImageInfo::new(
                image,
                vk::Alloc::default(),
                vk::ImageTiling::OPTIMAL,
                image_layout,
                format,
                1,
                queue_family_index,
                None,
                None,
                None,
            )
        };
        image_info.sample_count = sample_count;
        image_info.image_usage_flags = image_usage_flags;

        let backend_render_target = backend_render_targets::make_vk((width, height), &image_info);
        let color_type = color_type_from_vk_format(format)
            .ok_or_else(|| anyhow!(format!("unsupported vk format {format:?}")))?;
        let surface = gpu::surfaces::wrap_backend_render_target(
            &mut context,
            &backend_render_target,
            SurfaceOrigin::TopLeft,
            color_type,
            None,
            None,
        )
        .ok_or_else(|| {
            anyhow!(format!(
                "wrap vulkan surface format={:?} layout={:?} usage=0x{:x} sample_count={sample_count}",
                format,
                image_layout,
                image_usage_flags as u32
            ))
        })?;
        Ok(Self {
            backend: RenderSurfaceBackend::Vulkan { surface, context },
        })
    }

    pub fn canvas(&mut self) -> &mut Canvas {
        match &mut self.backend {
            RenderSurfaceBackend::CpuRaster(surface) => {
                #[allow(invalid_reference_casting)]
                unsafe { &mut *(surface.canvas() as *const Canvas as *mut Canvas) }
            }
            #[cfg(target_os = "ios")]
            RenderSurfaceBackend::Metal { surface, .. } => {
                #[allow(invalid_reference_casting)]
                unsafe { &mut *(surface.canvas() as *const Canvas as *mut Canvas) }
            }
            #[cfg(target_os = "android")]
            RenderSurfaceBackend::Vulkan { surface, .. } => {
                #[allow(invalid_reference_casting)]
                unsafe { &mut *(surface.canvas() as *const Canvas as *mut Canvas) }
            }
        }
    }

    pub fn size(&mut self) -> (i32, i32) {
        let info = match &mut self.backend {
            RenderSurfaceBackend::CpuRaster(surface) => surface.image_info(),
            #[cfg(target_os = "ios")]
            RenderSurfaceBackend::Metal { surface, .. } => surface.image_info(),
            #[cfg(target_os = "android")]
            RenderSurfaceBackend::Vulkan { surface, .. } => surface.image_info(),
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
            #[cfg(target_os = "ios")]
            RenderSurfaceBackend::Metal { .. } => None,
            #[cfg(target_os = "android")]
            RenderSurfaceBackend::Vulkan { .. } => None,
        }
    }

    pub fn flush(&mut self) {
        match &mut self.backend {
            RenderSurfaceBackend::CpuRaster(_) => {}
            #[cfg(target_os = "ios")]
            RenderSurfaceBackend::Metal { context, .. } => {
                context.flush_and_submit();
            }
            #[cfg(target_os = "android")]
            RenderSurfaceBackend::Vulkan { context, .. } => {
                context.flush_and_submit();
            }
        }
    }
}

pub struct SkiaRenderer {
    font_collection: textlayout::FontCollection,
    scale_factor: f32,
    theme: Theme,
}

impl SkiaRenderer {
    pub fn new() -> Self {
        let mut font_collection = textlayout::FontCollection::new();
        font_collection.set_default_font_manager(FontMgr::default(), None);
        Self {
            font_collection,
            scale_factor: 1.0,
            theme: Theme::default(),
        }
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
        let style = self.resolve_style(&node.kind, &node.style);
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
        let progress = node.value.as_ref().and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
        
        Ok(LayerNode {
            kind: node.kind.clone(),
            bounds,
            text: node.text.clone(),
            action: node.action.clone(),
            style,
            children,
            icon: node.icon.clone(),
            progress,
        })
    }

    fn resolve_style(&self, kind: &str, overrides: &Style) -> ResolvedStyle {
        let base = match kind {
            "button" => Style {
                background: Some(ColorSpec::Named("primary".into())),
                color: Some(ColorSpec::Named("primary_foreground".into())),
                padding: Some(self.theme.spacing.md),
                radius: Some(self.theme.radii.md),
                gap: Some(self.theme.spacing.sm),
            },
            "text" => Style {
                background: None,
                color: Some(ColorSpec::Named("foreground".into())),
                padding: None,
                radius: None,
                gap: None,
            },
            _ => Style::default(),
        };
        
        let merged = base.merged_with(overrides);
        
        let background = merged.background
            .map(|c| self.color_from_spec(&c))
            .unwrap_or(Color::TRANSPARENT);
        let color = merged.color
            .map(|c| self.color_from_spec(&c))
            .unwrap_or(self.theme.palette.foreground);
        let radius = merged.radius.unwrap_or(0.0);
        let padding = merged.padding.unwrap_or(self.theme.spacing.sm);
        let gap = merged.gap.unwrap_or(self.theme.spacing.sm);
        
        ResolvedStyle { background, color, radius, padding, gap }
    }

    fn color_from_spec(&self, spec: &ColorSpec) -> Color {
        match spec {
            ColorSpec::Named(name) => self.theme.resolve_color(name),
            ColorSpec::Rgba(r, g, b, a) => Color::from_argb(*a, *r, *g, *b),
        }
    }

    fn draw_layer(&self, layer: &LayerNode, canvas: &mut Canvas, hits: &mut Vec<ActionHit>) -> Result<()> {
        match layer.kind.as_str() {
            "text" => {
                if let Some(ref text) = layer.text {
                    self.draw_text_colored(
                        canvas,
                        text,
                        Point::new(layer.bounds.left(), layer.bounds.top() + layer.style.padding),
                        layer.style.color,
                    );
                }
            }
            "button" => {
                self.draw_rounded_rect(canvas, layer.bounds, layer.style.radius, &layer.style.background);
                if let Some(ref text) = layer.text {
                    let text_x = layer.bounds.left() + layer.style.padding;
                    let text_y = layer.bounds.top() + layer.bounds.height() / 2.0 + self.theme.typography.base / 3.0;
                    self.draw_text_colored(canvas, text, Point::new(text_x, text_y), layer.style.color);
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
            "input" | "textarea" => {
                self.draw_rounded_rect(canvas, layer.bounds, layer.style.radius, &self.theme.palette.input);
                let text_x = layer.bounds.left() + layer.style.padding;
                let text_y = layer.bounds.top() + layer.bounds.height() / 2.0 + self.theme.typography.base / 3.0;
                if let Some(ref text) = layer.text {
                    self.draw_text_colored(canvas, text, Point::new(text_x, text_y), layer.style.color);
                } else {
                    self.draw_text_colored(
                        canvas,
                        &layer.text.clone().unwrap_or_default(),
                        Point::new(text_x, text_y),
                        self.theme.palette.muted_foreground,
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
            "checkbox" | "switch" => {
                let size = 20.0 * self.scale_factor;
                let check_rect = Rect::from_xywh(layer.bounds.left(), layer.bounds.top(), size, size);
                let bg = if layer.text.as_deref() == Some("true") {
                    self.theme.palette.primary
                } else {
                    self.theme.palette.input
                };
                self.draw_rounded_rect(canvas, check_rect, layer.style.radius, &bg);
                if let Some(ref action) = layer.action {
                    hits.push(ActionHit {
                        action: action.clone(),
                        x: layer.bounds.left(),
                        y: layer.bounds.top(),
                        w: size,
                        h: size,
                    });
                }
            }
            "badge" => {
                self.draw_rounded_rect(canvas, layer.bounds, layer.style.radius, &layer.style.background);
                if let Some(ref text) = layer.text {
                    let text_x = layer.bounds.left() + layer.style.padding;
                    let text_y = layer.bounds.top() + layer.style.padding + self.theme.typography.sm;
                    self.draw_text_colored_sized(canvas, text, Point::new(text_x, text_y), layer.style.color, self.theme.typography.sm);
                }
            }
            "separator" => {
                let mut paint = Paint::default();
                paint.set_color(self.theme.palette.border);
                paint.set_stroke_width(1.0);
                paint.set_anti_alias(true);
                let mid_y = layer.bounds.top() + layer.bounds.height() / 2.0;
                canvas.draw_line(
                    Point::new(layer.bounds.left(), mid_y),
                    Point::new(layer.bounds.right(), mid_y),
                    &paint,
                );
            }
            "card" | "card_header" | "card_footer" => {
                self.draw_rounded_rect(canvas, layer.bounds, layer.style.radius, &layer.style.background);
                let mut paint = Paint::default();
                paint.set_color(self.theme.palette.border);
                paint.set_stroke_width(1.0);
                paint.set_style(skia_safe::PaintStyle::Stroke);
                paint.set_anti_alias(true);
                let rrect = skia_safe::RRect::new_rect_xy(layer.bounds, layer.style.radius, layer.style.radius);
                canvas.draw_rrect(rrect, &paint);
            }
            "spinner" => {
                let center = Point::new(
                    layer.bounds.left() + layer.bounds.width() / 2.0,
                    layer.bounds.top() + layer.bounds.height() / 2.0,
                );
                let radius = layer.bounds.width().min(layer.bounds.height()) / 2.0 - 2.0;
                let mut paint = Paint::default();
                paint.set_color(self.theme.palette.primary);
                paint.set_stroke_width(2.0);
                paint.set_style(skia_safe::PaintStyle::Stroke);
                paint.set_anti_alias(true);
                canvas.draw_circle(center, radius, &paint);
            }
            "progress" => {
                self.draw_rounded_rect(canvas, layer.bounds, layer.style.radius, &self.theme.palette.muted);
                let progress_width = layer.bounds.width() * (layer.progress.clamp(0.0, 1.0));
                let progress_rect = Rect::from_xywh(
                    layer.bounds.left(),
                    layer.bounds.top(),
                    progress_width,
                    layer.bounds.height(),
                );
                self.draw_rounded_rect(canvas, progress_rect, layer.style.radius, &self.theme.palette.primary);
            }
            "avatar" => {
                let size = layer.bounds.width().min(layer.bounds.height());
                let avatar_rect = Rect::from_xywh(layer.bounds.left(), layer.bounds.top(), size, size);
                self.draw_rounded_rect(canvas, avatar_rect, size / 2.0, &self.theme.palette.muted);
                if let Some(ref text) = layer.text {
                    let text_x = layer.bounds.left() + size / 2.0 - self.theme.typography.base / 2.0;
                    let text_y = layer.bounds.top() + size / 2.0 + self.theme.typography.base / 3.0;
                    self.draw_text_colored(canvas, text, Point::new(text_x, text_y), self.theme.palette.foreground);
                }
            }
            "list_item" => {
                if let Some(ref icon_name) = layer.icon {
                    if let Some(icon_path) = IconPaths::get(icon_name, 24.0) {
                        let mut paint = Paint::default();
                        paint.set_color(layer.style.color);
                        paint.set_anti_alias(true);
                        canvas.save();
                        canvas.translate((layer.bounds.left() + self.theme.spacing.sm, layer.bounds.top() + self.theme.spacing.sm));
                        canvas.draw_path(&icon_path, &paint);
                        canvas.restore();
                    }
                }
                if let Some(ref text) = layer.text {
                    let text_x = layer.bounds.left() + if layer.icon.is_some() { 40.0 } else { self.theme.spacing.sm };
                    let text_y = layer.bounds.top() + layer.bounds.height() / 2.0 + self.theme.typography.base / 3.0;
                    self.draw_text_colored(canvas, text, Point::new(text_x, text_y), layer.style.color);
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

    fn draw_rounded_rect(&self, canvas: &mut Canvas, rect: Rect, radius: f32, color: &Color) {
        let mut paint = Paint::default();
        paint.set_color(*color);
        paint.set_anti_alias(true);
        if radius > 0.0 {
            let rrect = skia_safe::RRect::new_rect_xy(rect, radius, radius);
            canvas.draw_rrect(rrect, &paint);
        } else {
            canvas.draw_rect(rect, &paint);
        }
    }

    fn draw_text(&self, canvas: &mut Canvas, text: &str, at: Point) {
        self.draw_text_colored(canvas, text, at, Color::BLACK);
    }

    fn draw_text_colored(&self, canvas: &mut Canvas, text: &str, at: Point, color: Color) {
        self.draw_text_colored_sized(canvas, text, at, color, self.theme.typography.base);
    }

    fn draw_text_colored_sized(&self, canvas: &mut Canvas, text: &str, at: Point, color: Color, size: f32) {
        let mut builder =
            textlayout::ParagraphBuilder::new(&textlayout::ParagraphStyle::default(), self.font_collection.clone());
        let mut text_style = textlayout::TextStyle::new();
        text_style.set_font_size(size * self.scale_factor);
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

                let value = table.get::<_, Option<String>>("value").ok().flatten();
                let change_action = table.get::<_, Option<String>>("on_change").ok().flatten();
                let placeholder = table.get::<_, Option<String>>("placeholder").ok().flatten();
                let disabled = table.get::<_, Option<bool>>("disabled").ok().flatten().unwrap_or(false);
                let checked = table.get::<_, Option<bool>>("checked").ok().flatten();
                let icon = table.get::<_, Option<String>>("icon").ok().flatten();

                Ok(ViewNode {
                    kind,
                    children,
                    style: Style::default(),
                    text,
                    width,
                    height,
                    action,
                    value,
                    change_action,
                    placeholder,
                    disabled,
                    checked,
                    icon,
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
