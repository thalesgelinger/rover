# PLAN.md

## Vision

Lua-first mobile framework powered by Rust + Skia; iOS first, Android next.

## DX target
```
rover main.lua                # default platform (sim on macOS => iOS sim)
rover main.lua -p ios         # iOS sim (default)
rover main.lua -p ios --device <udid>  # later
rover build main.lua -p ios   # build only (future)
```

## Runtime architecture
- Rust core hosts event/render loop
- Skia (rust-skia) surfaces per platform
- Lua via mlua + LuaJIT (interpreter only on iOS; JIT later on Android/desktop)
- State reducer pattern: app.init (optional), app.render(required), custom actions
- No globals; props/state passed; layout primitives map to Skia draw + hit-test

## Workspace layout (proposed)
- `crates/rover-cli`: CLI binary, args, progress/log piping
- `crates/rover-runtime`: state/event loop, platform-agnostic core
- `crates/rover-lua`: mlua bindings exposing rover API
- `crates/rover-render`: Skia wrapper + surfaces abstraction
- `platform/ios-runner`: iOS glue/staticlib, Xcode templates, sim harness
- `examples/`: Lua samples + assets

## Lua API (MVP)
- `rover.app()` returns table
- Required: `render(state, act)`; optional: `init()` returns initial state
- Actions: functions on app table become reducers; wired as `act.<name>()`
- Primitives: `rover.col`, `rover.row`, `rover.text`, `rover.button`
- Layout props: width/height numbers or `"full"`; basic style only
- Events: on_click on button; expands later (touch, key)

## iOS flow (sim first)
- Prereqs: Rust toolchain + Xcode/CLT installed by user
- Embed XcodeProjectCLI Swift package (vendored) inside runner; no user install
- Generate/patch Xcode project; template lives in `platform/ios-runner`
- Build Rust staticlib, link into Xcode target; embed Lua (source in dev, bytecode in release) + assets/
- Copy `main.lua` + `assets/` into app bundle
- Launch via `xcrun simctl install/launch`; target sim default
- App name/bundle fixed for now; allow custom later
- Later: device signing/profile + JIT toggle (off on iOS)

## Android (later)
- Post-iOS MVP: cargo-ndk build, Skia backend, adb install/run; JIT can be on

## CLI shape
- `rover run <entry> [-p ios] [--sim|--device <udid>] [--verbose]`
- Handles: validate deps, prepare build dir, generate project, build, install, launch
- Logs to stdout/stderr; forward Lua errors; basic progress steps

## Packaging strategy
- Dev: load Lua from filesystem for easy edits; include assets/ via copy
- Release: bundle Lua bytecode + assets inside app; Rust compiled static; fat binaries for sim/arm64

## DX backlog
- Better logs and error surfacing
- Hot reload watching Lua files
- Inspector/overlay for layout+state

## Phases
- Phase 0: CLI scaffold + iOS sim run happy-path
- Phase 1: Stable runtime bindings (layout, events) + assets bundle
- Phase 2: iOS packaging polish, basic build command
- Phase 3: Android bring-up
- Phase 4: DX extras (reload, overlay)

## Open items
- Confirm LuaJIT interpreter-only on iOS accepted (yes)
- Asset folder convention: `assets/` beside entry (yes)
- Android work starts after iOS MVP
- Build command scope minimal initially (generate + archive); refine later
