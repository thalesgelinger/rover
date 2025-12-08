# PLAN.md

## Vision

**Rover**: A Lua-first cross-platform mobile framework using Skia for rendering, Zig for the native runtime, targeting iOS, Android, and desktop (macOS first).

**Developer Experience Goal**:
```bash
rover main.lua          # runs on current OS (macOS window)
rover main.lua -p ios   # builds & launches iOS simulator
rover main.lua -p android # future
```

---

## Architecture

```
┌────────────────────────────────────────────────────┐
│                    Lua Layer                        │
│  rover.app(), rover.col, rover.row, rover.text...  │
│  User code: init(), render(state, actions), custom │
├────────────────────────────────────────────────────┤
│                  Zig Runtime Core                   │
│  ┌─────────────┬─────────────┬──────────────────┐  │
│  │ Lua 5.4 VM  │ Skia Wrapper│ Layout Engine    │  │
│  │ (ziglua)    │ (C FFI)     │ (flexbox-like)   │  │
│  └─────────────┴─────────────┴──────────────────┘  │
│  ┌─────────────────────────────────────────────┐   │
│  │         Platform Abstraction Layer          │   │
│  │  - Window creation                          │   │
│  │  - Event loop (input, lifecycle)            │   │
│  │  - Skia surface/context setup               │   │
│  └─────────────────────────────────────────────┘   │
├────────────────────────────────────────────────────┤
│              Platform Backends                      │
│  ┌──────────┬───────────┬───────────┬──────────┐   │
│  │  macOS   │    iOS    │  Android  │  Linux   │   │
│  │ (Cocoa)  │ (UIKit)   │  (NDK)    │ (future) │   │
│  └──────────┴───────────┴───────────┴──────────┘   │
└────────────────────────────────────────────────────┘
```

---

## Tech Stack

