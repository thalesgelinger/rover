const std = @import("std");

// Minimal Skia C API bindings (CPU raster surface + PNG encode).
// Expects libskia built with the C API enabled. No GPU surface here.

pub const sk_color_t = u32;

pub const sk_colortype_t = enum(i32) {
    unknown,
    rgba8888,
    bgra8888,
    alpha8,
};

pub const sk_alphatype_t = enum(i32) {
    opaque_alpha,
    premul_alpha,
    unpremul_alpha,
};

pub const sk_imageinfo_t = extern struct {
    width: i32,
    height: i32,
    colorType: sk_colortype_t,
    alphaType: sk_alphatype_t,
};

pub const sk_point_t = extern struct {
    x: f32,
    y: f32,
};

pub const sk_rect_t = extern struct {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
};

pub const sk_matrix_t = extern struct {
    mat: [9]f32,
};

pub const sk_canvas_t = opaque {};
pub const sk_data_t = opaque {};
pub const sk_image_t = opaque {};
pub const sk_maskfilter_t = opaque {};
pub const sk_paint_t = opaque {};
pub const sk_path_t = opaque {};
pub const sk_picture_t = opaque {};
pub const sk_picture_recorder_t = opaque {};
pub const sk_shader_t = opaque {};
pub const sk_surface_t = opaque {};

extern fn sk_colortype_get_default_8888() sk_colortype_t;
extern fn sk_surface_new_raster(info: *const sk_imageinfo_t) ?*sk_surface_t;
extern fn sk_surface_new_raster_direct(info: *const sk_imageinfo_t, pixels: *anyopaque, rowBytes: usize) ?*sk_surface_t;
extern fn sk_surface_unref(surface: *sk_surface_t) void;
extern fn sk_surface_get_canvas(surface: *sk_surface_t) ?*sk_canvas_t;
extern fn sk_surface_new_image_snapshot(surface: *sk_surface_t) ?*sk_image_t;

extern fn sk_canvas_draw_paint(canvas: *sk_canvas_t, paint: *sk_paint_t) void;
extern fn sk_canvas_draw_rect(canvas: *sk_canvas_t, rect: *const sk_rect_t, paint: *sk_paint_t) void;
extern fn sk_canvas_draw_image(canvas: *sk_canvas_t, image: *const sk_image_t, x: f32, y: f32, paint: *const sk_paint_t) void;
extern fn sk_canvas_draw_image_rect(canvas: *sk_canvas_t, image: *const sk_image_t, src: *const sk_rect_t, dst: *const sk_rect_t, paint: *const sk_paint_t) void;
extern fn sk_canvas_draw_picture(canvas: *sk_canvas_t, picture: *const sk_picture_t, matrix: *const sk_matrix_t, paint: *const sk_paint_t) void;

extern fn sk_paint_new() ?*sk_paint_t;
extern fn sk_paint_delete(paint: *sk_paint_t) void;
extern fn sk_paint_is_antialias(paint: *const sk_paint_t) bool;
extern fn sk_paint_set_antialias(paint: *sk_paint_t, value: bool) void;
extern fn sk_paint_get_color(paint: *const sk_paint_t) sk_color_t;
extern fn sk_paint_set_color(paint: *sk_paint_t, color: sk_color_t) void;
extern fn sk_paint_is_stroke(paint: *const sk_paint_t) bool;
extern fn sk_paint_set_stroke(paint: *sk_paint_t, value: bool) void;
extern fn sk_paint_get_stroke_width(paint: *const sk_paint_t) f32;
extern fn sk_paint_set_stroke_width(paint: *sk_paint_t, width: f32) void;
extern fn sk_paint_get_stroke_miter(paint: *const sk_paint_t) f32;
extern fn sk_paint_set_stroke_miter(paint: *sk_paint_t, miter: f32) void;
extern fn sk_paint_get_stroke_cap(paint: *const sk_paint_t) i32;
extern fn sk_paint_set_stroke_cap(paint: *sk_paint_t, cap: i32) void;
extern fn sk_paint_get_stroke_join(paint: *const sk_paint_t) i32;
extern fn sk_paint_set_stroke_join(paint: *sk_paint_t, join: i32) void;
extern fn sk_paint_set_shader(paint: *sk_paint_t, shader: *sk_shader_t) void;
extern fn sk_paint_set_maskfilter(paint: *sk_paint_t, filter: *sk_maskfilter_t) void;

