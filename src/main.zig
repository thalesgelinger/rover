const std = @import("std");
const zlua = @import("zlua");
const args_module = @import("cli/args.zig");
const lua_vm = @import("runtime/lua_vm.zig");
const rover_api = @import("runtime/rover_api.zig");
const node_builder = @import("runtime/node_builder.zig");
const platform = @import("platform/platform.zig");
const skia = @import("render/skia.zig");
const fps_counter_module = @import("render/fps_counter.zig");

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    // Parse CLI args
    const argv = try std.process.argsAlloc(allocator);
    defer std.process.argsFree(allocator, argv);

    const parsed_args = args_module.parse(argv) catch |err| {
        std.debug.print("Error parsing arguments: {}\n", .{err});
        return err;
    };

    std.debug.print("Loading: {s} [platform: {}]\n", .{ parsed_args.lua_file, parsed_args.platform });

    // Initialize Lua VM
    var vm = try lua_vm.LuaVm.init(allocator);
    defer vm.deinit();

    // Register rover API
    rover_api.register(vm.lua);

    // Load and execute Lua file
    const app_ref = vm.loadFile(parsed_args.lua_file) catch |err| {
        std.debug.print("Error loading Lua file: {}\n", .{err});
        return err;
    };

    std.debug.print("Success: Lua file loaded\n", .{});

    // Build node tree
    var builder = node_builder.NodeBuilder.init(allocator, vm.lua, app_ref);
    defer builder.deinit();

    var root = try builder.build();
    defer root.deinit(vm.lua);

    if (parsed_args.debug_tree) {
        node_builder.debugPrint(&root);
    }

    // Create window
    std.debug.print("Creating window...\n", .{});
    var window = try platform.Window.create(800, 600, "Rover");
    defer window.destroy(allocator);

    // Get Metal resources from window
    const metal_device = window.getMetalDevice() orelse return error.MetalDeviceUnavailable;
    const metal_queue = window.getMetalQueue() orelse return error.MetalQueueUnavailable;
    const metal_layer = window.getMetalLayer() orelse return error.MetalLayerUnavailable;

    // Create Metal context
    std.debug.print("Initializing Metal context...\n", .{});
    var metal_ctx = try skia.MetalContext.init(metal_device, metal_queue);
    defer metal_ctx.deinit();

    // Get drawable size (accounts for retina)
    const drawable_size = window.getDrawableSize();
    std.debug.print("Drawable size: {}x{}\n", .{ drawable_size.width, drawable_size.height });

    // Create Metal canvas
    var canvas = try skia.MetalCanvas.init(&metal_ctx, metal_layer, drawable_size.width, drawable_size.height);
    defer canvas.deinit();

    // FPS counter
    var fps_counter = fps_counter_module.FpsCounter.init();

    std.debug.print("Entering render loop...\n", .{});

    // Render loop
    while (!window.shouldClose()) {
        window.pollEvents();

        const latest_size = window.getDrawableSize();
        if (latest_size.width != canvas.width or latest_size.height != canvas.height) {
            canvas.deinit();
            canvas = try skia.MetalCanvas.init(&metal_ctx, metal_layer, latest_size.width, latest_size.height);
            window.markDirty();
        }

        if (window.needsRedraw()) {
            fps_counter.recordFrame();

            // Clear to sky blue
            try canvas.clear(skia.rgba(135, 206, 235, 255));

            // Draw test rectangle (red-orange)
            try canvas.drawRect(200, 200, 400, 200, skia.rgba(255, 69, 0, 255));

            // Draw FPS counter if enabled
            if (parsed_args.debug_fps) {
                try fps_counter.draw(&canvas, drawable_size.width);
            }

            // Flush and present to screen
            canvas.flush();
            canvas.present();
            metal_ctx.flushAndSubmit(false);

            window.clearRedraw();
        } else {
            // Sleep to avoid busy-wait (check at ~60hz)
            std.Thread.sleep(16_000_000); // ~16ms
        }
    }

    std.debug.print("Window closed. Exiting.\n", .{});
}
