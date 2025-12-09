const std = @import("std");

/// Abstract window interface - platform-agnostic
pub const Window = struct {
    width: i32,
    height: i32,
    should_close: bool,
    needs_redraw: bool,
    backend: *anyopaque, // Platform-specific implementation

    /// Create a new window with the given dimensions
    pub fn create(width: i32, height: i32, title: []const u8) !*Window {
        return @import("macos.zig").createWindow(width, height, title);
    }

    /// Destroy the window and free resources
    pub fn destroy(self: *Window, allocator: std.mem.Allocator) void {
        @import("macos.zig").destroyWindow(self, allocator);
    }

    /// Check if window should close
    pub fn shouldClose(self: *Window) bool {
        return self.should_close;
    }

    /// Poll for events (non-blocking)
    pub fn pollEvents(self: *Window) void {
        @import("macos.zig").pollEvents(self);
    }

    /// Mark window as needing redraw
    pub fn markDirty(self: *Window) void {
        self.needs_redraw = true;
    }

    /// Check if window needs redraw
    pub fn needsRedraw(self: *Window) bool {
        return self.needs_redraw;
    }

    /// Clear redraw flag
    pub fn clearRedraw(self: *Window) void {
        self.needs_redraw = false;
    }

    /// Get Metal layer (CAMetalLayer* on macOS)
    pub fn getMetalLayer(self: *Window) ?*anyopaque {
        return @import("macos.zig").getMetalLayer(self);
    }

    /// Get Metal device
    pub fn getMetalDevice(self: *Window) ?*anyopaque {
        return @import("macos.zig").getMetalDevice(self);
    }

    /// Get Metal command queue
    pub fn getMetalQueue(self: *Window) ?*anyopaque {
        return @import("macos.zig").getMetalQueue(self);
    }

    /// Get drawable size (accounts for retina scaling)
    pub const DrawableSize = struct { width: i32, height: i32 };
    pub fn getDrawableSize(self: *Window) DrawableSize {
        return @import("macos.zig").getDrawableSize(self);
    }
};
