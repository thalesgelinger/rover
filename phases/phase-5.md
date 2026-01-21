# Fase 5: Events e Interatividade

**Status:** Not Started
**Duration:** 1-2 semanas
**Dependencies:** Fase 4

## Agent Context

### Prerequisites
- Phase 4 must be complete (Modifiers working)
- Event system builds on modifier infrastructure for event-triggered styles
- Input components use signals for two-way binding

### Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                     Platform Events                              │
│   TUI: crossterm::event  |  Web: addEventListener               │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    PlatformEvent                                 │
│   KeyDown { key, modifiers } | MouseDown { x, y, button }       │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Event Router                                  │
│   Hit testing → Find target node → Route to handlers            │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Lua Callback                                  │
│   on_press = function() count.val = count.val + 1 end           │
└─────────────────────────────────────────────────────────────────┘
```

### Key Concepts

1. **EventType**: Universal event names that work across platforms
2. **Focus Management**: Track which node has keyboard focus
3. **Two-Way Binding**: Signals automatically sync with input values
4. **Hit Testing**: Determine which node is under pointer coordinates

## Objetivo

Implementar sistema de eventos consistente entre plataformas.

## Entregas

### 5.1 Event Types

```rust
// rover-ui/src/event/types.rs
use crate::node::NodeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
    // Interaction events
    Press,
    LongPress,
    Release,

    // Pointer events (desktop/web)
    Hover,
    HoverEnd,
    PointerMove,

    // Focus events (accessibility)
    Focus,
    Blur,

    // Input events
    TextInput,
    KeyDown,
    KeyUp,

    // Lifecycle events
    Mount,
    Unmount,
    Visible,    // entered viewport
    Hidden,     // left viewport
}

#[derive(Debug, Clone)]
pub struct Event {
    pub event_type: EventType,
    pub target: NodeId,
    pub data: EventData,
    pub propagation_stopped: bool,
}

