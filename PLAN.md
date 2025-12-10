# PLAN.md

## Goal
Flutter-like engine with Lua UI: Rust + Skia GPU, minimal platform shells, full frame scheduler, retained scene tree, Rust hit-testing, assets/fonts in Rust, Lua as UI DSL.

## Architecture Philosophy
**Write Once, Run Everywhere (Core):**
- `rover-lua`: All component definitions (col, row, text, button, future: input, scrollview, etc.)
- `rover-render`: Layout engine, layer tree, hit-testing, Skia drawing - backend-agnostic
- `rover-runtime`: State management, action dispatch, dirty tracking, FFI surface

**Platform-Specific (Minimal Shells):**
- Surface creation: iOS (CAMetalLayer), Android (VkSurfaceKHR)
- Event forwarding: iOS (UITouch), Android (MotionEvent)
- Vsync hook: iOS (CADisplayLink), Android (Choreographer)
- Build/packaging: iOS (xcodebuild), Android (gradle)

## Engine Targets

### iOS (Complete)
- Skia on Metal: CAMetalLayer, MtlBackendContext, GPU surfaces
- Frame scheduler: CADisplayLink vsync, dirty tracking, retained layer tree
- Input: UITouch → Rust hit-testing → action dispatch
- Assets/fonts: Rust loaders, device scale support
- Build: ios-runner crate, Xcode template, simctl automation

### Android (In Progress)
- **Backend**: Vulkan-only (API 28+, best performance)
- **Target**: ARM64 (`aarch64-linux-android`)
- **Surface**: VkSurfaceKHR from ANativeWindow
- **Vsync**: Choreographer frame callbacks
- **Input**: MotionEvent → Rust hit-testing (same as iOS)
- **Assets**: AssetManager direct access (zero-copy)
- **Build**: android-runner crate, Gradle + NDK, adb automation

## Implementation Phases

### Phase 1: Rust Toolchain Setup ✅ PLANNED
1. Create `platform/android-runner/` workspace member
2. Configure `.cargo/config.toml` for NDK toolchain
3. Add rustup target: `aarch64-linux-android`
4. Test cross-compile: `cargo build -p rover-runtime --target aarch64-linux-android`

### Phase 2: Vulkan RenderSurface Backend
1. Add `RenderSurfaceBackend::Vulkan` variant to rover-render
2. Implement `RenderSurface::vulkan()` using `skia_safe::gpu::vk`
3. Mirror Metal's flush/present pattern
4. Test compiles

### Phase 3: Runtime Vulkan FFI
1. Add `rover_render_vulkan()` export to rover-runtime
2. Update `rover.h` with Vulkan signature
3. Test cross-compile for android target

### Phase 4: Android Template - Kotlin/Java
1. Create template structure: `platform/android-runner/templates/android-empty/`
2. AndroidManifest.xml (API 28+, fullscreen activity)
3. MainActivity.kt (minimal, hosts RoverVulkanView)
4. RoverVulkanView.kt (Vulkan init, Choreographer, touch events)
5. Gradle build files (AGP 8.2, NDK r26, arm64-v8a)

### Phase 5: Android Template - JNI Bridge
1. rover_jni.cpp (JNI wrappers for FFI)
2. Copy rover.h from iOS template
3. CMakeLists.txt (link librover_runtime.a)
4. Test Gradle build

### Phase 6: AndroidRunner Implementation
1. `ensure_prereqs()`: Check adb, gradle, NDK, rustup target
2. `stage_payload()`: Copy lua + assets to APK assets/rover/
3. `build_rust_staticlib()`: Cross-compile with NDK toolchain
4. `generate_project()`: Template → .rover/build/android/
5. `build_apk()`: gradlew assembleDebug
6. `install_and_launch()`: adb install + am start
7. `build_and_run()`: Orchestrate full flow

### Phase 7: CLI Integration
1. Add rover-android-runner dependency to rover-cli
2. Update `dispatch_platform_run()` for Android
3. Update `dispatch_platform_build()` for Android

### Phase 8: Vulkan WSI Full Implementation
1. Complete Vulkan instance/device/swapchain setup in Kotlin
2. Implement acquire/present in frame callback
3. Pass VkImage handles to Rust via JNI

### Phase 9: End-to-End Testing
1. Build rover CLI release
2. Start ARM64 emulator (API 28+)
3. Run: `rover examples/main.lua -p android`
4. Validate: UI renders, buttons work, 60fps

### Phase 10: Validation & Docs
1. Regression test: iOS still works
2. Physical device testing
3. Update README.md with Android setup
4. Document prerequisites (ANDROID_HOME, NDK)

## Technical Decisions (Locked In)

- **Vulkan-only**: No GLES fallback (simpler, best perf, 94% coverage)
- **Min API 28**: Android 9+ (modern Vulkan drivers)
- **ARM64 first**: `aarch64-linux-android` target (emulator + device)
- **AssetManager direct**: No extraction, zero-copy asset access
- **NDK r26**: Latest LTS, excellent Rust support

## Future Component Additions

### Unified (No Platform Code Needed)
- `rover.input`: Text field component in rover-lua + rover-render
- `rover.scrollview`: Layout engine extension, touch gesture handling in Rust
- `rover.image`: Asset loading in rover-render, Skia image decoding
- `rover.grid`: Layout primitive, flex-like sizing

### Platform-Specific (Optional FFI)
- `rover.camera`: JNI/Swift bridge, capability detection
- `rover.gps`: Location services, permission handling
- `rover.haptics`: Vibration/haptic feedback

## Notes
- LuaJIT interpreter-only on iOS (no JIT compilation allowed)
- Assets in `assets/` beside entry.lua for both platforms
- Scale factor passed from platform shell to Rust (retina/hdpi support)
- Hit-testing entirely in Rust - no platform UI overlay
- Dirty tracking avoids unnecessary renders (battery efficiency)
