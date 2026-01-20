# Fase 4: Styling Modifiers

**Status:** 🔲 Não Iniciado  
**Duration:** 2-3 semanas  
**Dependencies:** Fase 3

## Objetivo

Implementar sistema de modifiers semânticos que funcionam em TUI e Web.

## Entregas

### 4.1 Modifier Chain System

```rust
pub struct ModifierChain {
    modifiers: SmallVec<[Modifier; 8]>,
}

pub enum Modifier {
    // Layout
    Fill,
    Wrap,
    Gap(SpacingToken),
    Pad(SpacingToken),
    Center,
    CrossAlign(Alignment),
    MainAlign(Distribution),
    
    // Visual
    Tint(ColorToken),
    Surface(SurfaceToken),
    Radius(RadiusToken),
    
    // Conditional
    When(SignalId, Box<ModifierChain>),
    On(EventType, Box<ModifierChain>),
}

pub enum SpacingToken { None, Xs, Sm, Md, Lg, Xl }
pub enum ColorToken { Default, Muted, Primary, Danger, Success, Warning }
pub enum SurfaceToken { Flat, Raised, Filled, Ghost }
```

### 4.2 Lua Chainable API

```lua
local mod = rover.ui.mod

-- Chainable
mod:fill():center():gap("md"):surface("raised")

-- Condicional
mod:when(loading, mod:opacity("muted"))
mod:on("hover", mod:elevate("sm"))
```

```rust
// Implementação via metatable
impl LuaMod {
    fn fill(this: &LuaMod) -> LuaMod {
        let mut chain = this.chain.clone();
        chain.push(Modifier::Fill);
        LuaMod { chain }
    }
    
    fn gap(this: &LuaMod, token: String) -> LuaMod {
        let mut chain = this.chain.clone();
        chain.push(Modifier::Gap(SpacingToken::from_str(&token)));
        LuaMod { chain }
    }
}
```

### 4.3 Token → Platform Value Mapping

```rust
pub trait TokenResolver {
    fn resolve_spacing(&self, token: SpacingToken) -> f32;
    fn resolve_color(&self, token: ColorToken) -> Color;
    fn resolve_radius(&self, token: RadiusToken) -> f32;
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
    // TUI não tem cor real, pode usar ANSI codes
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
}
```

### 4.4 Modifier → RenderCommand

```rust
impl ModifierChain {
    fn to_style_commands(&self, node: NodeId, resolver: &dyn TokenResolver) -> Vec<RenderCommand> {
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
                Modifier::Surface(token) => {
                    let (bg, shadow) = resolver.resolve_surface(*token);
                    commands.push(RenderCommand::UpdateStyle {
                        node,
                        property: StyleProperty::Background,
                        value: StyleValue::Color(bg),
                    });
                }
                Modifier::When(signal, inner_chain) => {
                    // Registra subscription
                    // Quando signal muda, aplica/remove inner_chain
                }
                // ...
            }
        }
        
        commands
    }
}
```

### 4.5 Conditional Modifiers (`:when`)

```rust
pub struct ConditionalModifier {
    condition: SignalId,
    modifiers: ModifierChain,
    active: bool,
}

impl ConditionalModifier {
    fn on_signal_change(&mut self, new_value: bool, node: NodeId, renderer: &mut dyn Renderer) {
        if new_value && !self.active {
            // Aplica modifiers
            for cmd in self.modifiers.to_style_commands(node) {
                renderer.apply(cmd);
            }
            self.active = true;
        } else if !new_value && self.active {
            // Remove/reverte modifiers
            for cmd in self.modifiers.to_reset_commands(node) {
                renderer.apply(cmd);
            }
            self.active = false;
        }
    }
}
```

## Validação Fase 4

```lua
function StyledCard()
    local loading = signal(false)
    
    return ui.column {
        mod = mod
            :pad("md")
            :gap("sm")
            :surface("raised")
            :radius("md")
            :on("hover", mod:elevate("md"):shadow("soft"))
            :when(loading, mod:elevate("md"):shadow("soft")),
        
        ui.text { "Card Title", mod = mod:size("lg"):weight("bold") },
        ui.text { "Card content", mod = mod:tint("muted") },
    }
end
```

- **TUI:** Card aparece com padding de espaços, sem cor (ou ANSI colors)
- **Web:** Card aparece com padding em px, cores reais, shadow no hover
