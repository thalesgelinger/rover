const std = @import("std");
const Window = @import("window.zig").Window;

// Objective-C runtime bindings
extern "c" fn objc_getClass(name: [*:0]const u8) ?*anyopaque;
extern "c" fn sel_registerName(str: [*:0]const u8) ?*anyopaque;
extern "c" fn objc_msgSend() void;

// Metal C API (minimal subset)
extern "c" fn MTLCreateSystemDefaultDevice() ?*anyopaque;

// Helper for Objective-C message sending
fn msgSend(comptime ReturnType: type, target: anytype, selector: *anyopaque, args: anytype) ReturnType {
    const FnType = switch (args.len) {
        0 => *const fn (*anyopaque, *anyopaque) callconv(.c) ReturnType,
        1 => *const fn (*anyopaque, *anyopaque, @TypeOf(args[0])) callconv(.c) ReturnType,
        2 => *const fn (*anyopaque, *anyopaque, @TypeOf(args[0]), @TypeOf(args[1])) callconv(.c) ReturnType,
        3 => *const fn (*anyopaque, *anyopaque, @TypeOf(args[0]), @TypeOf(args[1]), @TypeOf(args[2])) callconv(.c) ReturnType,
        4 => *const fn (*anyopaque, *anyopaque, @TypeOf(args[0]), @TypeOf(args[1]), @TypeOf(args[2]), @TypeOf(args[3])) callconv(.c) ReturnType,
        5 => *const fn (*anyopaque, *anyopaque, @TypeOf(args[0]), @TypeOf(args[1]), @TypeOf(args[2]), @TypeOf(args[3]), @TypeOf(args[4])) callconv(.c) ReturnType,
        else => @compileError("Too many msgSend args"),
    };

    const func = @as(FnType, @ptrCast(&objc_msgSend));
    const target_ptr = if (@TypeOf(target) == *anyopaque) target else @as(*anyopaque, @ptrCast(target));

    return switch (args.len) {
        0 => func(target_ptr, selector),
        1 => func(target_ptr, selector, args[0]),
        2 => func(target_ptr, selector, args[0], args[1]),
        3 => func(target_ptr, selector, args[0], args[1], args[2]),
        4 => func(target_ptr, selector, args[0], args[1], args[2], args[3]),
        5 => func(target_ptr, selector, args[0], args[1], args[2], args[3], args[4]),
        else => unreachable,
    };
}

// Objective-C structure helpers
const CGFloat = f64;
const NSUInteger = c_ulong;

const CGPoint = extern struct {
    x: CGFloat,
    y: CGFloat,
};

const CGSize = extern struct {
    width: CGFloat,
    height: CGFloat,
};

const CGRect = extern struct {
    origin: CGPoint,
    size: CGSize,
};

// NSWindow style mask
const NSWindowStyleMask = enum(NSUInteger) {
    Borderless = 0,
    Titled = 1 << 0,
    Closable = 1 << 1,
    Miniaturizable = 1 << 2,
    Resizable = 1 << 3,
};

const NSBackingStoreType = enum(NSUInteger) {
    Retained = 0,
    Nonretained = 1,
    Buffered = 2,
};

// macOS-specific window data
const MacOSWindow = struct {
    ns_window: *anyopaque,
    metal_layer: *anyopaque,
    metal_device: *anyopaque,
    metal_queue: *anyopaque,
    window: Window,
};

