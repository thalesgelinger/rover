# Android Runtime

`rover-android` is the native Android UI target.

- Portable UI stays in `rover.ui`.
- Android initially has no `rover.android` Lua namespace.
- Android owns native Views; Rover owns signals, dirty updates, and dp layout.
- The bridge is JNI calls, not JSON.
- Generated native files live in `.rover/android`.

Run on default ADB device/emulator:

```bash
ROVER_WEB_SKIP_AUTO_BUILD=1 cargo run -p rover_cli -- run examples/ios_counter.lua --platform android
```

Run on a specific device:

```bash
ROVER_WEB_SKIP_AUTO_BUILD=1 cargo run -p rover_cli -- run examples/ios_counter.lua --platform android --device-id emulator-5554
```

Required local tools:

- Android SDK via `ANDROID_HOME` or `ANDROID_SDK_ROOT`.
- Android NDK via `ANDROID_NDK_HOME` or SDK `ndk/` directory.
- `adb` on `PATH`.
- `gradle` on `PATH`, unless `.rover/android/gradlew` exists.

Optional `rover.lua` metadata:

```lua
return {
  name = "Counter",
  android = {
    package_name = "lu.rover.generated.counter",
  },
}
```

Defaults:

- `name`: entry file stem.
- `android.package_name`: `lu.rover.generated.<sanitized-name>`.

Native extension direction:

- `.rover/android` is generated and can be edited while exploring native changes.
- Managed native plugins are reserved under `native/android/plugins/<name>/plugin.lua`.
- Future capture command shape: `rover capture -p android <name>`.
