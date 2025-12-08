const std = @import("std");
const testing = std.testing;
const canvas = @import("canvas.zig");

test "Canvas basic operations" {
    var c = try canvas.Canvas.init(100, 100);
    defer c.deinit();

    try testing.expect(c.width == 100);
    try testing.expect(c.height == 100);
    try testing.expect(c.pixels.len == 10000);
}

test "Canvas clear" {
    var c = try canvas.Canvas.init(50, 50);
    defer c.deinit();

    const red = canvas.rgba(255, 0, 0, 255);
    c.clear(red);

    try testing.expect(c.pixels[0] == red);
    try testing.expect(c.pixels[2499] == red);
}

test "Canvas draw rect" {
    var c = try canvas.Canvas.init(100, 100);
    defer c.deinit();

    // Start with white background
    const white = canvas.rgba(255, 255, 255, 255);
    c.clear(white);

    // Draw red rectangle
    const red = canvas.rgba(255, 0, 0, 255);
    c.drawRect(10, 10, 20, 20, red);

    // Check corners of rectangle
    try testing.expect(c.pixels[10 * 100 + 10] == red); // Top-left
    try testing.expect(c.pixels[10 * 100 + 29] == red); // Top-right
    try testing.expect(c.pixels[29 * 100 + 10] == red); // Bottom-left
    try testing.expect(c.pixels[29 * 100 + 29] == red); // Bottom-right

    // Check area outside rectangle is still white
    try testing.expect(c.pixels[0] == white); // Top-left corner
    try testing.expect(c.pixels[9 * 100 + 9] == white); // Just before rect
}

test "Canvas save PPM" {
    var c = try canvas.Canvas.init(10, 10);
    defer c.deinit();

    const red = canvas.rgba(255, 0, 0, 255);
    c.clear(red);

    var tmp_dir = testing.tmpDir(.{});
    defer tmp_dir.cleanup();

    const file_path = try std.fs.path.join(testing.allocator, &.{"test.ppm"});
    defer testing.allocator.free(file_path);

    try c.savePng(file_path);

    // Check file exists and has content
    const file = try std.fs.cwd().openFile(file_path, .{});
    defer file.close();

    const stat = try file.stat();
    try testing.expect(stat.size > 0);

    // Clean up
    try std.fs.cwd().deleteFile(file_path);
}