pub fn createWindow(width: i32, height: i32, title: []const u8) !*Window {
    const allocator = std.heap.c_allocator;

    // Initialize NSApplication if needed
    const NSApplication = objc_getClass("NSApplication") orelse return error.CocoaInitFailed;
    const sharedApplication = sel_registerName("sharedApplication") orelse return error.CocoaInitFailed;
    const ns_app = msgSend(*anyopaque, NSApplication, sharedApplication, .{});

    const setActivationPolicy = sel_registerName("setActivationPolicy:") orelse return error.CocoaInitFailed;
    _ = msgSend(void, ns_app, setActivationPolicy, .{@as(c_long, 0)}); // NSApplicationActivationPolicyRegular

    // Create NSWindow
    const NSWindow = objc_getClass("NSWindow") orelse return error.CocoaInitFailed;
    const alloc = sel_registerName("alloc") orelse return error.CocoaInitFailed;
    const window_alloc = msgSend(*anyopaque, NSWindow, alloc, .{});

    const frame = CGRect{
        .origin = .{ .x = 100, .y = 100 },
        .size = .{ .width = @as(CGFloat, @floatFromInt(width)), .height = @as(CGFloat, @floatFromInt(height)) },
    };

    // Window style: titled + closable + resizable
    const style_mask: NSUInteger = @intFromEnum(NSWindowStyleMask.Titled) | @intFromEnum(NSWindowStyleMask.Closable) | @intFromEnum(NSWindowStyleMask.Resizable) | @intFromEnum(NSWindowStyleMask.Miniaturizable);
    const backing: NSUInteger = @intFromEnum(NSBackingStoreType.Buffered);

    const initWithContentRect = sel_registerName("initWithContentRect:styleMask:backing:defer:") orelse return error.CocoaInitFailed;
    const ns_window = msgSend(*anyopaque, window_alloc, initWithContentRect, .{ frame, style_mask, backing, @as(u8, 0) });

    // Set window title
    const setTitle = sel_registerName("setTitle:") orelse return error.CocoaInitFailed;
    const NSString = objc_getClass("NSString") orelse return error.CocoaInitFailed;
    const stringWithUTF8String = sel_registerName("stringWithUTF8String:") orelse return error.CocoaInitFailed;

    // Create null-terminated title
    const title_z = try allocator.dupeZ(u8, title);
    defer allocator.free(title_z);

    const ns_title = msgSend(*anyopaque, NSString, stringWithUTF8String, .{title_z.ptr});
    _ = msgSend(void, ns_window, setTitle, .{ns_title});

    // Make window visible
    const makeKeyAndOrderFront = sel_registerName("makeKeyAndOrderFront:") orelse return error.CocoaInitFailed;
    _ = msgSend(void, ns_window, makeKeyAndOrderFront, .{@as(?*anyopaque, null)});

    // Activate app
    const activateIgnoringOtherApps = sel_registerName("activateIgnoringOtherApps:") orelse return error.CocoaInitFailed;
    _ = msgSend(void, ns_app, activateIgnoringOtherApps, .{@as(u8, 1)});

    // Create Metal device
    const metal_device = MTLCreateSystemDefaultDevice() orelse return error.MetalInitFailed;

    // Create Metal command queue
    const newCommandQueue = sel_registerName("newCommandQueue") orelse return error.MetalInitFailed;
    const metal_queue = msgSend(*anyopaque, metal_device, newCommandQueue, .{});

    // Create CAMetalLayer
    const CAMetalLayer = objc_getClass("CAMetalLayer") orelse return error.MetalInitFailed;
    const layer_alloc = msgSend(*anyopaque, CAMetalLayer, alloc, .{});
    const init = sel_registerName("init") orelse return error.MetalInitFailed;
    const metal_layer = msgSend(*anyopaque, layer_alloc, init, .{});

    // Configure metal layer
    const setDevice = sel_registerName("setDevice:") orelse return error.MetalInitFailed;
    _ = msgSend(void, metal_layer, setDevice, .{metal_device});

    const setPixelFormat = sel_registerName("setPixelFormat:") orelse return error.MetalInitFailed;
    _ = msgSend(void, metal_layer, setPixelFormat, .{@as(c_ulong, 80)}); // MTLPixelFormatBGRA8Unorm

    const setFramebufferOnly = sel_registerName("setFramebufferOnly:") orelse return error.MetalInitFailed;
    _ = msgSend(void, metal_layer, setFramebufferOnly, .{@as(u8, 0)});

    const setDrawableSize = sel_registerName("setDrawableSize:") orelse return error.MetalInitFailed;
    const scale_sel = sel_registerName("backingScaleFactor") orelse return error.CocoaInitFailed;
    const scale = msgSend(CGFloat, ns_window, scale_sel, .{});
    const drawable_size_init = CGSize{ .width = frame.size.width * scale, .height = frame.size.height * scale };

    // Set layer frame to match window size
    const setFrame = sel_registerName("setFrame:") orelse return error.MetalInitFailed;
    _ = msgSend(void, metal_layer, setFrame, .{frame});
    _ = msgSend(void, metal_layer, setDrawableSize, .{drawable_size_init});

    // Get content view and set layer
    const contentView = sel_registerName("contentView") orelse return error.CocoaInitFailed;
    const view = msgSend(*anyopaque, ns_window, contentView, .{});

    const setWantsLayer = sel_registerName("setWantsLayer:") orelse return error.CocoaInitFailed;
    _ = msgSend(void, view, setWantsLayer, .{@as(u8, 1)});

    const setLayer = sel_registerName("setLayer:") orelse return error.CocoaInitFailed;
    _ = msgSend(void, view, setLayer, .{metal_layer});

    // Allocate MacOSWindow struct
    const macos_window = try allocator.create(MacOSWindow);
    macos_window.* = .{
        .ns_window = ns_window,
        .metal_layer = metal_layer,
        .metal_device = metal_device,
        .metal_queue = metal_queue,
        .window = .{
            .width = @max(1, @as(i32, @intFromFloat(drawable_size_init.width))),
            .height = @max(1, @as(i32, @intFromFloat(drawable_size_init.height))),
            .should_close = false,
            .needs_redraw = true, // Initial draw
            .backend = undefined,
        },
    };

    macos_window.window.backend = macos_window;

    return &macos_window.window;
}

