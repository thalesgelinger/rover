# rover-android-runner

Android platform support for Rover engine.

## Prerequisites

- **Android SDK** (API 28+)
- **Android NDK** r26 or later
- **Rust target**: `aarch64-linux-android`
- **Gradle CLI** (in PATH) or Android Studio (wrapper detected)
- **Emulator or physical device** (ARM64)

## Environment Setup

```bash
# Set Android SDK location
export ANDROID_HOME=/path/to/sdk
# or
export ANDROID_SDK_ROOT=/path/to/sdk

# NDK (auto-detected from SDK if not set)
export ANDROID_NDK_ROOT=$ANDROID_HOME/ndk/26.x.x

# Add Rust target
rustup target add aarch64-linux-android
```

## Usage

```bash
# Run on emulator/device
rover examples/main.lua -p android

# Build only (no install)
rover build examples/main.lua -p android
```

- Activity drives Choreographer vsync; rendering stops on pause/destroy and resizes on `surfaceChanged`.

## Architecture

- **Vulkan-only rendering** (API 28+, SurfaceView + Choreographer)
- **Asset flow**: Lua + assets copied into app files (AssetManager passthrough still TODO)
- **JNI bridge**: surface create/change/destroy, density scaling, vsync + tap input
- **Shared core** with iOS (rover-lua, rover-render, rover-runtime)

## Build Output

- Workspace: `.rover/build/android/`
- APK: `.rover/build/android/project/app/build/outputs/apk/debug/app-debug.apk`
- Staticlib: `target/aarch64-linux-android/debug/librover_runtime.a`
