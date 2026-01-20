# Fase 9: HTTP e Async

**Status:** 🔲 Não Iniciado  
**Duration:** 2 semanas  
**Dependencies:** Fase 5

## Objetivo

Implementar I/O não-bloqueante com sintaxe síncrona.

## Entregas

### 9.1 Async Runtime Integration

```rust
// Usar tokio ou async-std internamente
// Expor API síncrona pro Lua via coroutines

pub fn http_get(lua: &Lua, url: String) -> LuaResult<LuaValue> {
    // Suspende coroutine Lua
    // Faz request async
    // Resume coroutine com resultado
}
```

### 9.2 Lua API

```lua
-- Parece síncrono, mas não bloqueia UI
local data = rover.http.get("/users")

-- Com error handling
local ok, result = pcall(function()
    return rover.http.post("/login", { email = email.val, password = password.val })
end)

if ok then
    -- success
else
    error.val = result
end
```

### 9.3 Effect + HTTP

```lua
rover.effect(function()
    local filter = filter.val  -- subscribe
    loading.val = true
    
    local ok, data = pcall(function()
        return rover.http.get("/items?filter=" .. filter)
    end)
    
    if ok then
        items.val = data
    else
        error.val = data
    end
    
    loading.val = false
end)
```
