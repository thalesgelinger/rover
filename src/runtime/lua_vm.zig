const std = @import("std");
const zlua = @import("zlua");

pub const LuaVm = struct {
    allocator: std.mem.Allocator,
    lua: *zlua.Lua,

    pub fn init(allocator: std.mem.Allocator) !LuaVm {
        var lua = try zlua.Lua.init(allocator);
        lua.openLibs();
        return .{
            .allocator = allocator,
            .lua = lua,
        };
    }

    pub fn deinit(self: *LuaVm) void {
        self.lua.*.deinit();
    }

    pub fn loadFile(self: *LuaVm, file_path: []const u8) !i32 {
        const file = try std.fs.cwd().openFile(file_path, .{});
        defer file.close();

        const content = try file.readToEndAlloc(self.allocator, 1024 * 1024);
        defer self.allocator.free(content);

        // Create null-terminated copy for Lua
        const content_z = try self.allocator.dupeZ(u8, content);
        defer self.allocator.free(content_z);

        try self.lua.*.loadString(content_z);
        try self.lua.*.protectedCall(.{ .args = 0, .results = 1 });

        if (!self.lua.*.isTable(-1)) {
            self.lua.*.pop(1);
            return error.InvalidAppReturn;
        }

        return try self.lua.*.ref(zlua.registry_index);
    }

    pub fn getGlobal(self: *LuaVm, name: []const u8) !void {
        self.lua.*.getGlobal(name);
    }

    pub fn setGlobal(self: *LuaVm, name: []const u8) void {
        self.lua.*.setGlobal(name);
    }

    pub fn pushFunction(self: *LuaVm, func: zlua.LuaCFunction) void {
        self.lua.*.pushFunction(func);
    }

    pub fn newTable(self: *LuaVm) void {
        self.lua.*.newTable();
    }

    pub fn setField(self: *LuaVm, name: [:0]const u8) void {
        self.lua.*.setField(-2, name);
    }

    pub fn getTop(self: *LuaVm) i32 {
        return self.lua.*.getTop();
    }
};
