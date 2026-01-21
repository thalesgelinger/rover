# Fase 4: Styling Modifiers

**Status:** Not Started
**Duration:** 2-3 semanas
**Dependencies:** Fase 3

## Agent Context

### Prerequisites
- Phase 3 must be complete (Web renderer working)
- Both TUI and Web should be able to apply styling
- Modifiers work through RenderCommands, not direct DOM/terminal manipulation

### Architecture Overview

```
┌────────────────────────────────────────────────────────────────┐
│                      Lua API                                    │
│  ui.text { "Hello" }:pad("md"):tint("primary")                 │
└────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌────────────────────────────────────────────────────────────────┐
│                    ModifierChain                                │
│  [ Modifier::Pad(Md), Modifier::Tint(Primary) ]                │
└────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌────────────────────────────────────────────────────────────────┐
│                    TokenResolver                                │
│  TUI: Pad(Md) → 2 chars    Web: Pad(Md) → 16px                │
└────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌────────────────────────────────────────────────────────────────┐
│                    RenderCommand                                │
│  UpdateStyle { node, property: Padding, value: "16px" }        │
└────────────────────────────────────────────────────────────────┘
```

### Key Concept: Semantic Tokens

Modifiers use **semantic tokens** (Xs, Sm, Md, Lg, Xl) instead of raw values.
Each platform's TokenResolver maps tokens to appropriate platform values.

## Objetivo

Implementar sistema de modifiers semânticos que funcionam em TUI e Web.

## Entregas

### 4.1 Modifier Chain System

```rust
// rover-ui/src/modifier/mod.rs
pub mod chain;
pub mod tokens;
pub mod resolver;

// rover-ui/src/modifier/tokens.rs
use smartstring::{LazyCompact, SmartString};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpacingToken { None, Xs, Sm, Md, Lg, Xl }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorToken { Default, Muted, Primary, Secondary, Danger, Success, Warning }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceToken { Flat, Raised, Filled, Ghost }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadiusToken { None, Sm, Md, Lg, Full }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignment { Start, Center, End, Stretch }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Distribution { Start, Center, End, SpaceBetween, SpaceAround }

impl SpacingToken {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "none" | "0" => Self::None,
            "xs" => Self::Xs,
            "sm" => Self::Sm,
            "md" => Self::Md,
            "lg" => Self::Lg,
            "xl" => Self::Xl,
            _ => Self::Md, // default
        }
    }
}
```

```rust
// rover-ui/src/modifier/chain.rs
use smallvec::SmallVec;
use crate::signal::SignalId;
use super::tokens::*;

#[derive(Debug, Clone)]
pub struct ModifierChain {
    pub modifiers: SmallVec<[Modifier; 8]>,
}

#[derive(Debug, Clone)]
pub enum Modifier {
    // Layout
    Fill,
    Wrap,
    Gap(SpacingToken),
    Pad(SpacingToken),
    PadX(SpacingToken),
    PadY(SpacingToken),
    Center,
    CrossAlign(Alignment),
    MainAlign(Distribution),
    Width(SizeValue),
    Height(SizeValue),
    MinWidth(SizeValue),
    MaxWidth(SizeValue),

    // Visual
    Tint(ColorToken),
    Background(ColorToken),
    Surface(SurfaceToken),
    Radius(RadiusToken),
    Opacity(f32),
    Border(ColorToken, SpacingToken),

    // Typography
    Size(TextSizeToken),
    Weight(TextWeightToken),
    Italic,
    Underline,

    // Conditional
    When(SignalId, Box<ModifierChain>),
    On(EventType, Box<ModifierChain>),
}

#[derive(Debug, Clone)]
pub enum SizeValue {
    Token(SpacingToken),
    Pixels(f32),
    Percent(f32),
    Auto,
}

#[derive(Debug, Clone, Copy)]
pub enum TextSizeToken { Xs, Sm, Md, Lg, Xl, Xxl }

#[derive(Debug, Clone, Copy)]
pub enum TextWeightToken { Normal, Medium, Semibold, Bold }

#[derive(Debug, Clone, Copy)]
pub enum EventType { Hover, Press, Focus, Active }

impl ModifierChain {
    pub fn new() -> Self {
        Self { modifiers: SmallVec::new() }
    }

    pub fn push(&mut self, modifier: Modifier) {
        self.modifiers.push(modifier);
    }

    pub fn extend(&mut self, other: &ModifierChain) {
        self.modifiers.extend(other.modifiers.iter().cloned());
    }
}

impl Default for ModifierChain {
    fn default() -> Self {
        Self::new()
    }
}
```

