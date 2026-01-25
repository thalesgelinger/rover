# Rover UI: Implementation Roadmap

## Overview

Multi-phased implementation plan for building a cross-platform Lua UI runtime. Core foundation must be solid before any platform-specific work begins.

---

## Phase 0: Core Foundation (StubRenderer Testing)

> **Goal:** Complete the core UI system with async primitives, testable via StubRenderer.

### 0.1 Scheduler & Async Primitives

**Files to create:**
- `rover-ui/src/scheduler/mod.rs` - Module entry
- `rover-ui/src/scheduler/timer.rs` - Timer queue (BinaryHeap)
- `rover-ui/src/scheduler/channel.rs` - IO completion channel

**Scheduler struct:**
```rust
pub struct Scheduler {
    timers: BinaryHeap<TimerTask>,          // Min-heap by wake_time
    io_receiver: mpsc::Receiver<IoCompletion>,
    io_sender: mpsc::Sender<IoCompletion>,  // Clone for background threads
    pending_coroutines: HashMap<CoroutineId, PendingCoroutine>,
}
```

**Core methods:**
- `schedule_delay(thread, delay_ms)` - Add timer
- `spawn_blocking(f, thread)` - Run on background thread, resume when done
- `tick(lua, runtime) -> bool` - Process ready timers/IO, return true if work done
- `next_wake_time() -> Option<Instant>` - For event loop timeout

**Lua API:**
- `rover.delay(ms)` - Yield coroutine, resume after ms

**Unit tests:**
- Timer scheduling and ordering
- Timer cancellation
- Multiple timers with same deadline
- Background task completion via channel

### 0.2 Coroutine Runner

**Files to modify:**
- `rover-ui/src/lib.rs` - Export scheduler
- `rover-ui/src/lua/mod.rs` - Register rover.delay

**Key pattern:** Same as rover-server's `execute_handler_coroutine`:
```rust
fn run_lua_coroutine(lua: &Lua, func: Function) -> CoroutineResult {
    let thread = lua.create_thread(func)?;
    match thread.resume::<()>(()) {
        Ok(_) => CoroutineResult::Completed,
        Err(e) if e.is_yield() => CoroutineResult::Yielded(thread),
        Err(e) => CoroutineResult::Error(e),
    }
}
```

**Unit tests:**
- Simple coroutine runs to completion
- Coroutine yields and resumes
- Error propagation
- Nested coroutines

### 0.3 Application Loop

**File to create:**
- `rover-ui/src/app.rs` - Main application runner

```rust
pub struct App<R: Renderer> {
    lua: Lua,
    runtime: SignalRuntime,
    registry: UiRegistry,
    scheduler: Scheduler,
    renderer: R,
}

impl<R: Renderer> App<R> {
    pub fn new(renderer: R) -> Self;
    pub fn run_script(&mut self, code: &str) -> Result<()>;
    pub fn tick(&mut self) -> bool;  // Returns true if should continue
    pub fn run(&mut self);           // Blocking loop
}
```

**Unit tests (with StubRenderer):**
- App runs Lua script
- Signal changes trigger render
- rover.delay works correctly
- Multiple coroutines interleave properly

### 0.4 Event System Foundation

**Files to create:**
- `rover-ui/src/events/mod.rs` - Event types
- `rover-ui/src/events/queue.rs` - Event queue

**Event types:**
```rust
pub enum UiEvent {
    Click { node_id: NodeId },
    Input { node_id: NodeId, text: String },
    Focus { node_id: NodeId },
    Blur { node_id: NodeId },
    KeyPress { key: Key, modifiers: Modifiers },
    Custom { name: String, data: LuaValue },
}
```

**Event handler registry:**
```rust
pub struct EventHandlers {
    handlers: HashMap<(NodeId, EventType), EffectId>,
}
```

**Unit tests:**
- Event dispatch to correct handler
- Handler removal on node removal
- Event bubbling (if implemented)

### 0.5 Core Components

**Files to modify:**
- `rover-ui/src/ui/node.rs` - Add new node types
- `rover-ui/src/ui/ui.rs` - Lua bindings for new components

