import SwiftUI

struct ContentView: View {
    @StateObject private var host = RoverHost()

    var body: some View {
        ZStack {
            if let node = host.view {
                RenderNode(node: node) { action in
                    host.dispatch(action: action)
                }
            } else {
                ProgressView("Loading...")
            }
        }
        .padding()
        .onAppear {
            host.start()
        }
    }
}

final class RoverHost: ObservableObject {
    @Published var view: ViewNode?
    private var handle: UnsafeMutableRawPointer?

    func start() {
        guard handle == nil else { return }
        let root = (Bundle.main.bundlePath as NSString).appendingPathComponent("rover")
        root.withCString { ptr in
            handle = rover_create(ptr)
        }
        render()
    }

    func render() {
        guard let handle else { return }
        if let ptr = rover_render_json(handle) {
            updateView(from: ptr)
            rover_string_free(ptr)
        }
    }

    func dispatch(action: String) {
        guard let handle else { return }
        action.withCString { ptr in
            if let out = rover_dispatch_action_json(handle, ptr) {
                updateView(from: out)
                rover_string_free(out)
            }
        }
    }

    private func updateView(from ptr: UnsafeMutablePointer<CChar>) {
        let text = String(cString: ptr)
        guard let data = text.data(using: .utf8) else { return }
        if let node = try? JSONDecoder().decode(ViewNode.self, from: data) {
            DispatchQueue.main.async {
                self.view = node
            }
        }
    }

    deinit {
        if let handle {
            rover_destroy(handle)
        }
    }
}

struct RenderNode: View {
    let node: ViewNode
    var onAction: (String) -> Void

    var body: some View {
        switch node.kind {
        case "col":
            VStack(alignment: .leading, spacing: 8) {
                ForEach(Array(node.children.enumerated()), id: \.0) { _, child in
                    RenderNode(node: child, onAction: onAction)
                }
            }
            .frame(maxWidth: frameWidth(node.width), maxHeight: frameHeight(node.height))
        case "row":
            HStack(alignment: .center, spacing: 8) {
                ForEach(Array(node.children.enumerated()), id: \.0) { _, child in
                    RenderNode(node: child, onAction: onAction)
                }
            }
            .frame(maxWidth: frameWidth(node.width), maxHeight: frameHeight(node.height))
        case "text":
            Text(node.text ?? "")
                .frame(maxWidth: frameWidth(node.width), maxHeight: frameHeight(node.height))
        case "button":
            Button(node.text ?? "Button") {
                if let action = node.action {
                    onAction(action)
                }
            }
            .frame(maxWidth: frameWidth(node.width), maxHeight: frameHeight(node.height))
        default:
            EmptyView()
        }
    }

    private func frameWidth(_ dim: Dimension?) -> CGFloat? {
        guard let dim else { return nil }
        switch dim {
        case .auto:
            return nil
        case .full:
            return .infinity
        case .px(let v):
            return CGFloat(v)
        }
    }

    private func frameHeight(_ dim: Dimension?) -> CGFloat? {
        guard let dim else { return nil }
        switch dim {
        case .auto:
            return nil
        case .full:
            return .infinity
        case .px(let v):
            return CGFloat(v)
        }
    }
}

struct ViewNode: Decodable {
    let kind: String
    let children: [ViewNode]
    let text: String?
    let width: Dimension?
    let height: Dimension?
    let action: String?
}

enum Dimension: Decodable {
    case auto
    case full
    case px(Double)

    private enum CodingKeys: String, CodingKey {
        case kind
        case value
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let kind = try container.decode(String.self, forKey: .kind)
        switch kind {
        case "auto":
            self = .auto
        case "full":
            self = .full
        case "px":
            let value = try container.decode(Double.self, forKey: .value)
            self = .px(value)
        default:
            throw DecodingError.dataCorrupted(.init(codingPath: decoder.codingPath, debugDescription: "unknown dimension"))
        }
    }
}
