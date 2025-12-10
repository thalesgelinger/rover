# PLAN.md

## Goal
Flutter-like engine with Lua UI: Rust + Skia GPU, minimal platform shells (iOS first), full frame scheduler, retained scene tree, Rust hit-testing, assets/fonts in Rust, Lua as UI DSL.

## Engine targets
- Skia on Metal (iOS): CAMetalLayer, MtlBackendContext, GPU surfaces; avoid CPU copies.
- Vulkan/GL (Android later) with same RenderSurface API.
- Frame scheduler: vsync-driven (CADisplayLink), state/dirty tracking, layout + paint per frame.
- Retained layer tree: build from Lua virtual tree; cache layout/text; incremental paint.
- Input: pointer/touch forwarded to Rust; hit-testing in Rust (no Swift overlay buttons); action dispatch updates state + triggers re-render.
- Assets/fonts: Rust loaders, device scale support, font fallback cache.
- Output: Skia draws into GPU surface; present via swapchain; optional screenshot/debug RGBA fallback.

## Near-term tasks (iOS)
1) RenderSurface abstraction in rover-render: CPU fallback + Metal surface impl.
2) Runtime frame loop: vsync tick, dirty flag, layout/paint into RenderSurface.
3) Layer tree + hit-test: build retained nodes from Lua values; compute bounds; hit map for pointer events.
4) Input bridge: Swift forwards touches to Rust (x,y,phase); Rust returns actions/state.
5) Metal shell: CAMetalLayer host view; create Skia surface each frame; present.
6) Fonts/assets: load in Rust; expose scale factor from Swift; cache fonts.
7) CLI/run: keep current staging/build; ensure sim/device selection stable.

## Android later
- EGL/Vulkan surface binding, same RenderSurface API.

## Notes
- LuaJIT interpreter-only on iOS; assets in `assets/` beside entry.
- Keep Swift/Kotlin minimal: create surface, forward events, vsync hook.
