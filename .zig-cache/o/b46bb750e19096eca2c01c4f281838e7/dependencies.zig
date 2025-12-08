pub const packages = struct {
    pub const @"N-V-__8AABAhDAAIlXL7OA-0Z5sWQh_FOFGoImvOvJzkRGOg" = struct {
        pub const available = false;
    };
    pub const @"N-V-__8AACcgQgCuLYTPzCp6pnBmFJHyG77RAtM13hjOfTaG" = struct {
        pub const available = false;
    };
    pub const @"N-V-__8AAFB1kwDHb7dLmDsOv91rOkqorfDB_2nJtqnp4F-b" = struct {
        pub const available = false;
    };
    pub const @"N-V-__8AAKEzFAAA695b9LXBhUSVK5MAV_VKSm1mEj3Acbze" = struct {
        pub const available = true;
        pub const build_root = "/Users/thalesgelinger/.cache/zig/p/N-V-__8AAKEzFAAA695b9LXBhUSVK5MAV_VKSm1mEj3Acbze";
        pub const deps: []const struct { []const u8, []const u8 } = &.{};
    };
    pub const @"N-V-__8AALg2DgDVsrOXOPBkTZ7Vt0MZc_Gha5N--G1M-FiH" = struct {
        pub const available = false;
    };
    pub const @"N-V-__8AALihEACTeiI1Me9rP-qPZT3BNTELDoSAXn76FIhw" = struct {
        pub const available = false;
    };
    pub const @"zlua-0.1.0-hGRpCww-BQCx3cX6zbKWfTgOx6B7Peo4P17RK2hm7-xV" = struct {
        pub const build_root = "/Users/thalesgelinger/.cache/zig/p/zlua-0.1.0-hGRpCww-BQCx3cX6zbKWfTgOx6B7Peo4P17RK2hm7-xV";
        pub const build_zig = @import("zlua-0.1.0-hGRpCww-BQCx3cX6zbKWfTgOx6B7Peo4P17RK2hm7-xV");
        pub const deps: []const struct { []const u8, []const u8 } = &.{
            .{ "lua51", "N-V-__8AABAhDAAIlXL7OA-0Z5sWQh_FOFGoImvOvJzkRGOg" },
            .{ "lua52", "N-V-__8AALg2DgDVsrOXOPBkTZ7Vt0MZc_Gha5N--G1M-FiH" },
            .{ "lua53", "N-V-__8AALihEACTeiI1Me9rP-qPZT3BNTELDoSAXn76FIhw" },
            .{ "lua54", "N-V-__8AAKEzFAAA695b9LXBhUSVK5MAV_VKSm1mEj3Acbze" },
            .{ "luajit", "N-V-__8AACcgQgCuLYTPzCp6pnBmFJHyG77RAtM13hjOfTaG" },
            .{ "luau", "N-V-__8AAFB1kwDHb7dLmDsOv91rOkqorfDB_2nJtqnp4F-b" },
        };
    };
};

pub const root_deps: []const struct { []const u8, []const u8 } = &.{
    .{ "zlua", "zlua-0.1.0-hGRpCww-BQCx3cX6zbKWfTgOx6B7Peo4P17RK2hm7-xV" },
};
