# XcodeProjectCLI vendor

Place the XcodeProjectCLI Swift package source here so rover can build it without user installs.

Expected layout:
- `platform/ios-runner/vendor/XcodeProjectCLI/Package.swift`
- sources as published by https://github.com/wojciech-kulik/XcodeProjectCLI

Build: rover calls `swift build` in this directory and uses the binary.
