# Specs – Adding API Definitions

The `specs` module provides a centralized registry of API documentation and type metadata for Rover's built-in globals, response builders, and Lua stdlib.

## Architecture

```
rover-parser/src/
  specs.rs        # Registry + public types (SpecDoc, MemberKind, lookup_spec)
  specs/data.rs   # All spec definitions (build_specs)
```

## Key Types

| Type | Purpose |
|------|---------|
| `ApiSpec` | Internal spec record (id, doc, members, kind, params, returns) |
| `SpecDoc` | Public view of a spec (id, doc, members) |
| `SpecDocMember` | Field/method entry (name, doc, target spec, kind) |
| `MemberKind` | `Field` or `Method` |
| `SpecKind` | `Object` or `Function` |

## Adding a New Spec

1. Open `specs/data.rs`
2. Add entries in `build_specs()` using macros:

```rust
// For objects with fields/methods:
api_object!(
    "my_object",
    "Documentation string.",
    [
        api_member!("field_name" => "target_spec", "Field doc.", field),
        api_member!("method_name" => "return_spec", "Method doc.", method)
    ]
),

// For callable functions:
api_function!(
    "my_function",
    "Doc string.",
    [
        api_param!("arg_name", "arg_type", "param doc")
    ],
    Some("ReturnType")  // or None
),
```

3. Reference spec by id elsewhere:
   - `lookup_spec("my_object")` returns `Option<SpecDoc>`
   - Type inference uses specs to seed type env

## Examples

**Response builder (object returning chainable methods):**
```rust
api_object!(
    "rover_response_json",
    "Build JSON response. Chain :status(code, data).",
    [
        api_member!("status" => "rover_response_builder", "Set status code.", method),
        api_member!("permanent" => "rover_response_builder", "Make permanent redirect.", method)
    ]
),
```

**Guard type (function returning a guard):**
```rust
api_function!(
    "rover_guard_string",
    "Create string guard.",
    [],
    Some("Guard<String>")
),
```

## Usage in Type Inference

`type_inference.rs::seed_symbol_spec_types()` walks the specs registry and populates `TypeEnv` so that:
- `rover.server{}` returns a `RoverServer` table
- `server.json:status(...)` is recognized
- `ctx.params`, `ctx.body:expect(...)` work

## Testing

Run `cargo test -p rover-parser` to verify specs don't break parsing or type checks.
