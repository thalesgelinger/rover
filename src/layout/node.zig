const std = @import("std");
const zlua = @import("zlua");

pub const NodeType = enum {
    col,
    row,
    text,
    button,
};

pub const Dimension = union(enum) {
    auto,
    full,
    pixels: f64,

    pub fn toString(self: Dimension) []const u8 {
        return switch (self) {
            .auto => "auto",
            .full => "full",
            .pixels => "px",
        };
    }
};

pub const NodeProps = struct {
    width: Dimension = .auto,
    height: Dimension = .auto,
    on_click_ref: ?i32 = null,
    text_content: ?[]u8 = null,
    label: ?[]u8 = null,
};

pub const Node = struct {
    allocator: std.mem.Allocator,
    node_type: NodeType,
    props: NodeProps = .{},
    children: std.ArrayList(Node),

    pub fn init(allocator: std.mem.Allocator, node_type: NodeType) Node {
        return .{
            .allocator = allocator,
            .node_type = node_type,
            .props = .{},
            .children = std.ArrayList(Node).empty,
        };
    }

    pub fn deinit(self: *Node, lua: ?*zlua.Lua) void {
        if (lua) |l| {
            if (self.props.on_click_ref) |ref| {
                l.unref(zlua.registry_index, ref);
            }
        }

        if (self.props.text_content) |text| {
            self.allocator.free(text);
        }

        if (self.props.label) |label_text| {
            self.allocator.free(label_text);
        }

        var i: usize = 0;
        while (i < self.children.items.len) : (i += 1) {
            var child = &self.children.items[i];
            child.deinit(lua);
        }
        self.children.deinit(self.allocator);
    }

    pub fn addChild(self: *Node, child: Node) !void {
        try self.children.append(self.allocator, child);
    }

    pub fn debugPrint(self: *const Node, writer: anytype, indent: usize) !void {
        try writer.writeByteNTimes(' ', indent);
        try writer.print("{s}", .{@tagName(self.node_type)});

        if (self.props.text_content) |text| {
            try writer.print(" text='{s}'", .{text});
        }

        if (self.props.label) |label_text| {
            try writer.print(" label='{s}'", .{label_text});
        }

        if (self.props.on_click_ref != null) {
            try writer.writeAll(" on_click=ref");
        }

        switch (self.props.width) {
            .pixels => |px| try writer.print(" width={d:.2}", .{px}),
            .full => try writer.writeAll(" width=full"),
            else => {},
        }

        switch (self.props.height) {
            .pixels => |px| try writer.print(" height={d:.2}", .{px}),
            .full => try writer.writeAll(" height=full"),
            else => {},
        }

        try writer.writeByte('\n');

        for (self.children.items) |child| {
            try child.debugPrint(writer, indent + 2);
        }
    }
};
