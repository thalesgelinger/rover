import AppKit

let app = NSApplication.shared
let host = RoverMacosHost.shared
app.delegate = host

let sourcePath: String
if CommandLine.arguments.count >= 2 {
    sourcePath = CommandLine.arguments[1]
} else if let bundled = Bundle.main.path(forResource: "bundle", ofType: "lua") {
    sourcePath = bundled
} else {
    fatalError("usage: rover-macos-host <app.lua>")
}

host.start(sourcePath: sourcePath)
app.run()