**New components:**
```rust
pub enum UiNode {
    Text { content: TextContent },
    Column { children: Vec<NodeId> },
    Row { children: Vec<NodeId> },
    View { children: Vec<NodeId> },
    // NEW:
    Button { label: TextContent, on_click: Option<EffectId> },
    Input { value: TextContent, on_change: Option<EffectId> },
    Checkbox { checked: bool, on_toggle: Option<EffectId> },
    Image { source: ImageSource },
}
```

**Lua API:**
```lua
rover.ui.button { label = "Click me", on_click = function() ... end }
rover.ui.input { value = signal, on_change = function(text) ... end }
rover.ui.checkbox { checked = signal, on_toggle = function(val) ... end }
rover.ui.image { source = "path/to/image.png" }
```

**Unit tests (StubRenderer):**
- Button creation with handlers
- Input value changes
- Checkbox toggle
- All components rendered correctly in stub

### 0.6 Conditional & List Rendering

**Files to modify:**
- `rover-ui/src/ui/ui.rs` - Add if/each helpers

**Lua API:**
```lua
-- Conditional (returns node or nil)
rover.ui.when(condition_signal, rover.ui.text { "Visible when true" })

-- List rendering with key
rover.ui.each(items_signal, function(item, index)
    return rover.ui.text { item.name }
end, function(item) return item.id end)  -- key function
```

**Implementation:**
- `when` creates effect that mounts/unmounts child
- `each` tracks keys, diffs list, adds/removes/reorders nodes

**Unit tests:**
- Conditional shows/hides based on signal
- List adds items
- List removes items
- List reorders items (keyed)
- Nested conditionals

### Phase 0 Deliverables

| Item | Status | Test Coverage |
|------|--------|---------------|
| Scheduler | [ ] | [ ] Timer tests |
| rover.delay() | [ ] | [ ] Coroutine tests |
| App loop | [ ] | [ ] Integration tests |
| Event system | [ ] | [ ] Event dispatch tests |
| Button component | [ ] | [ ] StubRenderer tests |
| Input component | [ ] | [ ] StubRenderer tests |
| Checkbox component | [ ] | [ ] StubRenderer tests |
| Conditional rendering | [ ] | [ ] Show/hide tests |
| List rendering | [ ] | [ ] Add/remove/reorder tests |

### Phase 0 Verification

```bash
# All tests pass
cargo test -p rover-ui

```

<ALSO>
    Create some example files on ./examples, and test it using `cargo run -p rover_cli -- examples/<the files you created>.lua`
</ALSO>

---

## Milestone: TUI (Terminal UI)

> **Goal:** Full terminal-based UI with crossterm + ratatui.

### TUI.1 Platform Loop

**Files to create:**
- `rover-tui/Cargo.toml` - New crate
- `rover-tui/src/lib.rs` - Entry point
- `rover-tui/src/event_loop.rs` - mio + crossterm integration

**Dependencies:**
```toml
[dependencies]
rover-ui = { path = "../rover-ui" }
crossterm = "0.27"
ratatui = "0.26"
mio = "1.0"
```

**Event loop:**
```rust
pub struct TuiLoop {
    poll: Poll,
    terminal: Terminal<CrosstermBackend<Stdout>>,
    crossterm_events: EventStream,  // From crossterm
}

impl TuiLoop {
    fn run(&mut self, app: &mut App<TuiRenderer>) {
        loop {
            // Poll mio for: terminal events, timer wakeups
            let timeout = app.scheduler.next_wake_time();
            self.poll.poll(&mut events, timeout)?;

            // Handle terminal events (keyboard, resize)
            for event in crossterm_events {
                app.dispatch_event(convert_event(event));
            }

            // Tick scheduler (timers, IO completions)
            app.tick();

            // Render if dirty
            if app.registry.has_dirty_nodes() {
                self.terminal.draw(|f| app.renderer.render(f))?;
            }
        }
    }
}
```

### TUI.2 TuiRenderer

**Files to create:**
- `rover-tui/src/renderer.rs` - Renderer implementation

