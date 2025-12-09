// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "XcodeProjectCLI",
    platforms: [
        .macOS(.v13)
    ],
    products: [
        .executable(name: "xcodeprojectcli", targets: ["XcodeProjectCLI"]),
    ],
    targets: [
        .executableTarget(
            name: "XcodeProjectCLI",
            path: "Sources"
        )
    ]
)
