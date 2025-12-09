const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const with_skia = b.option(bool, "with-skia", "Link against vendor Skia (vendor/skia/macos-<arch>)") orelse false;

    const zlua = b.dependency("zlua", .{
        .target = target,
        .optimize = optimize,
    });

    const exe = b.addExecutable(.{
        .name = "rover",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/main.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = "zlua", .module = zlua.module("zlua") },
            },
        }),
    });

    if (with_skia) applySkia(b, target, exe);

    b.installArtifact(exe);

    const run_cmd = b.addRunArtifact(exe);
    if (b.args) |args| {
        run_cmd.addArgs(args);
    }

    const run_step = b.step("run", "Run the app");
    run_step.dependOn(&run_cmd.step);

    // Tests
    const unit_tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/render/canvas_test.zig"),
            .target = target,
            .optimize = optimize,
        }),
    });
    if (with_skia) applySkia(b, target, unit_tests);

    const run_unit_tests = b.addRunArtifact(unit_tests);

    const test_step = b.step("test", "Run unit tests");
    test_step.dependOn(&run_unit_tests.step);

    if (with_skia) {
        const skia_tests = b.addTest(.{
            .root_module = b.createModule(.{
                .root_source_file = b.path("src/render/skia_test.zig"),
                .target = target,
                .optimize = optimize,
            }),
        });
        applySkia(b, target, skia_tests);
        const run_skia_tests = b.addRunArtifact(skia_tests);
        test_step.dependOn(&run_skia_tests.step);
    }

    // Canvas test executable
    const canvas_test_exe = b.addExecutable(.{
        .name = "test_canvas",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/test_canvas.zig"),
            .target = target,
            .optimize = optimize,
        }),
    });
    if (with_skia) applySkia(b, target, canvas_test_exe);

    b.installArtifact(canvas_test_exe);
}

fn applySkia(b: *std.Build, target: std.Build.ResolvedTarget, step: *std.Build.Step.Compile) void {
    const arch_dir = switch (target.result.cpu.arch) {
        .aarch64 => "macos-arm64",
        .x86_64 => "macos-x64",
        else => @panic("Skia link: unsupported arch"),
    };

    const lib_dir = b.pathJoin(&.{ "vendor", "skia", arch_dir, "lib" });
    const include_root = b.pathJoin(&.{ "vendor", "skia", arch_dir });

    step.root_module.addLibraryPath(b.path(lib_dir));
    step.root_module.addIncludePath(b.path(include_root));

    const shim_flags = &.{ "-std=c++17", b.fmt("-I{s}", .{include_root}) };
    step.addCSourceFiles(.{ .files = &.{"src/render/skia_shim.mm"}, .flags = shim_flags });

    step.linkSystemLibrary2("skia", .{});
    step.linkSystemLibrary2("z", .{});
    step.linkLibC();
    step.linkLibCpp();

    if (target.result.os.tag == .macos) {
        step.linkFramework("Metal");
        step.linkFramework("MetalKit");
        step.linkFramework("Cocoa");
        step.linkFramework("QuartzCore");
        step.linkFramework("ApplicationServices");
        step.linkFramework("CoreFoundation");
        step.linkFramework("CoreGraphics");
        step.linkFramework("CoreText");
        step.linkFramework("ImageIO");
        step.linkFramework("CoreServices");
        step.linkFramework("Accelerate");
    }
}
