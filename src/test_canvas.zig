const std = @import("std");
const canvas = @import("render/canvas.zig");

pub fn main() !void {
    var c = try canvas.Canvas.init(200, 200);
    defer c.deinit();

    // Clear to white
    c.clear(canvas.rgba(255, 255, 255, 255));

    // Draw red rectangle
    c.drawRect(50, 50, 100, 100, canvas.rgba(255, 0, 0, 255));

    // Draw blue rectangle
    c.drawRect(75, 75, 50, 50, canvas.rgba(0, 0, 255, 255));

    // Save as PPM
    try c.savePng("test_output.ppm");

    std.debug.print("Generated test_output.ppm\n", .{});
}