extern fn sk_image_encode(image: *const sk_image_t) ?*sk_data_t;
extern fn sk_image_ref(image: *const sk_image_t) void;
extern fn sk_image_unref(image: *const sk_image_t) void;
extern fn sk_image_get_width(image: *const sk_image_t) c_int;
extern fn sk_image_get_height(image: *const sk_image_t) c_int;
extern fn sk_image_get_unique_id(image: *const sk_image_t) u32;

extern fn sk_data_ref(data: *const sk_data_t) void;
extern fn sk_data_unref(data: *const sk_data_t) void;
extern fn sk_data_get_size(data: *const sk_data_t) usize;
extern fn sk_data_get_data(data: *const sk_data_t) ?*const anyopaque;

pub fn rgba(r: u8, g: u8, b: u8, a: u8) sk_color_t {
    return (@as(sk_color_t, a) << 24) | (@as(sk_color_t, r) << 16) | (@as(sk_color_t, g) << 8) | @as(sk_color_t, b);
}

pub const SkiaError = error{
    SurfaceCreateFailed,
    CanvasUnavailable,
    PaintCreateFailed,
    SnapshotFailed,
    EncodeFailed,
};

pub const Canvas = struct {
    surface: *sk_surface_t,
    canvas: *sk_canvas_t,
    width: i32,
    height: i32,

    pub fn init(width: i32, height: i32) !Canvas {
        var info = sk_imageinfo_t{
            .width = width,
            .height = height,
            .colorType = sk_colortype_get_default_8888(),
            .alphaType = sk_alphatype_t.premul_alpha,
        };

        const surface = sk_surface_new_raster(&info) orelse return SkiaError.SurfaceCreateFailed;
        const canvas_ptr = sk_surface_get_canvas(surface) orelse return SkiaError.CanvasUnavailable;

        return Canvas{ .surface = surface, .canvas = canvas_ptr, .width = width, .height = height };
    }

    pub fn initWithPixels(width: i32, height: i32, pixels: *anyopaque, row_bytes: usize) !Canvas {
        var info = sk_imageinfo_t{
            .width = width,
            .height = height,
            .colorType = sk_colortype_get_default_8888(),
            .alphaType = sk_alphatype_t.premul_alpha,
        };

        const surface = sk_surface_new_raster_direct(&info, pixels, row_bytes) orelse return SkiaError.SurfaceCreateFailed;
        const canvas_ptr = sk_surface_get_canvas(surface);
        if (canvas_ptr == null) return SkiaError.CanvasUnavailable;

        return Canvas{ .surface = surface, .canvas = canvas_ptr, .width = width, .height = height };
    }

    pub fn deinit(self: *Canvas) void {
        sk_surface_unref(self.surface);
    }

    pub fn clear(self: *Canvas, color: sk_color_t) !void {
        var paint = try Paint.init();
        defer paint.deinit();
        sk_paint_set_color(paint.handle, color);
        sk_canvas_draw_paint(self.canvas, paint.handle);
    }

    pub fn drawRect(self: *Canvas, x: f32, y: f32, w: f32, h: f32, color: sk_color_t) !void {
        var paint = try Paint.init();
        defer paint.deinit();
        sk_paint_set_color(paint.handle, color);
        var rect = sk_rect_t{ .left = x, .top = y, .right = x + w, .bottom = y + h };
        sk_canvas_draw_rect(self.canvas, &rect, paint.handle);
    }

    pub fn snapshotPngAlloc(self: *Canvas, allocator: std.mem.Allocator) ![]u8 {
        const image = sk_surface_new_image_snapshot(self.surface) orelse return SkiaError.SnapshotFailed;
        defer sk_image_unref(image);
        const data = sk_image_encode(image) orelse return SkiaError.EncodeFailed;
        defer sk_data_unref(data);

        const size = sk_data_get_size(data);
        const raw = sk_data_get_data(data) orelse return SkiaError.EncodeFailed;
        if (size == 0) return SkiaError.EncodeFailed;

        const slice = try allocator.alloc(u8, size);
        @memcpy(slice, @as([*]const u8, @ptrCast(raw))[0..size]);
        return slice;
    }

    pub fn savePng(self: *Canvas, path: []const u8) !void {
        var file = try std.fs.cwd().createFile(path, .{});
        defer file.close();

        var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
        defer arena.deinit();
        const buf = try self.snapshotPngAlloc(arena.allocator());
        try file.writeAll(buf);
    }
};

