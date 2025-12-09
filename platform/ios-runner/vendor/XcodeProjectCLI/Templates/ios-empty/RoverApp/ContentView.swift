import SwiftUI

struct ContentView: View {
    @StateObject private var host = RoverHost()

    var body: some View {
        GeometryReader { geo in
            ZStack {
                if let image = host.image {
                    Image(decorative: image, scale: 1, orientation: .up)
                        .resizable()
                        .aspectRatio(contentMode: .fit)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                        .overlay(hitOverlay(size: geo.size))
                } else {
                    ProgressView("Loading...")
                }
            }
            .onAppear { host.start(targetSize: geo.size) }
        }
    }

    private func hitOverlay(size: CGSize) -> some View {
        let imgSize = host.imageSize
        let scaleX = imgSize.width > 0 ? size.width / imgSize.width : 1
        let scaleY = imgSize.height > 0 ? size.height / imgSize.height : 1
        return ZStack {
            ForEach(host.hits.indices, id: \.self) { idx in
                let hit = host.hits[idx]
                Button(action: { host.dispatch(action: hit.action) }) {
                    Color.clear
                }
                .frame(width: hit.w * scaleX, height: hit.h * scaleY)
                .position(x: (hit.x + hit.w / 2) * scaleX, y: (hit.y + hit.h / 2) * scaleY)
            }
        }
    }
}

final class RoverHost: ObservableObject {
    @Published var image: CGImage?
    @Published var hits: [HitRect] = []
    @Published var imageSize: CGSize = .zero
    private var handle: UnsafeMutableRawPointer?
    private var lastSize: CGSize = .zero

    func start(targetSize: CGSize) {
        guard handle == nil else { render(size: targetSize); return }
        lastSize = targetSize
        let root = (Bundle.main.bundlePath as NSString).appendingPathComponent("rover")
        root.withCString { ptr in
            handle = rover_create(ptr)
        }
        render(size: targetSize)
    }

    func render(size: CGSize) {
        guard let handle else { return }
        lastSize = size
        let width = Int(size.width.rounded(.up))
        let height = Int(size.height.rounded(.up))
        let img = rover_render_rgba(handle, Int32(width), Int32(height))
        apply(image: img)
    }

    func dispatch(action: String) {
        guard let handle else { return }
        action.withCString { ptr in
            _ = rover_dispatch_action_json(handle, ptr)
        }
        render(size: lastSize)
    }

    private func apply(image img: RoverImage) {
        guard let base = img.data, img.len > 0 else { return }
        let hitsStr = img.hits_json.flatMap { String(cString: $0) }
        let data = Data(bytes: base, count: img.len)
        rover_image_free(img)
        guard let provider = CGDataProvider(data: data as CFData) else { return }
        let colorSpace = CGColorSpaceCreateDeviceRGB()
        let bitmapInfo = CGBitmapInfo.byteOrder32Little.union(.premultipliedFirst)
        if let cg = CGImage(width: Int(img.width),
                            height: Int(img.height),
                            bitsPerComponent: 8,
                            bitsPerPixel: 32,
                            bytesPerRow: img.row_bytes,
                            space: colorSpace,
                            bitmapInfo: bitmapInfo,
                            provider: provider,
                            decode: nil,
                            shouldInterpolate: true,
                            intent: .defaultIntent) {
            let hitsDecoded: [HitRect]
            if let hitsStr, let data = hitsStr.data(using: .utf8) {
                hitsDecoded = (try? JSONDecoder().decode([HitRect].self, from: data)) ?? []
            } else {
                hitsDecoded = []
            }
            DispatchQueue.main.async {
                self.image = cg
                self.hits = hitsDecoded
                self.imageSize = CGSize(width: Int(img.width), height: Int(img.height))
            }
        }
    }

    deinit {
        if let handle {
            rover_destroy(handle)
        }
    }
}

struct HitRect: Decodable {
    let action: String
    let x: Double
    let y: Double
    let w: Double
    let h: Double
}