```rust
pub struct TuiRenderer {
    layout_cache: HashMap<NodeId, Rect>,
}

impl Renderer for TuiRenderer {
    fn mount(&mut self, registry: &UiRegistry) {
        // Build initial layout
        self.layout(registry);
    }

    fn update(&mut self, registry: &UiRegistry, dirty_nodes: &[NodeId]) {
        // Recalculate affected layouts
        // Mark regions for redraw
    }
}

impl TuiRenderer {
    fn render(&self, frame: &mut Frame, registry: &UiRegistry) {
        self.render_node(frame, registry, registry.root(), frame.size());
    }

    fn render_node(&self, frame: &mut Frame, registry: &UiRegistry, id: NodeId, area: Rect) {
        match registry.get(id) {
            UiNode::Text { content } => {
                let text = content.current_value();
                frame.render_widget(Paragraph::new(text), area);
            }
            UiNode::Column { children } => {
                // Vertical layout
                let layout = Layout::vertical(/* ... */);
                for (child, rect) in children.zip(layout.areas()) {
                    self.render_node(frame, registry, *child, rect);
                }
            }
            // ... other node types
        }
    }
}
```

### TUI.3 Layout Engine

**Files to create:**
- `rover-tui/src/layout.rs` - Flexbox-like layout

**Layout algorithm:**
- Column: Stack children vertically, divide height
- Row: Stack children horizontally, divide width
- View: Single child fills area
- Text: Measure text, report size

### TUI.4 Input Handling

**Key mappings:**
- Arrow keys → Focus navigation
- Enter/Space → Button activation
- Tab → Focus next
- Escape → Back/Cancel
- Any char → Text input (if focused)

### TUI.5 Components Mapping

| Component | ratatui Widget |
|-----------|----------------|
| Text | Paragraph |
| Column | Layout::vertical |
| Row | Layout::horizontal |
| View | Block |
| Button | Paragraph + Block (highlighted on focus) |
| Input | Paragraph with cursor |
| Checkbox | `[x]` or `[ ]` text |
| Image | Not supported (placeholder text) |

### TUI Deliverables

| Item | Status |
|------|--------|
| rover-tui crate setup | [ ] |
| mio + crossterm event loop | [ ] |
| TuiRenderer | [ ] |
| Layout engine | [ ] |
| Keyboard navigation | [ ] |
| Text input handling | [ ] |
| Button click handling | [ ] |
| Counter example working | [ ] |

### TUI Verification

```bash
# Run TUI example
cargo run -p rover-tui --example counter

# Run with timer
cargo run -p rover-tui --example timer
```

---

## Milestone: Web (WASM)

> **Goal:** Run Rover UI in browser via WebAssembly.

### Web.1 Crate Setup

**Files to create:**
- `rover-web/Cargo.toml`
- `rover-web/src/lib.rs`

**Dependencies:**
```toml
[dependencies]
rover-ui = { path = "../rover-ui" }
wasm-bindgen = "0.2"
web-sys = { version = "0.3", features = ["Document", "Element", "HtmlElement", "Window"] }
js-sys = "0.3"
```

### Web.2 Platform Loop

**No mio needed - use JS event loop:**

```rust
#[wasm_bindgen]
pub fn start(canvas_id: &str) {
    // Setup panic hook for better errors
    console_error_panic_hook::set_once();

    // Create app
    let app = Rc::new(RefCell::new(App::new(WebRenderer::new(canvas_id))));

    // Schedule tick via requestAnimationFrame
    let tick = Rc::new(RefCell::new(None::<Closure<dyn FnMut()>>));
    let tick_clone = tick.clone();
    let app_clone = app.clone();

    *tick.borrow_mut() = Some(Closure::new(move || {
        app_clone.borrow_mut().tick();
        request_animation_frame(tick_clone.borrow().as_ref().unwrap());
    }));

    request_animation_frame(tick.borrow().as_ref().unwrap());
}
```

### Web.3 WebRenderer

