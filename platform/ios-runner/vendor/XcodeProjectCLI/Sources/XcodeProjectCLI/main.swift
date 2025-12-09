import Foundation

struct Args {
    let template: URL
    let output: URL
}

func parseArgs() -> Args {
    var template: URL?
    var output: URL?
    var iter = CommandLine.arguments.dropFirst().makeIterator()
    while let arg = iter.next() {
        switch arg {
        case "--template":
            if let val = iter.next() { template = URL(fileURLWithPath: val) }
        case "--out":
            if let val = iter.next() { output = URL(fileURLWithPath: val) }
        default:
            break
        }
    }
    guard let t = template, let o = output else {
        fputs("usage: xcodeprojectcli --template <dir> --out <dir>\n", stderr)
        exit(1)
    }
    return Args(template: t, output: o)
}

func copyTemplate(from: URL, to: URL) throws {
    let fm = FileManager.default
    if fm.fileExists(atPath: to.path) {
        try fm.removeItem(at: to)
    }
    try fm.createDirectory(at: to, withIntermediateDirectories: true)
    let enumerator = fm.enumerator(at: from, includingPropertiesForKeys: [.isDirectoryKey])!
    for case let url as URL in enumerator {
        let rel = url.path.replacingOccurrences(of: from.path, with: "").trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        let dest = to.appendingPathComponent(rel)
        let attrs = try url.resourceValues(forKeys: [.isDirectoryKey])
        if attrs.isDirectory == true {
            try fm.createDirectory(at: dest, withIntermediateDirectories: true)
        } else {
            try fm.copyItem(at: url, to: dest)
        }
    }
}

let args = parseArgs()
do {
    try copyTemplate(from: args.template, to: args.output)
    print("copied template to \(args.output.path)")
} catch {
    fputs("error: \(error)\n", stderr)
    exit(1)
}