### 4.2 Lua Chainable API

```rust
// rover-ui/src/lua/modifier.rs
use mlua::{Lua, Result, UserData, UserDataMethods};
use crate::modifier::{ModifierChain, Modifier, SpacingToken, ColorToken, SurfaceToken};

#[derive(Clone)]
pub struct LuaMod {
    pub chain: ModifierChain,
}

impl LuaMod {
    pub fn new() -> Self {
        Self { chain: ModifierChain::new() }
    }
}

impl UserData for LuaMod {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // Layout modifiers
        methods.add_method("fill", |_, this, ()| {
            let mut chain = this.chain.clone();
            chain.push(Modifier::Fill);
            Ok(LuaMod { chain })
        });

        methods.add_method("center", |_, this, ()| {
            let mut chain = this.chain.clone();
            chain.push(Modifier::Center);
            Ok(LuaMod { chain })
        });

        methods.add_method("gap", |_, this, token: String| {
            let mut chain = this.chain.clone();
            chain.push(Modifier::Gap(SpacingToken::from_str(&token)));
            Ok(LuaMod { chain })
        });

        methods.add_method("pad", |_, this, token: String| {
            let mut chain = this.chain.clone();
            chain.push(Modifier::Pad(SpacingToken::from_str(&token)));
            Ok(LuaMod { chain })
        });

        methods.add_method("padX", |_, this, token: String| {
            let mut chain = this.chain.clone();
            chain.push(Modifier::PadX(SpacingToken::from_str(&token)));
            Ok(LuaMod { chain })
        });

        methods.add_method("padY", |_, this, token: String| {
            let mut chain = this.chain.clone();
            chain.push(Modifier::PadY(SpacingToken::from_str(&token)));
            Ok(LuaMod { chain })
        });

        // Visual modifiers
        methods.add_method("tint", |_, this, token: String| {
            let mut chain = this.chain.clone();
            chain.push(Modifier::Tint(ColorToken::from_str(&token)));
            Ok(LuaMod { chain })
        });

        methods.add_method("surface", |_, this, token: String| {
            let mut chain = this.chain.clone();
            chain.push(Modifier::Surface(SurfaceToken::from_str(&token)));
            Ok(LuaMod { chain })
        });

        methods.add_method("radius", |_, this, token: String| {
            let mut chain = this.chain.clone();
            chain.push(Modifier::Radius(RadiusToken::from_str(&token)));
            Ok(LuaMod { chain })
        });

        // Typography
        methods.add_method("size", |_, this, token: String| {
            let mut chain = this.chain.clone();
            chain.push(Modifier::Size(TextSizeToken::from_str(&token)));
            Ok(LuaMod { chain })
        });

        methods.add_method("weight", |_, this, token: String| {
            let mut chain = this.chain.clone();
            chain.push(Modifier::Weight(TextWeightToken::from_str(&token)));
            Ok(LuaMod { chain })
        });

        // Conditional modifier
        methods.add_method("when", |lua, this, (condition, inner_mod): (mlua::Value, LuaMod)| {
            let runtime = crate::lua::helpers::get_runtime(lua)?;

            let signal_id = match condition {
                mlua::Value::UserData(ud) => {
                    let signal = ud.borrow::<crate::lua::signal::LuaSignal>()?;
                    signal.id
                }
                _ => return Err(mlua::Error::RuntimeError(
                    "when() requires a signal as first argument".to_string()
                )),
            };

            let mut chain = this.chain.clone();
            chain.push(Modifier::When(signal_id, Box::new(inner_mod.chain)));
            Ok(LuaMod { chain })
        });
    }
}

pub fn register_mod_constructor(lua: &Lua, rover_table: &mlua::Table) -> Result<()> {
    // Create mod constructor that returns a new LuaMod
    let mod_constructor = lua.create_function(|_, ()| {
        Ok(LuaMod::new())
    })?;

    // Also expose it as a global `mod` for convenience
    rover_table.set("mod", mod_constructor.clone())?;

    // Create a starting LuaMod for chainable API: rover.ui.mod:fill()...
    let ui_table: mlua::Table = rover_table.get("ui")?;
    ui_table.set("mod", LuaMod::new())?;

    Ok(())
}
```

