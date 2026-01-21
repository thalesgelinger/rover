# Phase 0: Event Loop Refactoring + Platform Abstractions

**Status:** ✅ Complete
**Duration:** 1 week
**Dependencies:** None

## Objective

Extract event loop from rover-server to create platform-agnostic abstractions before implementing TUI renderer. This was necessary to allow the signal system to work across different platforms (TUI, Web, iOS, Android).

## Deliverables

### 0.1 Platform Abstractions (rover-ui/src/platform/)

**Files:**
- `mod.rs` - Module export
- `tui.rs` - Platform-specific implementation

**Created Types:**

```rust
pub enum PlatformEvent {
    KeyDown { key: String, mods: KeyModifiers },
    KeyUp { key: String },
    Mouse { x: u16, y: u16, event: MouseEvent },
    Resize { width: u16, height: u16 },
    Tick,
    Quit,
}

pub trait PlatformHandler {
    fn init(&mut self) -> Result<()>;
    fn wait_for_event(&mut self, timeout_ms: u64) -> Vec<PlatformEvent>;
    fn render(&mut self) -> Result<()>;
    fn cleanup(&mut self);
}

pub struct TuiPlatform {
    terminal: Terminal,
    events: Vec<crossterm::event::Event>,
}
```

### 0.2 Generic Event Loop (rover-core/src/lib.rs)

**Created:**
- `GenericEventLoop<Handler>` - Platform-agnostic event loop with handler trait
- Key bindings as `HashMap<String, SignalId>` (uses SignalId/u32 instead of RegistryKey to avoid FromLua constraints)
- Platform auto-detection: UserData → TUI, Table with metadata → HTTP Server

**Design decisions:**
- Single-threaded design (stores `&Lua` directly, no Send/Sync needed)
- 16ms timeout (~60fps target)
- Signal mutations for everything (no imperative commands)

### 0.3 TUI Renderer Refactoring (rover-ui/src/renderer/tui.rs)

**Methods added:**
- `init()` - Initialize terminal
- `render()` - Render UI state
- `resize()` - Handle terminal resize
- `cleanup()` - Clean up terminal

**Changes:**
- `read_signal()` → `read_signal_bool()` at line 279

### 0.4 Signal Runtime Extensions (rover-ui/src/signal/runtime.rs)

**New methods:**
- `read_signal_display(id) -> Option<String>` - Read signal as display string
- `read_signal_bool(id) -> Option<bool>` - Read signal as boolean

### 0.5 Signal Value Fix (rover-ui/src/signal/value.rs)

**Change:**
- Wrapped `RegistryKey` in `Rc<RegistryKey>` to fix Clone issues

## Test Results

✅ `cargo build` successful (0 errors, only warnings)
✅ All crates compile successfully

## What's Next

Phase 2A: Verify and test TUI renderer with example/counter.lua
Phase 2B-D: Complete TUI renderer implementation (signal caching, list rendering, key bindings)

## Unresolved Questions

- Should we add key binding tests in Phase 2?
- Do we need a separate test for platform auto-detection?
