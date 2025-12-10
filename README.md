# Rover

Flutter-like engine with Lua UI: Rust + Skia GPU rendering, iOS first.

## Quick Start

```bash
# Build CLI
cargo build -p rover-cli --release

# Run example on iOS simulator
./target/release/rover examples/main.lua -p ios

# Run example on Android (needs SDK/NDK + Gradle; mise: `mise install gradle`)
./target/release/rover examples/main.lua -p android
```

## Usage

```bash
rover <ENTRY> [OPTIONS]

Options:
  -p, --platform <PLATFORM>  Target platform [default: ios] [possible values: ios, android]
      --device <UDID>        iOS device UDID (runs on device instead of simulator)
      --watch                Watch for file changes and hot reload (dev only)
      --verbose              Verbose logging

Commands:
  run    Run the app (default)
  build  Build only (no run)
```

### Hot Reload

Watch mode enables instant hot reload during development:

```bash
# Run with hot reload on iOS
./target/release/rover examples/main.lua -p ios --watch

# Run with hot reload on Android
./target/release/rover examples/main.lua -p android --watch
```

Changes to `.lua` files trigger automatic reload while preserving app state.

## Architecture

- **Lua DSL**: Declarative UI with `col`/`row`/`text`/`button`, flex layout
- **Skia+Metal**: GPU rendering via CAMetalLayer, no CPU copy
- **Retained tree**: Layout cached until state changes (dirty tracking)
- **Hit-testing**: Touch events processed in Rust, action dispatch triggers re-render

## Status

✅ iOS simulator with Metal  
🚧 Android (emulator, Vulkan; requires Android SDK/NDK + Gradle available via PATH or mise `mise install gradle`)  
🚧 Physical device support (planned)
