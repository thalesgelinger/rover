# Fase 1: Signal System (Rust Core)

**Status:** ✅ COMPLETE  
**Duration:** 2-3 semanas  
**Dependencies:** Nenhuma

## Objetivo

Implementar o sistema de signals completamente isolado, testável sem UI.

## Entregas

### 1.1 Signal Arena (Storage)

```rust
pub struct SignalArena {
    values: Vec<SignalValue>,
    versions: Vec<u64>,
    current_epoch: u64,
}

pub struct SignalId(u32);

pub enum SignalValue {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(StringId),
    Table(TableId),
}
```

*Critério de validação:*

- [x] Criar signal com valor inicial
- [x] Ler valor via `.val`
- [x] Escrever valor via `.val =`
- [x] Zero allocation em read/write

### 1.2 Subscriber Graph

```rust
pub struct SubscriberGraph {
    dependencies: Vec<SmallVec<[SignalId; 4]>>,
    subscribers: Vec<SmallVec<[SubscriberId; 8]>>,
}

pub enum SubscriberId {
    DerivedSignal(SignalId),
    Effect(EffectId),
    UiNode(NodeId),
}
```

*Critério de validação:*

- [x] Registrar dependência signal → subscriber
- [x] Notificar subscribers quando signal muda
- [x] Propagação em cascata (A → B → C)
- [x] Sem notificação se valor não mudou realmente

### 1.3 Derived Signals

```rust
pub struct DerivedSignal {
    id: SignalId,
    compute: ComputeFn,
    cached_value: SignalValue,
    dirty: bool,
}
```

*Critério de validação:*

- [x] `derive(fn)` cria signal computado
- [x] Recomputa quando dependências mudam
- [x] Lazy evaluation (só computa quando lido)
- [x] Cache de valor (não recomputa se limpo)

### 1.4 Magic Metamethods (Lua Bindings)

```lua
-- Operadores retornam derived signals
local double = count * 2
local is_big = count > 10
local label = "Count: " .. count
```

*Implementar metamethods:*

- [x] `__add`, `__sub`, `__mul`, `__div`, `__mod`, `__unm`
- [x] `__concat`
- [x] `__eq`, `__lt`, `__le`
- [x] `__tostring`

*Critério de validação:*

```lua
local count = signal(5)
local double = count * 2
assert(double.val == 10)

count.val = 20
assert(double.val == 40)  -- reatividade funciona

local is_big = count > 10
assert(is_big.val == true)
```

### 1.5 Effects

```rust
pub struct Effect {
    id: EffectId,
    callback: LuaFunction,
    dependencies: SmallVec<[SignalId; 4]>,
}
```

*Critério de validação:*

- [x] Effect roda uma vez no registro
- [x] Effect re-roda quando dependências mudam
- [x] Tracking automático de dependências
- [x] Cleanup function opcional

### 1.6 Utilities

```lua
rover.any(a, b, c)   -- true se qualquer um true
rover.all(a, b, c)   -- true se todos true
rover.none(a, b, c)  -- true se nenhum true
```

## Testes da Fase 1

```lua
-- test_signals.lua

-- Básico
local count = signal(0)
assert(count.val == 0)
count.val = 5
assert(count.val == 5)

-- Derived implícito
local double = count * 2
assert(double.val == 10)
count.val = 10
assert(double.val == 20)

-- Derived explícito
local info = derive(function()
    return "Count is " .. count.val .. ", double is " .. (count.val * 2)
end)
assert(info.val == "Count is 10, double is 20")

-- Comparisons
local is_big = count > 5
assert(is_big.val == true)
count.val = 3
assert(is_big.val == false)

-- Effect
local effect_count = 0
effect(function()
    local _ = count.val  -- subscribe
    effect_count = effect_count + 1
end)
assert(effect_count == 1)  -- roda no mount
count.val = 100
assert(effect_count == 2)  -- roda na mudança

-- Cascata
local a = signal(1)
local b = a * 2
local c = b * 2
assert(c.val == 4)
a.val = 5
assert(c.val == 20)  -- propaga através de b

-- Não notifica se valor igual
effect_count = 0
local stable = signal(10)
effect(function()
    local _ = stable.val
    effect_count = effect_count + 1
end)
assert(effect_count == 1)
stable.val = 10  -- mesmo valor
assert(effect_count == 1)  -- não re-rodou
```

## Estrutura de Arquivos Fase 1

```
rover-core/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── signal/
│   │   ├── mod.rs
│   │   ├── arena.rs        # SignalArena
│   │   ├── value.rs        # SignalValue enum
│   │   ├── graph.rs        # SubscriberGraph
│   │   ├── derived.rs      # DerivedSignal
│   │   └── effect.rs       # Effect system
│   └── lua/
│       ├── mod.rs
│       ├── signal.rs       # Lua bindings for signal
│       └── metamethods.rs  # __add, __mul, etc
└── tests/
    └── signal_tests.rs
```

## Important Limitation Discovered

**Comparison Operators Cannot Create Reactive Signals**

Devido à semântica do Lua, operadores de comparação (`<`, `>`, `<=`, `>=`, `==`, `~=`) devem retornar valores booleanos e não podem retornar derived signals.

❌ ** NÃO funciona:**
```lua
local count = rover.signal(10)
local is_big = count > 5  -- Retorna boolean plain, NÃO um signal!
```

✅ **Use rover.derive():**
```lua
local count = rover.signal(10)
local is_big = rover.derive(function()
    return count.val > 5
end)
print(is_big.val)  -- true (reativo!)
```

**Motivo:** Lua requer que metamethods de comparação (__lt, __le, __eq) retornem booleanos para uso em condicionais.

## Completion Notes

### Status: ✅ COMPLETE

Phase 1 (Signal System) foi completada com sucesso. Todas as entregas funcionando:
- Signal Arena (storage, versioning, recycling)
- Subscriber Graph (dependencies, propagation)
- Derived Signals (lazy evaluation, caching)
- Magic Metamethods (arithmetic, concat)
- Effects (lifecycle, cleanup, auto-tracking)
- Utilities (any, all)

**Test Coverage:** 20 unit tests + integration test (signal_test.lua)

### Documentação Criada

- `/docs/docs/guides/signals.md` - Guia do usuário com exemplos e padrões
- `/docs/docs/api-reference/signals.md` - Referência completa da API
