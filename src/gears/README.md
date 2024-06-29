# Rover Gears

Rover gears is the bridge used to compile only one time to use between native platforms

## Workflow

When working with gears every change made in rust must run a build command to update binaries in native platforms
use.
`cargo build --features android`
`cargo build --features ios`
