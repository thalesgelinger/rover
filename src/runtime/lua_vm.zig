const std = @import("std");
const zlua = @import("zlua");

pub const LuaVm = struct {
    allocator: std.mem.Allocator,
    lua: zlua.Lua,

    pub fn init(allocator: std.mem.Allocator) !LuaVm {
        var lua = try zlua.Lua.init(&allocator);
        lua.openLibs();
        return .{
            .allocator = allocator,
            .lua = lua,
        };
    }

    pub fn deinit(self: *LuaVm) void {
        self.lua.deinit();
    }

    pub fn loadFile(self: *LuaVm, file_path: []const u8) !void {
        const file = try std.fs.cwd().openFile(file_path, .{});
        defer file.close();

        const content = try file.readToEndAlloc(self.allocator, 1024 * 1024);
        defer self.allocator.free(content);

        try self.lua.loadString(content);
        try self.lua.protectedCall(0, 1, 0);
    }

    pub fn getGlobal(self: *LuaVm, name: []const u8) !void {
        self.lua.getGlobal(name);
    }

    pub fn setGlobal(self: *LuaVm, name: []const u8) void {
        self.lua.setGlobal(name);
    }

    pub fn pushFunction(self: *LuaVm, func: zlua.LuaCFunction) void {
        self.lua.pushFunction(func);
    }

    pub fn newTable(self: *LuaVm) void {
        self.lua.newTable();
    }

    pub fn setField(self: *LuaVm, name: []const u8) void {
        self.lua.setField(-2, name);
    }

    pub fn getTop(self: *LuaVm) i32 {
        return self.lua.getTop();
    }
};