pub const Paint = struct {
    handle: *sk_paint_t,

    pub fn init() !Paint {
        const paint = sk_paint_new() orelse return SkiaError.PaintCreateFailed;
        sk_paint_set_antialias(paint, true);
        return Paint{ .handle = paint };
    }

    pub fn deinit(self: *Paint) void {
        sk_paint_delete(self.handle);
    }
};

// Metal GPU support
pub const gr_direct_context_t = opaque {};

extern fn gr_direct_context_make_metal(device: *anyopaque, queue: *anyopaque) ?*gr_direct_context_t;
extern fn gr_direct_context_unref(context: *gr_direct_context_t) void;
extern fn gr_direct_context_flush_and_submit(context: *gr_direct_context_t, sync: bool) void;
extern fn sk_surface_wrap_metal_layer(context: *gr_direct_context_t, layer: *anyopaque, width: i32, height: i32) ?*sk_surface_t;
extern fn sk_surface_flush_and_submit(surface: *sk_surface_t) void;
extern fn sk_surface_present_to_layer(context: *gr_direct_context_t, surface: *sk_surface_t, layer: *anyopaque) void;
extern fn sk_canvas_draw_simple_text(canvas: *sk_canvas_t, text: [*]const u8, len: usize, x: f32, y: f32, paint: *const sk_paint_t) void;

pub const MetalContext = struct {
    context: *gr_direct_context_t,
    device: *anyopaque,
    queue: *anyopaque,

    pub fn init(device: *anyopaque, queue: *anyopaque) !MetalContext {
        const context = gr_direct_context_make_metal(device, queue) orelse return SkiaError.SurfaceCreateFailed;
        return MetalContext{
            .context = context,
            .device = device,
            .queue = queue,
        };
    }

    pub fn deinit(self: *MetalContext) void {
        gr_direct_context_unref(self.context);
    }

    pub fn flushAndSubmit(self: *MetalContext, sync: bool) void {
        gr_direct_context_flush_and_submit(self.context, sync);
    }
};

pub const MetalCanvas = struct {
    surface: *sk_surface_t,
    canvas: *sk_canvas_t,
    width: i32,
    height: i32,
    layer: *anyopaque,
    context: *gr_direct_context_t,

    pub fn init(metal_ctx: *MetalContext, layer: *anyopaque, width: i32, height: i32) !MetalCanvas {
        const surface = sk_surface_wrap_metal_layer(metal_ctx.context, layer, width, height) orelse return SkiaError.SurfaceCreateFailed;
        const canvas_ptr = sk_surface_get_canvas(surface) orelse return SkiaError.CanvasUnavailable;

        return MetalCanvas{
            .surface = surface,
            .canvas = canvas_ptr,
            .width = width,
            .height = height,
            .layer = layer,
            .context = metal_ctx.context,
        };
    }

    pub fn deinit(self: *MetalCanvas) void {
        sk_surface_unref(self.surface);
    }

    pub fn clear(self: *MetalCanvas, color: sk_color_t) !void {
        var paint = try Paint.init();
        defer paint.deinit();
        sk_paint_set_color(paint.handle, color);
        sk_canvas_draw_paint(self.canvas, paint.handle);
    }

    pub fn drawRect(self: *MetalCanvas, x: f32, y: f32, w: f32, h: f32, color: sk_color_t) !void {
        var paint = try Paint.init();
        defer paint.deinit();
        sk_paint_set_color(paint.handle, color);
        var rect = sk_rect_t{ .left = x, .top = y, .right = x + w, .bottom = y + h };
        sk_canvas_draw_rect(self.canvas, &rect, paint.handle);
    }

    pub fn drawText(self: *MetalCanvas, text: []const u8, x: f32, y: f32, color: sk_color_t) !void {
        var paint = try Paint.init();
        defer paint.deinit();
        sk_paint_set_color(paint.handle, color);
        sk_canvas_draw_simple_text(self.canvas, text.ptr, text.len, x, y, paint.handle);
    }

    pub fn flush(self: *MetalCanvas) void {
        sk_surface_flush_and_submit(self.surface);
    }

    pub fn present(self: *MetalCanvas) void {
        sk_surface_present_to_layer(self.context, self.surface, self.layer);
    }
};
