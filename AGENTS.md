# AGENTS.md

Purpose: fast, safe defaults for agentic edits in this repo.

## Repo Shape
- Monorepo Rust workspace (Cargo workspace at root).
- Main crates:
  - rover_cli (binary `rover`)
  - rover_core
  - rover_server
  - rover-lsp
  - rover-parser
  - rover-types
  - rover-db
  - rover-ui
  - rover-runtime
  - rover-bundler
  - rover-tui
  - rover-openapi
- Docs app in `docs/` (Docusaurus + TypeScript).

## Build / Check / Lint / Test
Run from repo root unless noted.

### Rust Workspace
- Build all (dev):
  - `cargo build --workspace`
- Build release:
  - `cargo build --workspace --release`
- Type-check all:
  - `cargo check --workspace`
- Lint (clippy):
  - `cargo clippy --workspace --all-targets --all-features`
- Format check:
  - `cargo fmt --all -- --check`
- Format apply:
  - `cargo fmt --all`
- Run all tests:
  - `cargo test --workspace`

### Single Test (important)
- Single package tests:
  - `cargo test -p rover-parser`
- Single test by name substring:
  - `cargo test -p rover-parser should_parse_rest_api_basic`
- Single exact test name:
  - `cargo test -p rover-parser should_parse_rest_api_basic -- --exact`
- Single integration test file:
  - `cargo test -p rover_db --test integration_test`
- Single test inside integration file:
  - `cargo test -p rover_db --test integration_test test_create_db_module`
- Show test output:
  - `cargo test -p rover-parser should_parse_rest_api_basic -- --nocapture`

### Run CLI locally
- Run rover binary via Cargo:
  - `cargo run -p rover_cli -- run path/to/app.lua`
- Check Lua file:
  - `cargo run -p rover_cli -- check path/to/app.lua`
- Format Lua file:
  - `cargo run -p rover_cli -- fmt path/to/app.lua`
- Build app bundle:
  - `cargo run -p rover_cli -- build path/to/app.lua --out my-app`

### Docs (`docs/`)
- Node >= 20 required.
- Install deps:
  - `cd docs && npm ci`
- Start docs dev server:
  - `cd docs && npm run start`
- Build docs:
  - `cd docs && npm run build`
- Typecheck docs:
  - `cd docs && npm run typecheck`

### Perf scripts
- Benchmark suite:
  - `bash tests/perf/run_benchmark.sh`
- Quick wrk test:
  - `bash tests/perf/test.sh`

## Code Style Rules (Rust)
### Formatting
- Always use rustfmt output as source of truth.
- Keep code rustfmt-clean before finalizing.
- Prefer small, composable functions over long blocks.

### Imports
- Group imports in this order when possible:
  1) std
  2) external crates
  3) local crate/module imports
- Use explicit imports; avoid wildcard imports unless existing file pattern uses them.
- Keep `pub use` re-exports near module declarations in `lib.rs`.

### Naming
- Types/traits/enums: `PascalCase`.
- Functions/modules/variables: `snake_case`.
- Constants/statics: `SCREAMING_SNAKE_CASE`.
- Test names: descriptive snake_case, often starts with `should_` or `test_`.

### Types and APIs
- Prefer concrete types at boundaries; use traits/generics only when needed.
- Return `anyhow::Result<T>` for app/service fallible flows.
- Return domain-specific errors (`thiserror`) where crate already models them.
- At Lua boundary code (`mlua`), use `mlua::Result`/`LuaResult` and convert carefully.
- Avoid unnecessary clones in hot paths; pass refs (`&T`, `&str`) where practical.

### Error Handling
- Never silently swallow errors unless intentionally best-effort.
- Add context to propagated errors when it improves debugging.
- Avoid `unwrap()`/`expect()` on user/input/runtime paths.
- `unwrap()` is acceptable in tests and narrow invariant-only code paths.
- Prefer early returns over deep nesting.

### Control Flow
- Prefer `match` for tagged branching and exhaustive handling.
- Keep nested `if let` chains readable; extract helper fn when too deep.
- Keep side effects explicit; avoid hidden global mutation.

### Tests
- Put focused unit tests near code with `#[cfg(test)]`.
- Use integration tests in crate `tests/` dirs for cross-module behavior.
- Assert behavior, not implementation details.
- For parser/runtime behavior, include realistic Lua snippets in tests.

### Performance-sensitive areas
- Treat `rover-server`, `rover-ui`, and parser/type inference as perf-sensitive.
- Prefer allocation-aware patterns in request/parse hot loops.
- Benchmark before/after non-trivial perf edits (`tests/perf/*`).

## Docs/TS Style (docs app)
- Keep TypeScript type-safe (`npm run typecheck` clean).
- Prefer explicit props/types over `any`.
- Follow existing Docusaurus file/layout patterns.

## Agent Working Rules for this Repo
- Make minimal, scoped edits; do not refactor unrelated code.
- Preserve existing crate boundaries and public API unless task requires change.
- If touching multiple crates, validate with targeted package tests first.
- Prefer targeted checks before full workspace runs for faster loops.
- Do not add new dependencies without clear need.
- Do not commit unless user explicitly asks.

## Fast Validation Recipe
- `cargo fmt --all`
- `cargo clippy --workspace --all-targets --all-features`
- `cargo test --workspace`
- If only one crate changed, prefer:
  - `cargo test -p <crate-name>`
  - then run full workspace checks before merge.
