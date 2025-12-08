const std = @import("std");
const zlua = @import("zlua");

fn luaApp(lua: *zlua.Lua) i32 {
    // Create new table for app instance
    lua.newTable();
    return 1;
}

pub fn register(lua: *zlua.Lua) void {
    // Create rover global table
    lua.newTable();

    // Register rover.app function
    lua.pushFunction(luaApp);
    lua.setField(-2, "app");

    // Set as global
    lua.setGlobal("rover");
}