```rust
pub struct WebRenderer {
    document: Document,
    container: HtmlElement,
    node_elements: HashMap<NodeId, HtmlElement>,
}

impl Renderer for WebRenderer {
    fn mount(&mut self, registry: &UiRegistry) {
        let root_el = self.create_element(registry, registry.root());
        self.container.append_child(&root_el);
    }

    fn update(&mut self, registry: &UiRegistry, dirty_nodes: &[NodeId]) {
        for &id in dirty_nodes {
            if let Some(el) = self.node_elements.get(&id) {
                self.update_element(el, registry.get(id));
            }
        }
    }
}
```

### Web.4 DOM Mapping

| Component | DOM Element |
|-----------|-------------|
| Text | `<span>` |
| Column | `<div style="display:flex;flex-direction:column">` |
| Row | `<div style="display:flex;flex-direction:row">` |
| View | `<div>` |
| Button | `<button>` |
| Input | `<input type="text">` |
| Checkbox | `<input type="checkbox">` |
| Image | `<img>` |

### Web.5 Event Binding

```rust
fn bind_click(&mut self, node_id: NodeId, element: &HtmlElement) {
    let app = self.app.clone();
    let closure = Closure::new(move |_: Event| {
        app.borrow_mut().dispatch_event(UiEvent::Click { node_id });
    });
    element.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref());
    closure.forget();  // Leak closure (bound for element lifetime)
}
```

### Web.6 Timer via setTimeout

```rust
// In scheduler, for web platform:
fn schedule_delay_web(&self, delay_ms: u64, waker: Waker) {
    let closure = Closure::once(move || {
        waker.wake();
    });
    window().set_timeout_with_callback_and_timeout_and_arguments_0(
        closure.as_ref().unchecked_ref(),
        delay_ms as i32,
    );
    closure.forget();
}
```

### Web Deliverables

| Item | Status |
|------|--------|
| rover-web crate setup | [ ] |
| WASM build working | [ ] |
| requestAnimationFrame loop | [ ] |
| WebRenderer | [ ] |
| DOM element creation | [ ] |
| Event binding (click, input) | [ ] |
| setTimeout for rover.delay | [ ] |
| Counter example in browser | [ ] |

### Web Verification

```bash
# Build WASM
wasm-pack build rover-web --target web

# Serve example
cd rover-web/www && python -m http.server 8080

# Open http://localhost:8080
```

---

## Milestone: iOS

> **Goal:** Native iOS app with UIKit.

### iOS.1 Project Structure

```
rover-ios/
├── Cargo.toml              # Rust library (cdylib)
├── src/
│   ├── lib.rs
│   ├── renderer.rs
│   └── ffi.rs              # C-compatible FFI
├── RoverApp/               # Xcode project
│   ├── RoverApp.xcodeproj
│   ├── Sources/
│   │   ├── RoverBridge.swift   # Swift ↔ Rust
│   │   ├── RoverView.swift     # UIKit integration
│   │   └── AppDelegate.swift
│   └── rover.h             # Generated C header
```

### iOS.2 Rust FFI

```rust
// ffi.rs
#[no_mangle]
pub extern "C" fn rover_create_app() -> *mut App<IosRenderer> {
    Box::into_raw(Box::new(App::new(IosRenderer::new())))
}

#[no_mangle]
pub extern "C" fn rover_tick(app: *mut App<IosRenderer>) -> bool {
    let app = unsafe { &mut *app };
    app.tick()
}

#[no_mangle]
pub extern "C" fn rover_dispatch_event(app: *mut App<IosRenderer>, event: *const UiEventFfi) {
    // ...
}
```

### iOS.3 Platform Loop (GCD)

```swift
// RoverView.swift
class RoverView: UIView {
    private var app: UnsafeMutableRawPointer?
    private var displayLink: CADisplayLink?

    func start() {
        app = rover_create_app()

        displayLink = CADisplayLink(target: self, selector: #selector(tick))
        displayLink?.add(to: .main, forMode: .common)
    }

    @objc func tick() {
        rover_tick(app)
        // Render changes
    }
}
```

### iOS.4 IosRenderer

**Challenge:** UIKit views must be created/updated on main thread.

**Solution:** Renderer returns "operations" that Swift executes:

