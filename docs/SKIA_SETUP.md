# Skia Setup Guide

Rover uses Skia for 2D graphics rendering. You have two options:

## Option 1: Download Prebuilds (Quick - 5 min)

**Recommended for most users**

1. Download prebuilds for your platform:
   ```bash
   # macOS arm64 (M1/M2/M3)
   curl -L https://github.com/thalesgelinger/rover/releases/download/v0.1.0/skia-macos-arm64.tar.gz -o /tmp/skia-macos-arm64.tar.gz
   
   # macOS x64 (Intel)
   curl -L https://github.com/thalesgelinger/rover/releases/download/v0.1.0/skia-macos-x64.tar.gz -o /tmp/skia-macos-x64.tar.gz
   ```

2. Extract to vendor directory:
   ```bash
   mkdir -p vendor/skia
   tar -xzf /tmp/skia-macos-arm64.tar.gz -C vendor/skia/
   ```

3. Build with Skia:
   ```bash
   zig build -Dwith-skia=true test
   ```

## Option 2: Build from Source (Slow - 60+ min)

**For contributors or if prebuilds don't work**

### Prerequisites

- macOS with Xcode Command Line Tools
- Python 3
- Git
- ~15GB free disk space
- Good internet connection

### Build Steps

1. Run setup script (clones Skia, depot_tools):
   ```bash
   ./scripts/setup.sh
   ```
   Takes ~10 min, downloads ~2GB

2. Build Skia for your platform:
   ```bash
   # macOS arm64
   ./scripts/build_skia_macos.sh arm64
   
   # macOS x64
   ./scripts/build_skia_macos.sh x64
   
   # Both platforms
   ./scripts/build_skia_macos.sh
   ```
   Takes ~45-60 min per arch

3. Test build:
   ```bash
   zig build -Dwith-skia=true test
   ```
   Should see: `✓ PNG saved to /tmp/rover_skia_test_output.png`

### Build Output

Prebuilds are located in:
- `vendor/skia/macos-arm64/` - 17M
- `vendor/skia/macos-x64/` - 17M

Each contains:
- `lib/libskia.a` - Static library
- `include/` - Skia headers

### Creating Release Tarball

For maintainers creating prebuilds:

```bash
cd vendor/skia
tar -czf skia-macos-arm64.tar.gz macos-arm64/
tar -czf skia-macos-x64.tar.gz macos-x64/

# Upload to GitHub Releases
gh release create v0.1.0 skia-macos-arm64.tar.gz skia-macos-x64.tar.gz
```

## Troubleshooting

### Build fails with "fork: Resource temporarily unavailable"
Your system has low process limits. The scripts use `-j1` flag already.

### Missing depot_tools or ninja
Run `./scripts/setup.sh` again.

### Linker errors about zlib symbols
Make sure build.zig links zlib: `step.linkSystemLibrary2("z", .{});`

### Test PNG is blank or corrupted
Skia might not be linking correctly. Check:
```bash
nm -g vendor/skia/macos-arm64/lib/libskia.a | grep SkSurface
```

## Architecture

```
rover/
├── scripts/
│   ├── setup.sh              # Clone Skia, setup depot_tools
│   └── build_skia_macos.sh   # Build Skia prebuilds
├── src/render/
│   ├── skia.zig              # Zig bindings
│   ├── skia_shim.cpp         # C++ bridge to Skia
│   └── skia_test.zig         # Tests
├── vendor/skia/              # Prebuilds (not in git)
│   ├── macos-arm64/
│   └── macos-x64/
└── build.zig                 # Links Skia with -Dwith-skia=true
```

## Why Not Commit Prebuilds?

- Too large: 34M (GitHub soft limit is 50-100M repos)
- Platform-specific: macOS arm64/x64, future iOS/Linux
- Users only need their platform (1 arch = 17M)
- GitHub Releases handles versioning better

## Future Platforms

When adding iOS/Linux/Windows:
1. Update `scripts/build_skia_*.sh` for new platform
2. Build prebuilds on CI
3. Upload to GitHub Releases
4. Update this doc with download links
