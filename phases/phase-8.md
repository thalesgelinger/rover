# Fase 8: Animation Modifiers

**Status:** 🔲 Não Iniciado  
**Duration:** 3-4 semanas  
**Dependencies:** Fase 6, Fase 7

## Objetivo

Implementar sistema de animações baseado em modifiers + timing.

## Entregas

### 8.1 Timing Modifiers

```rust
pub struct TimingConfig {
    duration: Duration,
    curve: EasingCurve,
    delay: Duration,
    loop_mode: LoopMode,
}

pub enum EasingCurve {
    Linear,
    Ease,
    EaseIn,
    EaseOut,
    Spring { damping: f32, stiffness: f32 },
    Bounce,
}

pub enum LoopMode {
    None,
    Forever,
    Count(u32),
}
```

### 8.2 Animated Properties

```rust
pub struct AnimatedProperty {
    property: StyleProperty,
    from: StyleValue,
    to: StyleValue,
    timing: TimingConfig,
    progress: f32,
}

impl AnimatedProperty {
    fn tick(&mut self, dt: Duration) -> StyleValue {
        self.progress += dt.as_secs_f32() / self.timing.duration.as_secs_f32();
        let t = self.timing.curve.apply(self.progress.min(1.0));
        self.from.interpolate(&self.to, t)
    }
}
```

### 8.3 Animation Loop

```rust
impl Rover {
    fn animation_tick(&mut self, dt: Duration) {
        for anim in &mut self.active_animations {
            let new_value = anim.tick(dt);
            self.render_queue.push(RenderCommand::UpdateStyle {
                node: anim.node,
                property: anim.property,
                value: new_value,
            });
            
            if anim.is_complete() {
                self.completed_animations.push(anim.id);
            }
        }
    }
}
```

### 8.4 Platform Animation Integration

```rust
// iOS: usar CAAnimation quando possível
// Android: usar Animator
// Web: usar CSS transitions/Web Animations API
// TUI: tick-based manual

impl IosRenderer {
    fn apply_animated(&mut self, cmd: RenderCommand, timing: TimingConfig) {
        match cmd {
            RenderCommand::UpdateStyle { node, property: StyleProperty::Opacity, value } => {
                let view = self.node_refs[&node];
                UIView.animate(withDuration: timing.duration) {
                    view.alpha = value.as_f64()
                }
            }
        }
    }
}
```

### 8.5 Lua API

```lua
mod:on("mount",
    mod
        :opacity("full")
        :move("y", "none")
        :duration("normal")
        :curve("spring")
)

mod:on("hover",
    mod:elevate("md"):shadow("soft"):duration("fast")
)

mod:when(loading,
    mod:rotate(360):duration("long"):loop()
)
```

## Validação Fase 8

```lua
function AnimatedCard()
    local expanded = signal(false)
    
    return ui.column {
        on_press = function() expanded.val = not expanded.val end,
        mod = mod
            :surface("raised")
            :radius("md")
            :pad("md")
            :on("mount", 
                mod:opacity("full"):move("y", "none"):duration("normal"):curve("spring"))
            :on("hover",
                mod:elevate("md"):duration("fast"))
            :when(expanded,
                mod:height("lg"):duration("normal"):curve("spring")),
        
        ui.text { "Click to expand" },
        
        ui.when(expanded, function()
            return ui.text { 
                "Expanded content!",
                mod = mod:on("mount", mod:opacity("full"):duration("fast"))
            }
        end)
    end
end
```

*Validar:*

- Animação de entrada suave
- Hover feedback
- Expand/collapse animado
- 60fps constante durante animações
