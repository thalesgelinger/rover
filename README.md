# Rover

Flutter-like engine with Lua UI: Rust + Skia GPU rendering, iOS first.

## Quick Start

```bash
# Build CLI
cargo build -p rover-cli --release

# Run example on iOS simulator
./target/release/rover examples/main.lua -p ios
```

## Usage

```bash
rover <ENTRY> [OPTIONS]

Options:
  -p, --platform <PLATFORM>  Target platform [default: ios] [possible values: ios, android]
      --device <UDID>        iOS device UDID (runs on device instead of simulator)
      --verbose              Verbose logging

Commands:
  run    Run the app (default)
  build  Build only (no run)
```

## Architecture

- **Lua DSL**: Declarative UI with `col`/`row`/`text`/`button`, flex layout
- **Skia+Metal**: GPU rendering via CAMetalLayer, no CPU copy
- **Retained tree**: Layout cached until state changes (dirty tracking)
- **Hit-testing**: Touch events processed in Rust, action dispatch triggers re-render

## Status

✅ iOS simulator with Metal  
🚧 Android (planned)  
🚧 Physical device support (planned)