#[derive(Debug, Clone)]
pub enum EventData {
    None,
    Pointer { x: u16, y: u16 },
    Key { key: String, modifiers: Vec<KeyModifier> },
    Text { value: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyModifier {
    Shift,
    Control,
    Alt,
    Meta,
}

impl Event {
    pub fn stop_propagation(&mut self) {
        self.propagation_stopped = true;
    }
}
```

### 5.2 Event Router & Focus Management

```rust
// rover-ui/src/event/router.rs
use crate::node::{NodeId, NodeArena};
use crate::layout::{LayoutEngine, Rect};
use super::types::{Event, EventType, EventData};

pub struct EventRouter {
    focused_node: Option<NodeId>,
    hovered_node: Option<NodeId>,
    pressed_node: Option<NodeId>,
}

impl EventRouter {
    pub fn new() -> Self {
        Self {
            focused_node: None,
            hovered_node: None,
            pressed_node: None,
        }
    }

    pub fn focused(&self) -> Option<NodeId> {
        self.focused_node
    }

    pub fn set_focus(&mut self, node: Option<NodeId>) -> Vec<Event> {
        let mut events = vec![];

        // Blur old focused node
        if let Some(old_focus) = self.focused_node {
            if Some(old_focus) != node {
                events.push(Event {
                    event_type: EventType::Blur,
                    target: old_focus,
                    data: EventData::None,
                    propagation_stopped: false,
                });
            }
        }

        // Focus new node
        if let Some(new_focus) = node {
            if Some(new_focus) != self.focused_node {
                events.push(Event {
                    event_type: EventType::Focus,
                    target: new_focus,
                    data: EventData::None,
                    propagation_stopped: false,
                });
            }
        }

        self.focused_node = node;
        events
    }

    pub fn hit_test(&self, x: u16, y: u16, arena: &NodeArena, layout: &LayoutEngine, root: NodeId) -> Option<NodeId> {
        self.hit_test_node(x, y, arena, layout, root)
    }

    fn hit_test_node(&self, x: u16, y: u16, arena: &NodeArena, layout: &LayoutEngine, node: NodeId) -> Option<NodeId> {
        let rect = layout.get_layout(node)?;

        if !rect.rect.contains(x, y) {
            return None;
        }

        // Check children first (top to bottom in render order)
        let children = arena.children(node);
        for child in children.into_iter().rev() {
            if let Some(hit) = self.hit_test_node(x, y, arena, layout, child) {
                return Some(hit);
            }
        }

        // If no child was hit, this node is the target
        Some(node)
    }

    pub fn handle_pointer_down(&mut self, x: u16, y: u16, arena: &NodeArena, layout: &LayoutEngine, root: NodeId) -> Vec<Event> {
        let mut events = vec![];

        if let Some(target) = self.hit_test(x, y, arena, layout, root) {
            self.pressed_node = Some(target);

            // Set focus to clicked node (if focusable)
            events.extend(self.set_focus(Some(target)));

            events.push(Event {
                event_type: EventType::Press,
                target,
                data: EventData::Pointer { x, y },
                propagation_stopped: false,
            });
        }

        events
    }

    pub fn handle_pointer_up(&mut self, x: u16, y: u16, arena: &NodeArena, layout: &LayoutEngine, root: NodeId) -> Vec<Event> {
        let mut events = vec![];

        if let Some(pressed) = self.pressed_node.take() {
            events.push(Event {
                event_type: EventType::Release,
                target: pressed,
                data: EventData::Pointer { x, y },
                propagation_stopped: false,
            });
        }

        events
    }

    pub fn handle_pointer_move(&mut self, x: u16, y: u16, arena: &NodeArena, layout: &LayoutEngine, root: NodeId) -> Vec<Event> {
        let mut events = vec![];
        let new_hover = self.hit_test(x, y, arena, layout, root);

        // HoverEnd on old node
        if let Some(old_hover) = self.hovered_node {
            if new_hover != Some(old_hover) {
                events.push(Event {
                    event_type: EventType::HoverEnd,
                    target: old_hover,
                    data: EventData::Pointer { x, y },
                    propagation_stopped: false,
                });
            }
        }

        // Hover on new node
        if let Some(new_node) = new_hover {
            if self.hovered_node != Some(new_node) {
                events.push(Event {
                    event_type: EventType::Hover,
                    target: new_node,
                    data: EventData::Pointer { x, y },
                    propagation_stopped: false,
                });
            }
        }

        self.hovered_node = new_hover;
        events
    }

    pub fn handle_key_down(&mut self, key: String, modifiers: Vec<KeyModifier>) -> Vec<Event> {
        let mut events = vec![];

        if let Some(focused) = self.focused_node {
            events.push(Event {
                event_type: EventType::KeyDown,
                target: focused,
                data: EventData::Key { key, modifiers },
                propagation_stopped: false,
            });
        }

        events
    }
}

impl Default for EventRouter {
    fn default() -> Self {
        Self::new()
    }
}
```

### 5.3 Input Components

```rust
// rover-ui/src/lua/input.rs
use mlua::{Lua, Function, Result, Table, Value};
use crate::node::{Node, NodeId};
use crate::lua::helpers::get_runtime;

// Node type for button
pub struct ButtonNode {
    pub text: String,
    pub on_press: Option<mlua::RegistryKey>,
    pub disabled: bool,
}

// Node type for text input
pub struct InputNode {
    pub value_signal: crate::signal::SignalId,
    pub placeholder: String,
    pub secure: bool,
    pub on_submit: Option<mlua::RegistryKey>,
    pub on_focus: Option<mlua::RegistryKey>,
    pub on_blur: Option<mlua::RegistryKey>,
}

fn create_button_fn(lua: &Lua) -> Result<Function> {
    lua.create_function(|lua, args: Table| -> Result<crate::lua::node::LuaNode> {
        let runtime = get_runtime(lua)?;

        let text: String = args.get("text").unwrap_or_default();
        let on_press: Option<Function> = args.get("on_press").ok();

        let on_press_key = if let Some(f) = on_press {
            Some(lua.create_registry_value(f)?)
        } else {
            None
        };

        let node_id = {
            let mut arena = runtime.node_arena.borrow_mut();
            arena.create(Node::Button(ButtonNode {
                text,
                on_press: on_press_key,
                disabled: false,
            }))
        };

        Ok(crate::lua::node::LuaNode::new(node_id))
    })
}

fn create_input_fn(lua: &Lua) -> Result<Function> {
    lua.create_function(|lua, args: Table| -> Result<crate::lua::node::LuaNode> {
        let runtime = get_runtime(lua)?;

        // Get the value signal for two-way binding
        let value_signal = match args.get::<Value>("value")? {
            Value::UserData(ud) => {
                let signal = ud.borrow::<crate::lua::signal::LuaSignal>()?;
                signal.id
            }
            _ => return Err(mlua::Error::RuntimeError(
                "ui.input requires 'value' to be a signal".to_string()
            )),
        };

        let placeholder: String = args.get("placeholder").unwrap_or_default();
        let secure: bool = args.get("secure").unwrap_or(false);

        let on_submit: Option<Function> = args.get("on_submit").ok();
        let on_focus: Option<Function> = args.get("on_focus").ok();
        let on_blur: Option<Function> = args.get("on_blur").ok();

        let node_id = {
            let mut arena = runtime.node_arena.borrow_mut();
            arena.create(Node::Input(InputNode {
                value_signal,
                placeholder,
                secure,
                on_submit: on_submit.map(|f| lua.create_registry_value(f)).transpose()?,
                on_focus: on_focus.map(|f| lua.create_registry_value(f)).transpose()?,
                on_blur: on_blur.map(|f| lua.create_registry_value(f)).transpose()?,
            }))
        };

        // Subscribe node to signal changes
        runtime.subscribe_node(
            crate::node::SignalOrDerived::Signal(value_signal),
            node_id
        );

        Ok(crate::lua::node::LuaNode::new(node_id))
    })
}

fn create_checkbox_fn(lua: &Lua) -> Result<Function> {
    lua.create_function(|lua, args: Table| -> Result<crate::lua::node::LuaNode> {
        let runtime = get_runtime(lua)?;

        let checked_signal = match args.get::<Value>("checked")? {
            Value::UserData(ud) => {
                let signal = ud.borrow::<crate::lua::signal::LuaSignal>()?;
                signal.id
            }
            _ => return Err(mlua::Error::RuntimeError(
                "ui.checkbox requires 'checked' to be a signal".to_string()
            )),
        };

        let on_change: Option<Function> = args.get("on_change").ok();

        let node_id = {
            let mut arena = runtime.node_arena.borrow_mut();
            arena.create(Node::Checkbox(CheckboxNode {
                checked_signal,
                on_change: on_change.map(|f| lua.create_registry_value(f)).transpose()?,
            }))
        };

        runtime.subscribe_node(
            crate::node::SignalOrDerived::Signal(checked_signal),
            node_id
        );

        Ok(crate::lua::node::LuaNode::new(node_id))
    })
}

pub fn register_input_components(lua: &Lua, ui_table: &Table) -> Result<()> {
    ui_table.set("button", create_button_fn(lua)?)?;
    ui_table.set("input", create_input_fn(lua)?)?;
    ui_table.set("checkbox", create_checkbox_fn(lua)?)?;
    Ok(())
}
```

### 5.4 Two-Way Binding Implementation

```rust
// rover-ui/src/event/binding.rs
use mlua::Lua;
use crate::node::{Node, NodeId, NodeArena};
use crate::signal::{SignalId, SignalValue, SignalRuntime};

pub fn handle_input_change(
    lua: &Lua,
    runtime: &SignalRuntime,
    node_id: NodeId,
    new_value: String,
) -> mlua::Result<()> {
    let arena = runtime.node_arena.borrow();

    if let Some(Node::Input(input)) = arena.get(node_id) {
        // Update the bound signal
        runtime.set_signal(input.value_signal, SignalValue::String(new_value.clone()));

        // Note: The signal change will automatically trigger any derived signals
        // and effects that depend on it, including updating the UI
    }

    Ok(())
}

pub fn handle_checkbox_toggle(
    lua: &Lua,
    runtime: &SignalRuntime,
    node_id: NodeId,
) -> mlua::Result<()> {
    let arena = runtime.node_arena.borrow();

    if let Some(Node::Checkbox(checkbox)) = arena.get(node_id) {
        // Get current value and toggle
        let current = runtime.get_signal(checkbox.checked_signal);
        let new_value = match current {
            SignalValue::Bool(b) => !b,
            _ => true,
        };

        runtime.set_signal(checkbox.checked_signal, SignalValue::Bool(new_value));

        // Call on_change callback if present
        if let Some(ref key) = checkbox.on_change {
            drop(arena); // Release borrow before calling Lua
            let callback: mlua::Function = lua.registry_value(key)?;
            callback.call::<_, ()>(new_value)?;
        }
    }

    Ok(())
}

pub fn handle_button_press(
    lua: &Lua,
    runtime: &SignalRuntime,
    node_id: NodeId,
) -> mlua::Result<()> {
    let arena = runtime.node_arena.borrow();

    if let Some(Node::Button(button)) = arena.get(node_id) {
        if button.disabled {
            return Ok(());
        }

        if let Some(ref key) = button.on_press {
            drop(arena); // Release borrow before calling Lua
            let callback: mlua::Function = lua.registry_value(key)?;
            callback.call::<_, ()>(())?;
        }
    }

    Ok(())
}
```

## File Structure

```
rover-ui/
├── src/
│   ├── event/
│   │   ├── mod.rs          # Module exports
│   │   ├── types.rs        # EventType, Event, EventData
│   │   ├── router.rs       # EventRouter, hit testing, focus management
│   │   └── binding.rs      # Two-way binding handlers
│   ├── lua/
│   │   └── input.rs        # ui.button, ui.input, ui.checkbox
│   └── node/
│       └── types.rs        # Add ButtonNode, InputNode, CheckboxNode
```

## Validation Checklist

- [ ] EventRouter correctly routes pointer events to target nodes
- [ ] Focus management: Tab key cycles through focusable nodes
- [ ] ui.button triggers on_press callback when pressed
- [ ] ui.input two-way binding: signal changes update input, input changes update signal
- [ ] ui.checkbox toggles when pressed and fires on_change
- [ ] Hit testing correctly identifies target node under pointer
- [ ] Hover/HoverEnd events fire when pointer enters/leaves nodes
- [ ] Events work consistently on TUI (keyboard) and Web (mouse + keyboard)

## Test Cases

```rust
#[test]
fn test_hit_testing() {
    let mut arena = NodeArena::new();
    let mut layout = LayoutEngine::new();

    let root = arena.create(Node::column());
    let child1 = arena.create(Node::text(TextContent::Static("A".into())));
    let child2 = arena.create(Node::text(TextContent::Static("B".into())));

    // Setup parent-child relationships...

    layout.compute(root, &arena, Size { width: 100, height: 100 });

    let router = EventRouter::new();

    // Child1 should be at top half, child2 at bottom half
    assert_eq!(router.hit_test(50, 25, &arena, &layout, root), Some(child1));
    assert_eq!(router.hit_test(50, 75, &arena, &layout, root), Some(child2));
}

#[test]
fn test_focus_cycle() {
    let mut router = EventRouter::new();

    let events = router.set_focus(Some(NodeId(1)));
    assert!(events.iter().any(|e| e.event_type == EventType::Focus && e.target == NodeId(1)));

    let events = router.set_focus(Some(NodeId(2)));
    assert!(events.iter().any(|e| e.event_type == EventType::Blur && e.target == NodeId(1)));
    assert!(events.iter().any(|e| e.event_type == EventType::Focus && e.target == NodeId(2)));
}
```

## Lua Usage Example

```lua
local email = rover.signal("")
local password = rover.signal("")
local loading = rover.signal(false)
local error_msg = rover.signal(nil)

local is_valid = rover.derive(function()
    return #email.val > 0 and #password.val > 0
end)

local submit = function()
    loading.val = true
    -- HTTP call (future phase)
end

return ui.column {
    mod = mod():gap("md"):pad("lg"):center(),

    ui.text { "Login", mod = mod():size("xl"):weight("bold") },

    ui.input {
        value = email,
        placeholder = "Email",
        mod = mod():fill(),
    },

    ui.input {
        value = password,
        placeholder = "Password",
        secure = true,
        mod = mod():fill(),
    },

    ui.when(error_msg, function()
        return ui.text { error_msg, mod = mod():tint("danger") }
    end),

    ui.button {
        text = "Login",
        on_press = submit,
        mod = mod()
            :intent("primary")
            :fill()
            :when(rover.derive(function() return not is_valid.val end), mod():opacity(0.5))
            :when(loading, mod():opacity(0.5)),
    },
}
```

## TUI-Specific Considerations

For TUI, events map differently:
- **Press**: Enter key or Space on focused element
- **Hover**: Not applicable (no mouse in basic TUI)
- **Focus**: Tab/Shift+Tab navigation
- **TextInput**: Character keys when input is focused

```rust
// TUI event mapping
fn map_tui_event(event: crossterm::event::Event, router: &EventRouter) -> Option<Event> {
    match event {
        Event::Key(KeyEvent { code: KeyCode::Enter, .. }) => {
            if let Some(focused) = router.focused() {
                Some(Event {
                    event_type: EventType::Press,
                    target: focused,
                    data: EventData::None,
                    propagation_stopped: false,
                })
            } else {
                None
            }
        }
        Event::Key(KeyEvent { code: KeyCode::Tab, modifiers, .. }) => {
            // Handle focus cycling
            // ...
        }
        Event::Key(KeyEvent { code: KeyCode::Char(c), .. }) => {
            if let Some(focused) = router.focused() {
                Some(Event {
                    event_type: EventType::TextInput,
                    target: focused,
                    data: EventData::Text { value: c.to_string() },
                    propagation_stopped: false,
                })
            } else {
                None
            }
        }
        _ => None,
    }
}
```