```rust
pub enum RenderOp {
    CreateView { node_id: u32, view_type: ViewType },
    UpdateText { node_id: u32, text: String },
    RemoveView { node_id: u32 },
    // ...
}

impl Renderer for IosRenderer {
    fn update(&mut self, registry: &UiRegistry, dirty_nodes: &[NodeId]) {
        for &id in dirty_nodes {
            self.ops.push(RenderOp::UpdateText { ... });
        }
    }
}
```

Swift polls ops and applies:

```swift
func applyRenderOps() {
    while let op = rover_next_render_op(app) {
        switch op.type {
        case .createView:
            let view = createUIView(for: op)
            views[op.nodeId] = view
        case .updateText:
            (views[op.nodeId] as? UILabel)?.text = op.text
        // ...
        }
    }
}
```

### iOS.5 Component Mapping

| Component | UIKit View |
|-----------|------------|
| Text | UILabel |
| Column | UIStackView (axis: .vertical) |
| Row | UIStackView (axis: .horizontal) |
| View | UIView |
| Button | UIButton |
| Input | UITextField |
| Checkbox | UISwitch |
| Image | UIImageView |

### iOS.6 Timer via DispatchQueue

```swift
// For rover.delay on iOS
func scheduleTimer(delayMs: UInt64, callback: @escaping () -> Void) {
    DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(Int(delayMs))) {
        callback()
    }
}
```

### iOS Deliverables

| Item | Status |
|------|--------|
| rover-ios crate setup | [ ] |
| C FFI layer | [ ] |
| Xcode project setup | [ ] |
| Swift bridge | [ ] |
| CADisplayLink loop | [ ] |
| IosRenderer | [ ] |
| RenderOp pattern | [ ] |
| UIKit view creation | [ ] |
| Event forwarding | [ ] |
| Counter example on simulator | [ ] |

### iOS Verification

```bash
# Build Rust library
cargo build -p rover-ios --target aarch64-apple-ios

# Open Xcode
open rover-ios/RoverApp/RoverApp.xcodeproj

# Run on simulator
```

---

## Milestone: Android

> **Goal:** Native Android app with Android Views.

### Android.1 Project Structure

```
rover-android/
├── Cargo.toml              # Rust library
├── src/
│   ├── lib.rs
│   ├── renderer.rs
│   └── jni.rs              # JNI bindings
├── app/                    # Android Studio project
│   ├── build.gradle.kts
│   └── src/main/
│       ├── java/com/rover/
│       │   ├── RoverLib.kt     # JNI declarations
│       │   ├── RoverView.kt    # Custom View
│       │   └── MainActivity.kt
│       └── jniLibs/            # Compiled .so files
```

### Android.2 Rust JNI

```rust
// jni.rs
#[no_mangle]
pub extern "system" fn Java_com_rover_RoverLib_createApp(
    env: JNIEnv,
    _class: JClass,
) -> jlong {
    let app = Box::new(App::new(AndroidRenderer::new()));
    Box::into_raw(app) as jlong
}

#[no_mangle]
pub extern "system" fn Java_com_rover_RoverLib_tick(
    env: JNIEnv,
    _class: JClass,
    app_ptr: jlong,
) -> jboolean {
    let app = unsafe { &mut *(app_ptr as *mut App<AndroidRenderer>) };
    app.tick() as jboolean
}
```

### Android.3 Platform Loop (Handler)

```kotlin
// RoverView.kt
class RoverView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null
) : FrameLayout(context, attrs) {

    private var appPtr: Long = 0
    private val handler = Handler(Looper.getMainLooper())

    fun start() {
        appPtr = RoverLib.createApp()
        scheduleFrame()
    }

    private fun scheduleFrame() {
        handler.post {
            RoverLib.tick(appPtr)
            applyRenderOps()
            scheduleFrame()  // Continuous loop
        }
    }
}
```

### Android.4 AndroidRenderer

Same RenderOp pattern as iOS:

```kotlin
fun applyRenderOps() {
    while (true) {
        val op = RoverLib.nextRenderOp(appPtr) ?: break
        when (op.type) {
            RenderOpType.CREATE_VIEW -> {
                val view = createView(op)
                views[op.nodeId] = view
            }
            RenderOpType.UPDATE_TEXT -> {
                (views[op.nodeId] as? TextView)?.text = op.text
            }
            // ...
        }
    }
}
```