### 4.3 Token → Platform Value Mapping

```rust
// rover-ui/src/modifier/resolver.rs
use super::tokens::*;

#[derive(Debug, Clone, Copy)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub fn to_css(&self) -> String {
        if self.a == 255 {
            format!("rgb({}, {}, {})", self.r, self.g, self.b)
        } else {
            format!("rgba({}, {}, {}, {})", self.r, self.g, self.b, self.a as f32 / 255.0)
        }
    }

    pub fn to_ansi(&self) -> String {
        // Convert to nearest ANSI 256 color
        format!("\x1b[38;2;{};{};{}m", self.r, self.g, self.b)
    }
}

pub trait TokenResolver: Send + Sync {
    fn resolve_spacing(&self, token: SpacingToken) -> f32;
    fn resolve_color(&self, token: ColorToken) -> Color;
    fn resolve_radius(&self, token: RadiusToken) -> f32;
    fn resolve_text_size(&self, token: TextSizeToken) -> f32;
}

pub struct TuiTokenResolver;

impl TokenResolver for TuiTokenResolver {
    fn resolve_spacing(&self, token: SpacingToken) -> f32 {
        match token {
            SpacingToken::None => 0.0,
            SpacingToken::Xs => 1.0,   // 1 char
            SpacingToken::Sm => 1.0,
            SpacingToken::Md => 2.0,   // 2 chars
            SpacingToken::Lg => 3.0,
            SpacingToken::Xl => 4.0,
        }
    }

    fn resolve_color(&self, token: ColorToken) -> Color {
        // TUI uses ANSI colors
        match token {
            ColorToken::Default => Color::rgb(255, 255, 255),
            ColorToken::Muted => Color::rgb(128, 128, 128),
            ColorToken::Primary => Color::rgb(59, 130, 246),   // blue
            ColorToken::Secondary => Color::rgb(139, 92, 246), // purple
            ColorToken::Danger => Color::rgb(239, 68, 68),     // red
            ColorToken::Success => Color::rgb(34, 197, 94),    // green
            ColorToken::Warning => Color::rgb(234, 179, 8),    // yellow
        }
    }

    fn resolve_radius(&self, _token: RadiusToken) -> f32 {
        0.0 // TUI doesn't support radius
    }

    fn resolve_text_size(&self, _token: TextSizeToken) -> f32 {
        1.0 // TUI has fixed character size
    }
}

pub struct WebTokenResolver;

impl TokenResolver for WebTokenResolver {
    fn resolve_spacing(&self, token: SpacingToken) -> f32 {
        match token {
            SpacingToken::None => 0.0,
            SpacingToken::Xs => 4.0,   // 4px
            SpacingToken::Sm => 8.0,
            SpacingToken::Md => 16.0,
            SpacingToken::Lg => 24.0,
            SpacingToken::Xl => 32.0,
        }
    }

    fn resolve_color(&self, token: ColorToken) -> Color {
        match token {
            ColorToken::Default => Color::rgb(17, 24, 39),     // gray-900
            ColorToken::Muted => Color::rgb(107, 114, 128),    // gray-500
            ColorToken::Primary => Color::rgb(59, 130, 246),   // blue-500
            ColorToken::Secondary => Color::rgb(139, 92, 246), // violet-500
            ColorToken::Danger => Color::rgb(239, 68, 68),     // red-500
            ColorToken::Success => Color::rgb(34, 197, 94),    // green-500
            ColorToken::Warning => Color::rgb(234, 179, 8),    // yellow-500
        }
    }

    fn resolve_radius(&self, token: RadiusToken) -> f32 {
        match token {
            RadiusToken::None => 0.0,
            RadiusToken::Sm => 4.0,
            RadiusToken::Md => 8.0,
            RadiusToken::Lg => 16.0,
            RadiusToken::Full => 9999.0,
        }
    }

    fn resolve_text_size(&self, token: TextSizeToken) -> f32 {
        match token {
            TextSizeToken::Xs => 12.0,
            TextSizeToken::Sm => 14.0,
            TextSizeToken::Md => 16.0,
            TextSizeToken::Lg => 18.0,
            TextSizeToken::Xl => 24.0,
            TextSizeToken::Xxl => 32.0,
        }
    }
}
```

