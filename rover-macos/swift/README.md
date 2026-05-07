# rover-macos AppKit Host

Thin native host for `rover-macos`.

Direction:
- AppKit owns `NSView` objects.
- Rust owns Rover UI runtime, dirty-node updates, and px layout.
- Bridge is C/Objective-C style callbacks, no JSON.
- One native view per Rover `NodeId`.

The Rust ABI is in `rover-macos/src/abi_export.rs`.
