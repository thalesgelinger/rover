# Rover Agent Guide

## Build/Test/Lint
- Build: `cargo build` or `cargo build -p <crate-name>` (rover-cli, rover-runtime, rover-lua, rover-render, ios-runner)
- Release: `cargo build --release` or `cargo build -p rover-cli --release`
- Test: `cargo test` (workspace) or `cargo test -p rover-lua` (single crate)
- Single test: `cargo test -p rover-lua loads_and_renders_example_app`
- Lint: `cargo clippy` or `cargo clippy -p <crate-name>`
- Run CLI: `./target/release/rover examples/main.lua -p ios`

## Code Style
- Rust 2021 edition, workspace members in crates/ and platform/
- Imports: std first, external crates, internal crates (rover-*), organize by category
- Allow `#![allow(dead_code)]` for WIP crates
- Error handling: `anyhow::Result<T>`, context via `.context()` or `.with_context(|| format!())`
- FFI: C ABI exports prefixed `rover_`, use `*mut RuntimeHandle`, null checks, CString for strings
- Naming: snake_case (fn/vars), PascalCase (types/enums), SCREAMING_SNAKE for consts
- Types: explicit where clarity needed, prefer owned types (PathBuf vs &Path in structs)
- Verbose bool returns for success/dirty flags (e.g. `render_if_dirty() -> Result<bool>`)
- CLI uses clap derive, platform-specific runners encapsulated (IosRunner)
- Keep Swift/platform code minimal: surface creation, event forwarding, vsync only

## Architecture Notes
- Lua DSL via rover-lua (mlua), render via rover-render (skia-safe), runtime orchestrates
- Retained layer tree with dirty tracking, Metal rendering on iOS via RenderSurface abstraction
- Hit-testing in Rust, actions dispatch state updates, mark dirty, invalidate layer tree