### 4.4 Modifier → RenderCommand

```rust
// rover-ui/src/modifier/commands.rs
use crate::node::{NodeId, RenderCommand};
use super::{chain::{Modifier, ModifierChain}, resolver::TokenResolver};

#[derive(Debug, Clone)]
pub enum StyleProperty {
    Gap,
    Padding,
    PaddingX,
    PaddingY,
    Color,
    BackgroundColor,
    BorderRadius,
    FontSize,
    FontWeight,
    Opacity,
    Display,
    FlexDirection,
    JustifyContent,
    AlignItems,
}

#[derive(Debug, Clone)]
pub enum StyleValue {
    Float(f32),
    Color(super::resolver::Color),
    String(String),
}

impl ModifierChain {
    pub fn to_style_commands(
        &self,
        node: NodeId,
        resolver: &dyn TokenResolver,
    ) -> Vec<RenderCommand> {
        let mut commands = vec![];

        for modifier in &self.modifiers {
            match modifier {
                Modifier::Gap(token) => {
                    let value = resolver.resolve_spacing(*token);
                    commands.push(RenderCommand::UpdateStyle {
                        node,
                        property: StyleProperty::Gap,
                        value: StyleValue::Float(value),
                    });
                }
                Modifier::Pad(token) => {
                    let value = resolver.resolve_spacing(*token);
                    commands.push(RenderCommand::UpdateStyle {
                        node,
                        property: StyleProperty::Padding,
                        value: StyleValue::Float(value),
                    });
                }
                Modifier::Tint(token) => {
                    let color = resolver.resolve_color(*token);
                    commands.push(RenderCommand::UpdateStyle {
                        node,
                        property: StyleProperty::Color,
                        value: StyleValue::Color(color),
                    });
                }
                Modifier::Radius(token) => {
                    let value = resolver.resolve_radius(*token);
                    commands.push(RenderCommand::UpdateStyle {
                        node,
                        property: StyleProperty::BorderRadius,
                        value: StyleValue::Float(value),
                    });
                }
                Modifier::Size(token) => {
                    let value = resolver.resolve_text_size(*token);
                    commands.push(RenderCommand::UpdateStyle {
                        node,
                        property: StyleProperty::FontSize,
                        value: StyleValue::Float(value),
                    });
                }
                Modifier::When(signal_id, inner_chain) => {
                    // Conditional modifiers are handled separately
                    // during signal subscription setup
                }
                _ => {
                    // Handle other modifiers...
                }
            }
        }

        commands
    }

    pub fn to_reset_commands(&self, node: NodeId) -> Vec<RenderCommand> {
        // Generate commands to reset styles to defaults
        // Used when :when condition becomes false
        let mut commands = vec![];

        for modifier in &self.modifiers {
            match modifier {
                Modifier::Gap(_) => {
                    commands.push(RenderCommand::UpdateStyle {
                        node,
                        property: StyleProperty::Gap,
                        value: StyleValue::Float(0.0),
                    });
                }
                // ... other reset commands
                _ => {}
            }
        }

        commands
    }
}
```

