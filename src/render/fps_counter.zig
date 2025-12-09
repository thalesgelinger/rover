const std = @import("std");
const skia = @import("skia.zig");

pub const FpsCounter = struct {
    frame_times: [60]f64,
    index: usize,
    last_time: i64,
    frame_count: usize,

    pub fn init() FpsCounter {
        return FpsCounter{
            .frame_times = [_]f64{0.0} ** 60,
            .index = 0,
            .last_time = std.time.milliTimestamp(),
            .frame_count = 0,
        };
    }

    pub fn recordFrame(self: *FpsCounter) void {
        const current_time = std.time.milliTimestamp();
        const delta = @as(f64, @floatFromInt(current_time - self.last_time));

        self.frame_times[self.index] = delta;
        self.index = (self.index + 1) % 60;
        self.frame_count += 1;
        self.last_time = current_time;
    }

    pub fn getFps(self: *FpsCounter) f64 {
        if (self.frame_count == 0) return 0.0;

        var sum: f64 = 0.0;
        const count = @min(self.frame_count, 60);

        var i: usize = 0;
        while (i < count) : (i += 1) {
            sum += self.frame_times[i];
        }

        const avg_ms = sum / @as(f64, @floatFromInt(count));
        if (avg_ms <= 0.0) return 0.0;

        return 1000.0 / avg_ms;
    }

    pub fn getFrameTime(self: *FpsCounter) f64 {
        if (self.frame_count == 0) return 0.0;

        var sum: f64 = 0.0;
        const count = @min(self.frame_count, 60);

        var i: usize = 0;
        while (i < count) : (i += 1) {
            sum += self.frame_times[i];
        }

        return sum / @as(f64, @floatFromInt(count));
    }

    pub fn draw(self: *FpsCounter, canvas: *skia.MetalCanvas, width: i32) !void {
        const fps = self.getFps();
        const frame_time = self.getFrameTime();

        // Draw semi-transparent background
        const bg_x: f32 = @as(f32, @floatFromInt(width)) - 150.0;
        const bg_y: f32 = 10.0;
        const bg_w: f32 = 140.0;
        const bg_h: f32 = 50.0;

        try canvas.drawRect(bg_x, bg_y, bg_w, bg_h, skia.rgba(0, 0, 0, 180));

        // Format FPS text
        var fps_buf: [64]u8 = undefined;
        const fps_text = try std.fmt.bufPrint(&fps_buf, "FPS: {d:.1}", .{fps});

        // Format frame time text
        var frame_buf: [64]u8 = undefined;
        const frame_text = try std.fmt.bufPrint(&frame_buf, "Frame: {d:.1}ms", .{frame_time});

        // Draw FPS text
        const text_x = bg_x + 10.0;
        try canvas.drawText(fps_text, text_x, bg_y + 20.0, skia.rgba(255, 255, 255, 255));

        // Draw frame time text
        try canvas.drawText(frame_text, text_x, bg_y + 40.0, skia.rgba(255, 255, 255, 255));
    }
};
