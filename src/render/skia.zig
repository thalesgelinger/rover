// Skia rendering wrapper
// This will be populated with actual Skia bindings once we have the binaries

pub const Color = packed struct(u32) {
    b: u8,
    g: u8,
    r: u8,
    a: u8,
};

pub fn rgba(r: u8, g: u8, b: u8, a: u8) u32 {
    return Color{ .r = r, .g = g, .b = b, .a = a };
}

pub const Canvas = struct {
    width: i32,
    height: i32,

    pub fn init(width: i32, height: i32) !Canvas {
        // TODO: Initialize actual Skia surface
        return Canvas{ .width = width, .height = height };
    }

    pub fn deinit(self: *Canvas) void {
        _ = self;
        // TODO: Cleanup Skia surface
    }

    pub fn clear(self: *Canvas, color: u32) void {
        _ = self;
        _ = color;
        // TODO: Clear Skia canvas
    }

    pub fn drawRect(self: *Canvas, x: f32, y: f32, w: f32, h: f32, color: u32) void {
        _ = self;
        _ = x;
        _ = y;
        _ = w;
        _ = h;
        _ = color;
        // TODO: Draw rectangle with Skia
    }

    pub fn drawText(self: *Canvas, text: []const u8, x: f32, y: f32, size: f32, color: u32) void {
        _ = self;
        _ = text;
        _ = x;
        _ = y;
        _ = size;
        _ = color;
        // TODO: Draw text with Skia
    }

    pub fn savePng(self: *Canvas, path: []const u8) !void {
        _ = self;
        _ = path;
        // TODO: Encode Skia surface to PNG
        return error.NotImplemented;
    }
};
