const std = @import("std");

pub const Color = packed struct(u32) {
    b: u8,
    g: u8,
    r: u8,
    a: u8,
};

pub fn rgba(r: u8, g: u8, b: u8, a: u8) u32 {
    return @as(u32, @bitCast(Color{ .r = r, .g = g, .b = b, .a = a }));
}

pub const Canvas = struct {
    width: i32,
    height: i32,
    pixels: []u32,

    pub fn init(width: i32, height: i32) !Canvas {
        const allocator = std.heap.page_allocator;
        const pixels = try allocator.alloc(u32, @as(usize, @intCast(width * height)));
        @memset(pixels, 0xffffffff); // White background

        return Canvas{ .width = width, .height = height, .pixels = pixels };
    }

    pub fn deinit(self: *Canvas) void {
        const allocator = std.heap.page_allocator;
        allocator.free(self.pixels);
    }

    pub fn clear(self: *Canvas, color: u32) void {
        @memset(self.pixels, color);
    }

    pub fn drawRect(self: *Canvas, x: f32, y: f32, w: f32, h: f32, color: u32) void {
        const start_x = @as(i32, @intFromFloat(x));
        const start_y = @as(i32, @intFromFloat(y));
        const end_x = @as(i32, @intFromFloat(x + w));
        const end_y = @as(i32, @intFromFloat(y + h));

        var py = start_y;
        while (py < end_y) : (py += 1) {
            if (py < 0 or py >= self.height) continue;

            var px = start_x;
            while (px < end_x) : (px += 1) {
                if (px < 0 or px >= self.width) continue;

                const index = @as(usize, @intCast(py * self.width + px));
                self.pixels[index] = color;
            }
        }
    }

    pub fn drawText(self: *Canvas, text: []const u8, x: f32, y: f32, size: f32, color: u32) void {
        _ = self;
        _ = text;
        _ = x;
        _ = y;
        _ = size;
        _ = color;
        // TODO: Implement text rendering (will need Skia or stb_truetype)
    }

    pub fn savePng(self: *Canvas, path: []const u8) !void {
        // For now, save as PPM (simple format)
        const file = try std.fs.cwd().createFile(path, .{});
        defer file.close();

        // Write header
        const header = try std.fmt.allocPrint(std.heap.page_allocator, "P3\n{} {}\n255\n", .{ self.width, self.height });
        defer std.heap.page_allocator.free(header);
        try file.writeAll(header);

        // Write pixels
        for (self.pixels) |pixel| {
            const color = @as(Color, @bitCast(pixel));
            const pixel_str = try std.fmt.allocPrint(std.heap.page_allocator, "{} {} {} ", .{ color.r, color.g, color.b });
            defer std.heap.page_allocator.free(pixel_str);
            try file.writeAll(pixel_str);
        }
    }
};
