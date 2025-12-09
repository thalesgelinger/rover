const std = @import("std");
const zlua = @import("zlua");
const node_module = @import("../layout/node.zig");

const Node = node_module.Node;
const NodeType = node_module.NodeType;
const Dimension = node_module.Dimension;

var global_lua: ?*zlua.Lua = null;
const BuildError = error{
    UnknownNodeType,
    RenderFunctionMissing,
    RenderDidNotReturnTable,
} || std.mem.Allocator.Error || zlua.Error;

fn actReturnFunction(state: ?*zlua.LuaState) callconv(.c) c_int {
    _ = state;
    if (global_lua) |lua| {
        lua.pushValue(zlua.Lua.upvalueIndex(1));
        return 1;
    }
    return 0;
}

pub const NodeBuilder = struct {
    allocator: std.mem.Allocator,
    lua: *zlua.Lua,
    app_ref: i32,
    act_ref: ?i32 = null,
    state_ref: ?i32 = null,

    pub fn init(allocator: std.mem.Allocator, lua: *zlua.Lua, app_ref: i32) NodeBuilder {
        global_lua = lua;
        return .{
            .allocator = allocator,
            .lua = lua,
            .app_ref = app_ref,
        };
    }

    pub fn deinit(self: *NodeBuilder) void {
        if (self.act_ref) |ref| {
            self.lua.unref(zlua.registry_index, ref);
            self.act_ref = null;
        }

        if (self.state_ref) |ref| {
            self.lua.unref(zlua.registry_index, ref);
            self.state_ref = null;
        }

        self.lua.unref(zlua.registry_index, self.app_ref);
        if (global_lua == self.lua) {
            global_lua = null;
        }
    }

    pub fn build(self: *NodeBuilder) BuildError!Node {
        self.state_ref = try self.callInit();
        return try self.callRender(self.state_ref);
    }

    fn callInit(self: *NodeBuilder) BuildError!?i32 {
        _ = self.lua.rawGetIndex(zlua.registry_index, self.app_ref);
        const app_index = self.lua.getTop();
        defer self.lua.pop(1);

        _ = self.lua.getField(app_index, "init");
        if (self.lua.isNil(-1)) {
            self.lua.pop(1);
            return null;
        }

        defer self.lua.pop(1); // function
        try self.lua.protectedCall(.{ .args = 0, .results = 1 });

        const state_ref = try self.lua.ref(zlua.registry_index);
        return state_ref;
    }

    fn callRender(self: *NodeBuilder, state_ref: ?i32) BuildError!Node {
        _ = self.lua.rawGetIndex(zlua.registry_index, self.app_ref);
        const app_index = self.lua.getTop();
        defer self.lua.pop(1);

        try self.ensureActTable(app_index);

        _ = self.lua.getField(app_index, "render");
        if (!self.lua.isFunction(-1)) {
            self.lua.pop(1);
            return error.RenderFunctionMissing;
        }

        if (state_ref) |ref| {
            _ = self.lua.rawGetIndex(zlua.registry_index, ref);
        } else {
            self.lua.pushNil();
        }

        if (self.act_ref) |ref| {
            _ = self.lua.rawGetIndex(zlua.registry_index, ref);
        } else {
            self.lua.pushNil();
        }

        try self.lua.protectedCall(.{ .args = 2, .results = 1 });

        const node_index = self.lua.getTop();
        if (!self.lua.isTable(node_index)) {
            self.lua.pop(1);
            return error.RenderDidNotReturnTable;
        }

        const root = try self.parseNode(node_index);
        self.lua.pop(1); // pop node table
        return root;
    }

    fn ensureActTable(self: *NodeBuilder, app_index: i32) BuildError!void {
        if (self.act_ref != null) return;

        self.lua.newTable();
        const act_index = self.lua.getTop();
        errdefer self.lua.pop(1);

        try self.populateActTable(app_index, act_index);

        self.act_ref = try self.lua.ref(zlua.registry_index);
    }

    fn populateActTable(self: *NodeBuilder, app_index: i32, act_index: i32) BuildError!void {
        self.lua.pushValue(app_index);
        const app_copy_index = self.lua.getTop();
        defer self.lua.pop(1);

        self.lua.pushNil();
        while (self.lua.next(app_copy_index)) {
            defer self.lua.pop(1);
            if (!self.lua.isString(-2)) continue;
            if (!self.lua.isFunction(-1)) continue;

            const key_z = try self.lua.toString(-2);
            const key = std.mem.sliceTo(key_z, 0);
            if (std.mem.eql(u8, key, "init") or std.mem.eql(u8, key, "render")) continue;

            self.lua.pushValue(-1);
            self.lua.pushClosure(actReturnFunction, 1);
            self.lua.setField(act_index, key_z);
        }
    }

    fn parseNode(self: *NodeBuilder, table_index: i32) BuildError!Node {
        _ = self.lua.getField(table_index, "type");
        defer self.lua.pop(1);
        const type_z = try self.lua.toString(-1);
        const node_type = try self.parseNodeType(type_z);

        var node = Node.init(self.allocator, node_type);

        _ = self.lua.getField(table_index, "props");
        if (!self.lua.isNil(-1)) {
            try self.parseProps(&node, self.lua.getTop());
        }
        self.lua.pop(1);

        _ = self.lua.getField(table_index, "children");
        if (!self.lua.isNil(-1)) {
            try self.parseChildren(&node, self.lua.getTop());
        }
        self.lua.pop(1);

        return node;
    }

    fn parseNodeType(self: *NodeBuilder, name: [:0]const u8) !NodeType {
        _ = self;
        const key = std.mem.sliceTo(name, 0);
        if (std.mem.eql(u8, key, "col")) return .col;
        if (std.mem.eql(u8, key, "row")) return .row;
        if (std.mem.eql(u8, key, "text")) return .text;
        if (std.mem.eql(u8, key, "button")) return .button;
        return error.UnknownNodeType;
    }

    fn parseProps(self: *NodeBuilder, node: *Node, props_index: i32) BuildError!void {
        _ = self.lua.getField(props_index, "width");
        const width = try self.parseDimension(self.lua.getTop());
        self.lua.pop(1);
        node.props.width = width;

        _ = self.lua.getField(props_index, "height");
        const height = try self.parseDimension(self.lua.getTop());
        self.lua.pop(1);
        node.props.height = height;

        _ = self.lua.getField(props_index, "on_click");
        if (self.lua.isNil(-1)) {
            self.lua.pop(1);
        } else {
            node.props.on_click_ref = try self.lua.ref(zlua.registry_index);
        }

        _ = self.lua.getField(props_index, "text");
        if (!self.lua.isNil(-1)) {
            const text_z = try self.lua.toString(-1);
            node.props.text_content = try self.copyString(text_z);
        }
        self.lua.pop(1);

        _ = self.lua.getField(props_index, "label");
        if (!self.lua.isNil(-1)) {
            const label_z = try self.lua.toString(-1);
            node.props.label = try self.copyString(label_z);
        }
        self.lua.pop(1);
    }

    fn parseChildren(self: *NodeBuilder, node: *Node, children_index: i32) BuildError!void {
        const count = self.lua.rawLen(children_index);
        var i: usize = 1;
        while (i <= count) : (i += 1) {
            _ = self.lua.rawGetIndex(children_index, @intCast(i));
            if (self.lua.isTable(-1)) {
                const table_idx = self.lua.getTop();
                const child = try self.parseNode(table_idx);
                self.lua.pop(1);
                try node.addChild(child);
            } else if (self.lua.isString(-1)) {
                const str_z = try self.lua.toString(-1);
                switch (node.node_type) {
                    .text => {
                        if (node.props.text_content == null) {
                            node.props.text_content = try self.copyString(str_z);
                        }
                    },
                    .button => {
                        if (node.props.label == null) {
                            node.props.label = try self.copyString(str_z);
                        }
                    },
                    else => {},
                }
                self.lua.pop(1);
            } else {
                self.lua.pop(1);
            }
        }
    }

    fn parseDimension(self: *NodeBuilder, index: i32) BuildError!Dimension {
        if (self.lua.isNil(index)) return .auto;
        if (self.lua.isString(index)) {
            const value = try self.lua.toString(index);
            const slice = std.mem.sliceTo(value, 0);
            if (std.mem.eql(u8, slice, "full")) return .full;
            return .auto;
        }
        if (self.lua.isNumber(index)) {
            const num = try self.lua.toNumber(index);
            return Dimension{ .pixels = num };
        }
        return .auto;
    }

    fn copyString(self: *NodeBuilder, text_z: [:0]const u8) ![]u8 {
        const slice = std.mem.sliceTo(text_z, 0);
        return try self.allocator.dupe(u8, slice);
    }
};

pub fn debugPrint(node: *const Node) void {
    const stdout = std.io.getStdOut().writer();
    node.debugPrint(stdout, 0) catch {};
}
