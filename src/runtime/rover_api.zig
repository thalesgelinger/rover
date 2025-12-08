const std = @import("std");
const zlua = @import("zlua");

var lua_ref: ?*zlua.Lua = null;

// Lua callback function - must match CFn signature
fn luaApp(state: ?*zlua.LuaState) callconv(.c) c_int {
    if (state == null or lua_ref == null) return 0;

    const lua = lua_ref.?;
    // Create new table for app instance
    lua.newTable();
    return 1;
}

pub fn register(lua: *zlua.Lua) void {
    lua_ref = lua;

    // Create rover global table
    lua.newTable();

    // Register rover.app function
    lua.pushFunction(luaApp);
    lua.setField(-2, "app");

    // Set as global
    lua.setGlobal("rover");
}