### 4.5 Conditional Modifiers (`:when`)

```rust
// rover-ui/src/modifier/conditional.rs
use crate::node::NodeId;
use crate::signal::SignalId;
use crate::renderer::Renderer;
use super::{chain::ModifierChain, resolver::TokenResolver};

pub struct ConditionalModifier {
    pub condition: SignalId,
    pub modifiers: ModifierChain,
    pub active: bool,
    pub node: NodeId,
}

impl ConditionalModifier {
    pub fn new(condition: SignalId, modifiers: ModifierChain, node: NodeId) -> Self {
        Self {
            condition,
            modifiers,
            active: false,
            node,
        }
    }

    pub fn on_signal_change(
        &mut self,
        new_value: bool,
        renderer: &mut dyn Renderer,
        resolver: &dyn TokenResolver,
    ) {
        if new_value && !self.active {
            // Apply modifiers
            let commands = self.modifiers.to_style_commands(self.node, resolver);
            for cmd in commands {
                renderer.apply_style(&cmd);
            }
            self.active = true;
        } else if !new_value && self.active {
            // Reset modifiers
            let commands = self.modifiers.to_reset_commands(self.node);
            for cmd in commands {
                renderer.apply_style(&cmd);
            }
            self.active = false;
        }
    }
}
```

## File Structure

```
rover-ui/
├── src/
│   ├── modifier/
│   │   ├── mod.rs          # Module exports
│   │   ├── chain.rs        # ModifierChain, Modifier enum
│   │   ├── tokens.rs       # SpacingToken, ColorToken, etc.
│   │   ├── resolver.rs     # TokenResolver trait, TUI/Web impls
│   │   ├── commands.rs     # to_style_commands(), to_reset_commands()
│   │   └── conditional.rs  # ConditionalModifier
│   ├── lua/
│   │   └── modifier.rs     # LuaMod userdata
│   └── node/
│       └── commands.rs     # Add UpdateStyle variant
```

## Validation Checklist

- [ ] ModifierChain can be constructed and cloned
- [ ] LuaMod chainable API works: `mod:fill():center():gap("md")`
- [ ] TuiTokenResolver maps tokens to character-based values
- [ ] WebTokenResolver maps tokens to pixel-based values
- [ ] UpdateStyle commands are generated correctly
- [ ] Conditional modifiers respond to signal changes
- [ ] Same Lua code produces appropriate styling on both TUI and Web

## Test Cases

```rust
#[test]
fn test_modifier_chain() {
    let mut chain = ModifierChain::new();
    chain.push(Modifier::Pad(SpacingToken::Md));
    chain.push(Modifier::Tint(ColorToken::Primary));
    assert_eq!(chain.modifiers.len(), 2);
}

#[test]
fn test_tui_resolver() {
    let resolver = TuiTokenResolver;
    assert_eq!(resolver.resolve_spacing(SpacingToken::Md), 2.0);
    assert_eq!(resolver.resolve_spacing(SpacingToken::Xl), 4.0);
}

#[test]
fn test_web_resolver() {
    let resolver = WebTokenResolver;
    assert_eq!(resolver.resolve_spacing(SpacingToken::Md), 16.0);
    assert_eq!(resolver.resolve_spacing(SpacingToken::Xl), 32.0);
}
```

## Lua Usage Example

```lua
local mod = rover.mod

-- Simple styling
ui.column {
    mod = mod():pad("md"):gap("sm"):surface("raised"):radius("md"),

    ui.text {
        "Card Title",
        mod = mod():size("lg"):weight("bold")
    },

    ui.text {
        "Card content",
        mod = mod():tint("muted")
    },
}

-- Conditional styling
local loading = rover.signal(false)

ui.button {
    text = "Submit",
    mod = mod()
        :pad("md")
        :surface("filled")
        :when(loading, mod():opacity(0.5))
}
```
