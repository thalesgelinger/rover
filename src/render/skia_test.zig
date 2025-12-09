const std = @import("std");
const sk = @import("skia.zig");

test "Skia generates valid PNG with magic bytes" {
    var c = try sk.Canvas.init(64, 64);
    defer c.deinit();

    try c.clear(sk.rgba(0, 0, 0, 255));
    try c.drawRect(8, 8, 32, 32, sk.rgba(255, 0, 0, 255));

    const allocator = std.testing.allocator;
    var arena = std.heap.ArenaAllocator.init(allocator);
    defer arena.deinit();

    const buf = try c.snapshotPngAlloc(arena.allocator());
    try std.testing.expect(buf.len > 100); // PNG should be reasonably sized

    // Verify PNG magic bytes: 89 50 4E 47 0D 0A 1A 0A
    const png_magic = [_]u8{ 0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A };
    try std.testing.expectEqualSlices(u8, &png_magic, buf[0..8]);
}

test "Skia saves PNG file to /tmp for manual inspection" {
    var c = try sk.Canvas.init(64, 64);
    defer c.deinit();

    // Black background
    try c.clear(sk.rgba(0, 0, 0, 255));

    // Red rectangle at (8,8) size 24x24
    try c.drawRect(8, 8, 32, 32, sk.rgba(255, 0, 0, 255));

    const path = "/tmp/rover_skia_test_output.png";
    try c.savePng(path);

    // Verify file exists and has PNG magic bytes
    var file = try std.fs.openFileAbsolute(path, .{});
    defer file.close();

    var magic: [8]u8 = undefined;
    _ = try file.read(&magic);

    const png_magic = [_]u8{ 0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A };
    try std.testing.expectEqualSlices(u8, &png_magic, &magic);

    const stat = try file.stat();
    try std.testing.expect(stat.size > 100);

    std.debug.print("\n✓ PNG saved to {s} ({} bytes)\n", .{ path, stat.size });
    std.debug.print("  Expected: 64x64 black canvas with red 24x24 rectangle at (8,8)\n", .{});
    std.debug.print("  Run: open {s}\n", .{path});
}
