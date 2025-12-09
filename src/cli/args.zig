const std = @import("std");

pub const Platform = enum {
    macos,
    ios,
    android,
};

pub const Args = struct {
    lua_file: []const u8,
    platform: Platform = .macos,
    debug_fps: bool = false,
};

pub fn parse(argv: []const []const u8) !Args {
    var args = Args{
        .lua_file = "",
    };

    var i: usize = 1; // Skip program name
    while (i < argv.len) : (i += 1) {
        const arg = argv[i];

        if (std.mem.eql(u8, arg, "-p") or std.mem.eql(u8, arg, "--platform")) {
            i += 1;
            if (i >= argv.len) {
                return error.MissingPlatformValue;
            }
            const platform_str = argv[i];
            args.platform = std.meta.stringToEnum(Platform, platform_str) orelse {
                return error.InvalidPlatform;
            };
        } else if (std.mem.eql(u8, arg, "--debug-fps")) {
            args.debug_fps = true;
        } else if (std.mem.startsWith(u8, arg, "-")) {
            return error.UnknownFlag;
        } else if (args.lua_file.len == 0) {
            args.lua_file = arg;
        } else {
            return error.TooManyArgs;
        }
    }

    if (args.lua_file.len == 0) {
        return error.MissingLuaFile;
    }

    return args;
}
