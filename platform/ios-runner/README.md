# ios-runner

Scaffolding for iOS simulator/device runner.
- Hosts the iOS template used for generated projects
- Links Rust staticlib + bundled Lua/assets
- Targets arm64 simulator first

## Run on simulator
1) Prereqs: Xcode + Command Line Tools installed (`xcodebuild`, `xcrun`, `swift`).
2) Rust target once: `rustup target add aarch64-apple-ios-sim`.
3) From repo root, build/run: `cargo run --release -- run examples/main.lua --platform ios`.
   - Builds `rover-runtime` staticlib for the simulator (lua54 vendored), copies Lua entry + assets into the app bundle, generates the Xcode project, builds, installs, and launches on the first available iPhone simulator.
   - If the simulator is already booted you may see a boot warning; it continues.

Output artifacts land in `.rover/build/ios-sim/DerivedData/Build/Products/Debug-iphonesimulator/RoverApp.app` with the Lua payload under `rover/`.