### Android.5 Component Mapping

| Component | Android View |
|-----------|--------------|
| Text | TextView |
| Column | LinearLayout (orientation: VERTICAL) |
| Row | LinearLayout (orientation: HORIZONTAL) |
| View | FrameLayout |
| Button | MaterialButton |
| Input | EditText |
| Checkbox | MaterialCheckBox |
| Image | ImageView |

### Android.6 Timer via Handler

```kotlin
fun scheduleTimer(delayMs: Long, callback: () -> Unit) {
    handler.postDelayed(callback, delayMs)
}
```

### Android Deliverables

| Item | Status |
|------|--------|
| rover-android crate setup | [ ] |
| JNI layer | [ ] |
| Android Studio project | [ ] |
| Kotlin bridge | [ ] |
| Handler loop | [ ] |
| AndroidRenderer | [ ] |
| RenderOp pattern | [ ] |
| Android View creation | [ ] |
| Event forwarding | [ ] |
| Counter example on emulator | [ ] |

### Android Verification

```bash
# Build Rust library
cargo ndk -t arm64-v8a build -p rover-android --release

# Open Android Studio
# Run on emulator
```

---

## Future Milestones (Not Planned Yet)

### Windows (Win32/WinUI)
- DirectX or WinUI 3
- Message pump integration

### Linux (GTK)
- GTK4 bindings
- GLib main loop

### macOS (AppKit)
- NSView hierarchy
- NSRunLoop integration

---

## Test Strategy

### Unit Tests (Phase 0)

```
rover-ui/src/
├── scheduler/
│   └── tests.rs        # Timer, channel tests
├── events/
│   └── tests.rs        # Event dispatch tests
└── ui/
    └── tests.rs        # Component tests with StubRenderer
```

### Integration Tests

```
rover-ui/tests/
├── counter.rs          # Counter app lifecycle
├── timer.rs            # rover.delay behavior
├── events.rs           # Click, input events
├── conditional.rs      # when() show/hide
└── list.rs             # each() add/remove/reorder
```

### Example Files

```
examples/
├── counter.lua         # Basic counter
├── timer.lua           # Counter with rover.delay
├── form.lua            # Input, button, validation
├── list.lua            # Dynamic list
└── conditional.lua     # Show/hide based on state
```

### Platform Tests

Each platform milestone has:
- `examples/counter` - Basic counter
- `examples/timer` - rover.delay works
- `examples/input` - Text input works

---

## Critical Files Summary

### Phase 0 (Core)

| File | Purpose |
|------|---------|
| `rover-ui/src/scheduler/mod.rs` | Timer queue, IO channel |
| `rover-ui/src/app.rs` | Main application runner |
| `rover-ui/src/events/mod.rs` | Event system |
| `rover-ui/src/ui/node.rs` | Component types |
| `rover-ui/src/ui/ui.rs` | Lua component API |
| `rover-ui/src/style/mod.rs` | Style types |

### TUI Milestone

| File | Purpose |
|------|---------|
| `rover-tui/src/event_loop.rs` | mio + crossterm |
| `rover-tui/src/renderer.rs` | TuiRenderer |
| `rover-tui/src/layout.rs` | Flexbox layout |

### Web Milestone

| File | Purpose |
|------|---------|
| `rover-web/src/lib.rs` | WASM entry |
| `rover-web/src/renderer.rs` | WebRenderer |

### iOS Milestone

| File | Purpose |
|------|---------|
| `rover-ios/src/ffi.rs` | C FFI |
| `rover-ios/src/renderer.rs` | IosRenderer |
| `RoverApp/Sources/RoverBridge.swift` | Swift bridge |

### Android Milestone

| File | Purpose |
|------|---------|
| `rover-android/src/jni.rs` | JNI bindings |
| `rover-android/src/renderer.rs` | AndroidRenderer |
| `app/src/main/java/.../RoverLib.kt` | Kotlin bridge |

