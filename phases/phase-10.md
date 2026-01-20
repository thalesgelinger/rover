# Fase 10: Polish e Otimizações

**Status:** 🔲 Não Iniciado  
**Duration:** Contínua  
**Dependencies:** Nenhuma

## Objetivo

Otimizar performance, reduzir bundle size, melhorar DX.

## Entregas

### 10.1 List Virtualization

```lua
-- ui.each com virtualização automática pra listas grandes
ui.each(items, function(item)
    return ui.row { key = item.id, ... }
end, { virtualize = true })
```

### 10.2 Batch Updates

```rust
// Agrupa múltiplos signal changes em um frame
rover.batch(function()
    a.val = 1
    b.val = 2
    c.val = 3
end)  -- Só uma render pass
```

### 10.3 DevTools

- Inspector de signals
- Visualização de subscriber graph
- Performance profiler
- Hot reload

### 10.4 Error Boundaries

```lua
ui.catch(
    function()
        return RiskyComponent {}
    end,
    function(err)
        return ui.text { "Error: " .. err, mod = mod:tint("danger") }
    end
)
```

---

# Cronograma Estimado

| Fase            | Duração    | Dependências |
|----------------|-----------|------------|
| 1. Signals      | 2-3 semanas| -           |
| 2. UI Core + TUI| 2-3 semanas| Fase 1      |
| 3. Web Renderer | 2 semanas  | Fase 2      |
| 4. Styling      | 2-3 semanas| Fase 3      |
| 5. Events       | 1-2 semanas| Fase 4      |
| 6. iOS          | 3-4 semanas| Fase 5      |
| 7. Android      | 2-3 semanas| Fase 5      |
| 8. Animations   | 3-4 semanas| Fase 6, 7   |
| 9. HTTP/Async   | 2 semanas  | Fase 5      |
| 10. Polish      | Contínua   | -           |

**Total estimado: 5-7 meses** para MVP funcional em todas plataformas.

---

# Métricas de Sucesso

## Performance Targets

| Métrica        | Target | React Native| Flutter  |
|---------------|-------|------------|---------|
| Update simples | < 1ms  | 3-10ms     | 1-3ms   |
| Cold start     | < 300ms| 800-2000ms | 300-600ms|
| Memory baseline| < 15MB | 40-80MB    | 30-50MB |
| Bundle size    | < 3MB  | 7-15MB     | 5-10MB  |
| 60fps scroll   | Sim    | Difícil    | Sim     |

## Validation Checklist

- [ ] Mesmo código Lua roda em TUI, Web, iOS, Android
- [ ] Update granular comprovado (não re-renderiza siblings)
- [ ] Zero allocation em steady-state updates
- [ ] 60fps em animações
- [ ] Memory não cresce em uso prolongado

---

# Riscos e Mitigações

| Risco                         | Probabilidade| Impacto| Mitigação                         |
|------------------------------|-------------|-------|----------------------------------|
| Layout engine complexo        | Alta        | Alto  | Usar Taffy (flexbox Rust)        |
| JNI overhead Android          | Média       | Médio | Batch commands, minimize crossings|
| Lua GC pauses                 | Média       | Médio | Arena allocation, object pooling  |
| iOS App Store rejection       | Baixa       | Alto  | Seguir guidelines, não usar JIT   |
| Performance não atinge targets| Média       | Alto  | Profile early, optimize hot paths |

---

# Notas: Future Multi-Threading (Lynx-Style)

## Objetivo

Implementar arquitetura multi-thread para mobile nativo (iOS/Android), separando UI thread de compute thread.

## Arquitetura Lynx-Style

```
┌─────────────────────────────────────────────────────────┐
│                 Main Thread (UI)                        │
│  - Event handling                                       │
│  - Renderer (native views)                              │
│  - Layout computation                                   │
└──────────────┬──────────────────────────────────────────┘
               │ Message Channel
               │ (Command Queue)
               ▼
┌─────────────────────────────────────────────────────────┐
│              Worker Thread (Compute)                    │
│  - Lua VM                                               │
│  - Signal Runtime (Arc<RwLock<SignalRuntime>>)          │
│  - Effect execution                                     │
│  - Derived computation                                  │
└─────────────────────────────────────────────────────────┘
```

## Signal Runtime Thread-Safe Design

