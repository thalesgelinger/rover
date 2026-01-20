# Fase 5: Events e Interatividade

**Status:** 🔲 Não Iniciado  
**Duration:** 1-2 semanas  
**Dependencies:** Fase 4

## Objetivo

Implementar sistema de eventos consistente entre plataformas.

## Entregas

### 5.1 Event Types

```rust
pub enum EventType {
    // Universal
    Press,
    LongPress,
    
    // Pointer (desktop/web)
    Hover,
    HoverEnd,
    
    // Focus (accessibility)
    Focus,
    Blur,
    
    // Lifecycle
    Mount,
    Unmount,
    Visible,  // entrou no viewport
}
```

### 5.2 Event → Modifier Trigger

```rust
pub struct EventModifier {
    event: EventType,
    modifiers: ModifierChain,
}

// on_press, on_hover, etc viram bindings internos
// que setam um signal interno e disparam :when
```

### 5.3 Input Components

```lua
-- Button
ui.button {
    text = "Click me",
    icon = "plus",  -- opcional
    on_press = function() end,
    mod = mod:intent("primary"),
}

-- Input
ui.input {
    value = email,  -- signal, two-way binding
    placeholder = "Email",
    secure = false,  -- password mode
    on_submit = function() end,
    on_focus = function() end,
    on_blur = function() end,
}

-- Checkbox
ui.checkbox {
    checked = remember_me,  -- signal
    on_change = function(new_value) end,
}

-- Switch
ui.switch {
    value = dark_mode,
    on_change = function(new_value) end,
}
```

### 5.4 Two-Way Binding

```rust
// ui.input { value = signal }
// Internamente:
// - Lê signal pra valor inicial
// - Em onChange nativo, seta signal.val = novo_valor
// - Signal change propaga de volta pro input (caso setado externamente)
```

## Validação Fase 5

```lua
function LoginForm()
    local email = signal("")
    local password = signal("")
    local loading = signal(false)
    local error = signal(nil)
    
    local is_valid = derive(function()
        return #email.val > 0 and #password.val > 0
    end)
    
    local submit = function()
        loading.val = true
        -- HTTP call aqui (fase futura)
    end
    
    return ui.column {
        mod = mod:gap("md"):pad("lg"):width("sm"):center(),
        
        ui.text { "Login", mod = mod:size("xl"):weight("bold") },
        
        ui.input {
            value = email,
            placeholder = "Email",
            mod = mod:fill(),
        },
        
        ui.input {
            value = password,
            placeholder = "Password",
            secure = true,
            mod = mod:fill(),
        },
        
        ui.when(error, function()
            return ui.text { error, mod = mod:tint("danger") }
        end),
        
        ui.button {
            text = "Login",
            on_press = submit,
            mod = mod
                :intent("primary")
                :fill()
                :when(not is_valid, mod:opacity("muted"):disabled())
                :when(loading, mod:opacity("muted"):disabled()),
        },
    }
end
```
