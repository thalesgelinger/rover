const zlua = @import("zlua");

var lua_ref: ?*zlua.Lua = null;

fn copyProp(lua: *zlua.Lua, source_index: i32, dest_index: i32, key: [:0]const u8) void {
    _ = lua.getField(source_index, key);
    if (lua.isNil(-1)) {
        lua.pop(1);
    } else {
        lua.setField(dest_index, key);
    }
}

fn pushNode(lua: *zlua.Lua, arg_index: i32, node_type: [:0]const u8) !void {
    if (!lua.isTable(arg_index)) return error.InvalidNodeArgs;

    lua.newTable();
    const node_index = lua.getTop();

    _ = lua.pushStringZ(node_type);
    lua.setField(node_index, "type");

    lua.newTable();
    const props_index = lua.getTop();
    copyProp(lua, arg_index, props_index, "width");
    copyProp(lua, arg_index, props_index, "height");
    copyProp(lua, arg_index, props_index, "on_click");
    copyProp(lua, arg_index, props_index, "text");
    copyProp(lua, arg_index, props_index, "label");
    lua.setField(node_index, "props");

    lua.newTable();
    const children_index = lua.getTop();
    const count = lua.rawLen(arg_index);
    var i: usize = 1;
    while (i <= count) : (i += 1) {
        _ = lua.rawGetIndex(arg_index, @intCast(i));
        lua.rawSetIndex(children_index, @intCast(i));
    }
    lua.setField(node_index, "children");
}

fn makeNode(state: ?*zlua.LuaState, node_type: [:0]const u8) c_int {
    if (state == null or lua_ref == null) return 0;

    const lua = lua_ref.?;
    pushNode(lua, 1, node_type) catch {
        lua.pushNil();
        return 1;
    };
    return 1;
}

fn luaApp(state: ?*zlua.LuaState) callconv(.c) c_int {
    if (state == null or lua_ref == null) return 0;

    const lua = lua_ref.?;
    lua.newTable();
    return 1;
}

fn luaCol(state: ?*zlua.LuaState) callconv(.c) c_int {
    return makeNode(state, "col");
}

fn luaRow(state: ?*zlua.LuaState) callconv(.c) c_int {
    return makeNode(state, "row");
}

fn luaText(state: ?*zlua.LuaState) callconv(.c) c_int {
    return makeNode(state, "text");
}

fn luaButton(state: ?*zlua.LuaState) callconv(.c) c_int {
    return makeNode(state, "button");
}

pub fn register(lua: *zlua.Lua) void {
    lua_ref = lua;

    lua.newTable();

    lua.pushFunction(luaApp);
    lua.setField(-2, "app");

    lua.pushFunction(luaCol);
    lua.setField(-2, "col");

    lua.pushFunction(luaRow);
    lua.setField(-2, "row");

    lua.pushFunction(luaText);
    lua.setField(-2, "text");

    lua.pushFunction(luaButton);
    lua.setField(-2, "button");

    lua.setGlobal("rover");
}