```rust
// Phase 1: Single-threaded (Lua app_data)
pub struct SignalRuntime {
    arena: SignalArena,
    graph: SubscriberGraph,
    // ...
}

// Future: Multi-threaded (Arc + RwLock)
pub struct SharedSignalRuntime {
    inner: Arc<RwLock<SignalRuntime>>,
}

impl SharedSignalRuntime {
    pub fn read_signal(&self, id: SignalId) -> SignalValue {
        self.inner.read().unwrap().get_signal(id).clone()
    }
    
    pub fn write_signal(&self, id: SignalId, value: SignalValue) {
        let mut rt = self.inner.write().unwrap();
        rt.set_signal(id, value);
        // Flush render commands to main thread via channel
    }
}
```

## Message Passing

```rust
pub enum UICommand {
    RenderCommand(RenderCommand),
    RunEffect(EffectId),
    Batch(Vec<UICommand>),
}

pub enum WorkerMessage {
    SignalChanged(SignalId, SignalValue),
    UserEvent(EventType, NodeId),
    Shutdown,
}

// Main thread → Worker
let (worker_tx, worker_rx) = channel::<WorkerMessage>();

// Worker → Main thread  
let (main_tx, main_rx) = channel::<UICommand>();
```

## Thread-Safe Primitives

**Primitives** (`Bool`, `Int`, `Float`, `String`):
- Already `Send + Sync`
- Can be cloned across thread boundary

**Tables**:
- Option A: Keep thread-local, serialize for cross-thread
- Option B: Use Arc<RwLock<LuaTable>> with careful lifetime management
- **Recommended**: Thread-local signals, message passing for updates

## Effect Execution

Effects always run on **worker thread**:
```rust
// Worker thread
fn run_effect(lua: &Lua, effect_id: EffectId) -> Result<()> {
    let mut rt = get_runtime(lua);
    let effect = &rt.effects[effect_id.0 as usize];
    
    // Run cleanup
    if let Some(cleanup_key) = &effect.cleanup {
        let cleanup: Function = lua.registry_value(cleanup_key)?;
        cleanup.call::<_, ()>(())?;
    }
    
    // Run callback
    let callback: Function = lua.registry_value(&effect.callback)?;
    let result = callback.call::<_, Value>(())?;
    
    // Check if returned cleanup fn
    if let Value::Function(cleanup) = result {
        rt.effects[effect_id.0 as usize].cleanup = Some(lua.create_registry_value(cleanup)?);
    }
    
    // Flush render commands to main thread
    let commands = rt.take_render_commands();
    main_tx.send(UICommand::Batch(commands))?;
    
    Ok(())
}
```

## Considerations

1. **Lock granularity**: Use fine-grained locks per subsystem (arena, graph, effects) instead of single big lock
2. **Lock-free structures**: Consider crossbeam or dashmap for lock-free collections where possible
3. **Batching**: Reduce cross-thread messages by batching render commands
4. **Deadlock prevention**: Never hold multiple locks, always acquire in same order
5. **GC coordination**: Lua GC runs on worker thread, doesn't block UI

## Migration Path (Phase 1 → Future)

1. **Phase 1**: Implement with passed runtime (`lua.app_data`)
2. **Phase 2-5**: Keep single-threaded
3. **Phase 6-7**: Add `SharedSignalRuntime` wrapper for mobile
4. **Validate**: Primitives work across threads, tables remain thread-local

## Phase 1 Registry Cleanup Limitation

In Phase 1, proper disposal of `RegistryKey` values is skipped for simplicity. This means:

- **Memory leak for long-running apps**: Registry values (functions, tables stored in signals/derived/effects) will accumulate in Lua's registry
- **Impact**: Each signal/derived/effect stores 1-3 RegistryKeys. For apps creating 10,000+ signals/effects over time, this could grow significantly
- **Workaround**: Restart Lua VM periodically to clear registry
- **Future fix**: Phase 6+ will add proper disposal with explicit lifecycle management

**Why skip for Phase 1:**
- `RegistryKey` cannot be cloned, making clean disposal in `__gc` difficult
- Need to replace keys with placeholder values, which requires careful memory management
- For Phase 1 scope (short-lived processes, TUI/Web targets), the impact is acceptable
- Proper disposal will be implemented when adding UI node lifecycle in Phase 2+

## Performance Targets (Multi-Thread)

- Signal read: < 100ns (no contention)
- Signal write: < 500ns (lock acquisition + notify)
- Effect execution: runs async, doesn't block UI
- 120fps rendering (ProMotion displays)