| Layer | Technology | Notes |
|-------|------------|-------|
| Language (user) | Lua 5.4 | No JIT = iOS safe |
| Language (runtime) | Zig 0.13.0 stable | C interop, cross-compile |
| Lua binding | [ziglua](https://github.com/natecraddock/ziglua) | Zig-native Lua 5.4 bindings |
| Rendering | Skia (prebuilt) | Use prebuilt binaries, wrap via C FFI |
| macOS window | Cocoa via Zig objc or mach-glfw | |
| iOS project | XcodeProjectCLI | Auto-generate .xcodeproj |
| CLI | Zig | `rover` binary |

---

## Phases

### Phase 1: Project Skeleton & CLI
**Goal**: `rover` binary that parses args and loads Lua file

**Tasks**:
- [ ] 1.1 Init Zig project structure (`build.zig`, `src/`)
- [ ] 1.2 Add ziglua as dependency
- [ ] 1.3 CLI arg parsing: `rover <file.lua> [-p platform]`
- [ ] 1.4 Load & execute Lua file, print result
- [ ] 1.5 Expose `rover` global table to Lua (empty for now)

**Tests**:
- `rover test.lua` loads file, `rover.app()` returns table
- CLI parses `-p ios`, `-p macos` flags

**Deliverable**: Can run `rover main.lua`, Lua executes, prints debug

---

### Phase 2: Skia Integration
**Goal**: Zig can call Skia, draw to a buffer

**Tasks**:
- [ ] 2.1 Download/setup prebuilt Skia (macOS arm64 + x64)
- [ ] 2.2 Create Zig C bindings for Skia C API (skia/include/c/)
- [ ] 2.3 Create software raster surface, draw rect, save PNG
- [ ] 2.4 Abstract into `Canvas` struct (drawRect, drawText, etc.)

**Tests**:
- Unit test: create canvas, draw red rect, verify PNG output
- Unit test: draw text "Hello", verify non-empty output

**Deliverable**: `zig build test` produces `test_output.png` with shapes

---

### Phase 3: macOS Window
**Goal**: Native window displaying Skia canvas

**Tasks**:
- [ ] 3.1 macOS backend using Cocoa (NSWindow, NSView, Metal or OpenGL surface)
- [ ] 3.2 Alternative: use mach-glfw for quicker start
- [ ] 3.3 Skia GPU surface connected to window
- [ ] 3.4 Render loop: clear screen, draw test rect each frame
- [ ] 3.5 Basic event handling: window close, resize

**Tests**:
- Manual: window opens, shows blue rect
- Resize window, canvas resizes

**Deliverable**: `rover main.lua` opens window with test graphics

---

### Phase 4: Rover Lua API - Components
**Goal**: `rover.col`, `rover.row`, `rover.text`, `rover.button` return node trees

**Tasks**:
- [ ] 4.1 Define Node struct in Zig (type, props, children)
- [ ] 4.2 Register Lua functions: `rover.col`, `rover.row`, `rover.text`, `rover.button`
- [ ] 4.3 Parse Lua table args: `{ width=100, height=50, "child1", "child2" }`
- [ ] 4.4 Build node tree from `app.render()` return value
- [ ] 4.5 Debug print node tree

**Tests**:
- Lua returns `rover.col { rover.text { "hi" } }` → Zig prints tree structure
- Props parsed: `width`, `height`, `on_click`

**Deliverable**: Node tree built from Lua, printable

---

### Phase 5: Layout Engine
**Goal**: Position nodes using flexbox-like algorithm

**Tasks**:
- [ ] 5.1 Layout struct: x, y, width, height per node
- [ ] 5.2 Implement `col` layout: stack children vertically
- [ ] 5.3 Implement `row` layout: stack children horizontally
- [ ] 5.4 Handle `width`, `height` props (fixed values)
- [ ] 5.5 Handle `full` keyword = parent size
- [ ] 5.6 Basic sizing: text measures via Skia

**Tests**:
- col with 2 children: y positions stacked
- row with 2 children: x positions stacked
- `width = 'full'` fills parent

**Deliverable**: Node tree has computed layout positions

---

### Phase 6: Rendering Pipeline
**Goal**: Draw node tree to Skia canvas

**Tasks**:
- [ ] 6.1 Render traversal: walk tree, draw each node
- [ ] 6.2 `text` node: Skia drawText at computed position
- [ ] 6.3 `button` node: draw rect + text (basic styling)
- [ ] 6.4 `col`/`row`: just layout containers, maybe draw debug bounds
- [ ] 6.5 Frame loop: re-render on state change

**Tests**:
- `rover.text { "Hello" }` visible in window
- `rover.button { "Click" }` shows rect with text

**Deliverable**: main.lua counter UI renders visually (static)

---

### Phase 7: State & Actions
**Goal**: State management, actions trigger re-render

**Tasks**:
- [ ] 7.1 Call `app.init()` on startup, store state in Zig
- [ ] 7.2 Build `act` table: wrap each custom function as callable
- [ ] 7.3 `on_click` stores action reference
- [ ] 7.4 On click event: call action with state, get new state
- [ ] 7.5 Re-call `render(state, act)`, diff/rebuild tree, re-render

**Tests**:
- Click "Increase" → state = 1 → text updates
- Click "Decrease" → state = -1

**Deliverable**: Counter app fully functional on macOS

---

### Phase 8: Event System
**Goal**: Mouse/touch input routed to Lua actions

**Tasks**:
- [ ] 8.1 macOS: capture mouse click events
- [ ] 8.2 Hit testing: which node was clicked (using layout bounds)
- [ ] 8.3 Trigger `on_click` if node has handler
- [ ] 8.4 Abstract input: `InputEvent { type, x, y }`

**Tests**:
- Click inside button bounds → action fires
- Click outside → nothing

**Deliverable**: Interactive counter on macOS

---

### Phase 9: iOS Build Pipeline
**Goal**: `rover main.lua -p ios` builds & runs on simulator

**Tasks**:
- [ ] 9.1 iOS backend: UIKit app with Metal Skia surface
- [ ] 9.2 Zig cross-compile to iOS arm64 + x86_64 (simulator)
- [ ] 9.3 Template Xcode project (embed in rover binary or generate)
- [ ] 9.4 Use XcodeProjectCLI to generate .xcodeproj
- [ ] 9.5 Bundle: Lua file + Zig static lib → .app
- [ ] 9.6 `xcodebuild` + `xcrun simctl` to build & launch simulator
- [ ] 9.7 CLI: `rover main.lua -p ios` does all above

**Tests**:
- `rover main.lua -p ios` opens simulator with counter
- Touch events work

**Deliverable**: iOS simulator running main.lua

---

### Phase 10: Hot Reload (Nice-to-have)
**Goal**: Edit Lua, see changes without restart

**Tasks**:
- [ ] 10.1 File watcher (fswatch or Zig polling)
- [ ] 10.2 On change: re-execute Lua, rebuild tree, re-render
- [ ] 10.3 Preserve state across reloads (optional)

**Tests**:
- Change text in Lua → window updates in <1s

**Deliverable**: Dev loop feels instant

---

### Phase 11: Android (Future)
**Goal**: `rover main.lua -p android`

**Tasks**:
- [ ] 11.1 Android backend: NativeActivity + Skia
- [ ] 11.2 Zig cross-compile to Android NDK
- [ ] 11.3 Gradle template project
- [ ] 11.4 APK bundling
- [ ] 11.5 `adb install` + launch

---

## Directory Structure

```
rover/
├── build.zig
├── src/
│   ├── main.zig           # CLI entry
│   ├── runtime/
│   │   ├── lua_vm.zig     # Lua 5.4 via ziglua
│   │   ├── rover_api.zig  # rover.* Lua bindings
│   │   └── state.zig      # App state management
│   ├── render/
│   │   ├── skia.zig       # Skia C wrapper
│   │   ├── canvas.zig     # High-level draw API
│   │   └── renderer.zig   # Node tree → draw calls
│   ├── layout/
│   │   ├── node.zig       # Node struct
│   │   ├── layout.zig     # Flexbox algorithm
│   │   └── measure.zig    # Text measurement
│   ├── platform/
│   │   ├── platform.zig   # Abstract interface
│   │   ├── macos.zig      # Cocoa backend
│   │   ├── ios.zig        # UIKit backend
│   │   └── android.zig    # NDK backend (future)
│   └── cli/
│       ├── args.zig       # Arg parsing
│       └── build_ios.zig  # iOS project generation
├── templates/
│   └── ios/               # Xcode project template
├── vendor/
│   └── skia/              # Prebuilt Skia libs
├── test/
│   ├── test_lua.zig
│   ├── test_layout.zig
│   └── test_skia.zig
└── examples/
    └── main.lua           # Counter example
```

---

## Testing Strategy

| Type | Tool | Scope |
|------|------|-------|
| Unit | `zig build test` | Layout math, node parsing, Skia draw calls |
| Integration | Manual + screenshots | Window renders correctly |
| E2E | `rover examples/main.lua` | Full app runs, interactive |
| CI | GitHub Actions | macOS + Linux (no iOS in CI initially) |

---

## Open Questions / Risks

1. **Skia prebuilt availability**: Need arm64 + x64 macOS, arm64 iOS. May need to build ourselves eventually.

2. **Skia C API completeness**: Skia C API (`include/c/`) is limited. May need C++ shim for some features.

3. **iOS code signing**: Simulator = no signing. Device = needs provisioning. Start with simulator only.

4. **Zig + iOS cross-compile**: Supported but may have edge cases. Need to test early.

5. **Text shaping**: Skia basic text is simple. Complex scripts (Arabic, Hindi) need HarfBuzz later.

---

## Milestones Summary

| Milestone | Phases | Outcome |
|-----------|--------|---------|
| M1: Foundation | 1-2 | CLI loads Lua, Skia draws to PNG |
| M2: Visual | 3-4 | macOS window with Lua-defined nodes |
| M3: Interactive | 5-8 | Counter app works on macOS |
| M4: Mobile | 9 | iOS simulator works |
| M5: DX | 10 | Hot reload |
| M6: Android | 11 | Android works |
