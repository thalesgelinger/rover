import SwiftUI
import Metal
import QuartzCore

struct ContentView: View {
    var body: some View {
        MetalHostView()
            .ignoresSafeArea()
    }
}

struct MetalHostView: UIViewRepresentable {
    func makeUIView(context: Context) -> RoverMetalView {
        RoverMetalView()
    }

    func updateUIView(_ uiView: RoverMetalView, context: Context) {}
}

final class RoverMetalView: UIView {
    private let host = RoverMetalHost()
    private var displayLink: CADisplayLink?

    override class var layerClass: AnyClass { CAMetalLayer.self }

    override init(frame: CGRect) {
        super.init(frame: frame)
        commonInit()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        commonInit()
    }

    private func commonInit() {
        guard let metalLayer = layer as? CAMetalLayer else { return }
        metalLayer.device = MTLCreateSystemDefaultDevice()
        metalLayer.pixelFormat = .bgra8Unorm
        metalLayer.framebufferOnly = false
        host.start(layer: metalLayer)
        startDisplayLink()
    }

    private func startDisplayLink() {
        displayLink = CADisplayLink(target: self, selector: #selector(step))
        displayLink?.add(to: .main, forMode: .common)
    }

    @objc private func step() {
        autoreleasepool {
            guard let metalLayer = layer as? CAMetalLayer else { return }
            host.render(layer: metalLayer, scale: contentScaleFactor)
        }
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        if let metalLayer = layer as? CAMetalLayer {
            metalLayer.drawableSize = CGSize(width: bounds.width * contentScaleFactor,
                                             height: bounds.height * contentScaleFactor)
        }
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let touch = touches.first else { return }
        let point = touch.location(in: self)
        host.pointerTap(point: point, scale: contentScaleFactor)
    }
}

final class RoverMetalHost {
    private var handle: UnsafeMutableRawPointer?
    private var device: MTLDevice?
    private var commandQueue: MTLCommandQueue?

    func start(layer: CAMetalLayer) {
        guard handle == nil else { return }
        guard let device = layer.device else { return }
        self.device = device
        self.commandQueue = device.makeCommandQueue()
        let root = (Bundle.main.bundlePath as NSString).appendingPathComponent("rover")
        root.withCString { ptr in
            handle = rover_create(ptr)
        }
    }

    func render(layer: CAMetalLayer, scale: CGFloat) {
        guard let handle, let device, let queue = commandQueue else { return }
        layer.device = device
        guard let drawable = layer.nextDrawable() else { return }
        let texture = drawable.texture
        let ok = rover_render_metal(handle,
                                    Unmanaged.passUnretained(device).toOpaque(),
                                    Unmanaged.passUnretained(queue).toOpaque(),
                                    Unmanaged.passUnretained(texture).toOpaque(),
                                    Int32(texture.width),
                                    Int32(texture.height),
                                    Float(scale))
        if ok, let commandBuffer = queue.makeCommandBuffer() {
            commandBuffer.present(drawable)
            commandBuffer.commit()
        }
    }

    func pointerTap(point: CGPoint, scale: CGFloat) {
        guard let handle else { return }
        let scaled = CGPoint(x: point.x * scale, y: point.y * scale)
        _ = rover_pointer_tap(handle, Float(scaled.x), Float(scaled.y))
    }

    deinit {
        if let handle {
            rover_destroy(handle)
        }
    }
}