pub fn destroyWindow(window: *Window, _: std.mem.Allocator) void {
    const macos_window: *MacOSWindow = @ptrCast(@alignCast(window.backend));

    const close = sel_registerName("close") orelse return;
    _ = msgSend(void, macos_window.ns_window, close, .{});

    // Note: Metal objects are owned by Objective-C runtime, don't release manually here

    const allocator = std.heap.c_allocator;
    allocator.destroy(macos_window);
}

pub fn pollEvents(window: *Window) void {
    const NSApplication = objc_getClass("NSApplication") orelse return;
    const sharedApplication = sel_registerName("sharedApplication") orelse return;
    const ns_app = msgSend(*anyopaque, NSApplication, sharedApplication, .{});

    const nextEventMatchingMask = sel_registerName("nextEventMatchingMask:untilDate:inMode:dequeue:") orelse return;
    const sendEvent = sel_registerName("sendEvent:") orelse return;
    const updateWindows = sel_registerName("updateWindows") orelse return;
    const isVisible_sel = sel_registerName("isVisible") orelse return;
    const contentView_sel = sel_registerName("contentView") orelse return;
    const bounds_sel = sel_registerName("bounds") orelse return;
    const setFrame_sel = sel_registerName("setFrame:") orelse return;
    const setDrawableSize_sel = sel_registerName("setDrawableSize:") orelse return;
    const scale_sel = sel_registerName("backingScaleFactor") orelse return;

    const NSDefaultRunLoopMode = blk: {
        const NSString = objc_getClass("NSString") orelse return;
        const stringWithUTF8String = sel_registerName("stringWithUTF8String:") orelse return;
        break :blk msgSend(*anyopaque, NSString, stringWithUTF8String, .{"kCFRunLoopDefaultMode"});
    };

    const macos_window: *MacOSWindow = @ptrCast(@alignCast(window.backend));

    // Poll all pending events
    while (true) {
        const event = msgSend(?*anyopaque, ns_app, nextEventMatchingMask, .{ @as(c_ulong, 0xFFFFFFFF), @as(?*anyopaque, null), NSDefaultRunLoopMode, @as(u8, 1) });

        if (event == null) break;

        _ = msgSend(void, ns_app, sendEvent, .{event});

        // Check if window should close
        const visible = msgSend(u8, macos_window.ns_window, isVisible_sel, .{});
        if (visible == 0) {
            window.should_close = true;
        }
    }

    _ = msgSend(void, ns_app, updateWindows, .{});

    // Update window size and mark redraw if it changed
    const view = msgSend(*anyopaque, macos_window.ns_window, contentView_sel, .{});
    if (@intFromPtr(view) != 0) {
        const bounds = msgSend(CGRect, view, bounds_sel, .{});
        const scale = msgSend(CGFloat, macos_window.ns_window, scale_sel, .{});
        const point_width = bounds.size.width;
        const point_height = bounds.size.height;
        const pixel_width = @max(1, @as(i32, @intFromFloat(point_width * scale)));
        const pixel_height = @max(1, @as(i32, @intFromFloat(point_height * scale)));

        if (pixel_width != window.width or pixel_height != window.height) {
            window.width = pixel_width;
            window.height = pixel_height;
            window.markDirty();

            // Update metal layer frame and drawable size
            _ = msgSend(void, macos_window.metal_layer, setFrame_sel, .{bounds});
            const drawable_size = CGSize{
                .width = @as(CGFloat, @floatFromInt(pixel_width)),
                .height = @as(CGFloat, @floatFromInt(pixel_height)),
            };
            _ = msgSend(void, macos_window.metal_layer, setDrawableSize_sel, .{drawable_size});
        }
    }
}

pub fn getMetalLayer(window: *Window) ?*anyopaque {
    const macos_window: *MacOSWindow = @ptrCast(@alignCast(window.backend));
    return macos_window.metal_layer;
}

pub fn getMetalDevice(window: *Window) ?*anyopaque {
    const macos_window: *MacOSWindow = @ptrCast(@alignCast(window.backend));
    return macos_window.metal_device;
}

pub fn getMetalQueue(window: *Window) ?*anyopaque {
    const macos_window: *MacOSWindow = @ptrCast(@alignCast(window.backend));
    return macos_window.metal_queue;
}

pub fn getDrawableSize(window: *Window) @import("window.zig").Window.DrawableSize {
    const macos_window: *MacOSWindow = @ptrCast(@alignCast(window.backend));

    // For now, just return window dimensions
    // Metal layer drawable size isn't available until first draw
    _ = macos_window;
    return .{
        .width = window.width,
        .height = window.height,
    };
}
