const std = @import("std");
const zlua = @import("zlua");
const args_module = @import("cli/args.zig");
const lua_vm = @import("runtime/lua_vm.zig");
const rover_api = @import("runtime/rover_api.zig");

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
    vm.loadFile(parsed_args.lua_file) catch |err| {
        std.debug.print("Error loading Lua file: {}\n", .{err});
        return err;
    };

    std.debug.print("Success: Lua file loaded\n", .{});
}
