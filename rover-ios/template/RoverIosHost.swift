import UIKit

typealias RoverRuntime = UnsafeMutableRawPointer
typealias RoverNativeView = UnsafeMutableRawPointer

typealias RoverCreateViewFn = @convention(c) (UInt32, Int32) -> RoverNativeView?
typealias RoverAppendChildFn = @convention(c) (RoverNativeView?, RoverNativeView?) -> Void
typealias RoverRemoveViewFn = @convention(c) (RoverNativeView?) -> Void
typealias RoverSetFrameFn = @convention(c) (RoverNativeView?, Float, Float, Float, Float) -> Void
typealias RoverSetTextFn = @convention(c) (RoverNativeView?, UnsafePointer<CChar>?, Int) -> Void
typealias RoverSetBoolFn = @convention(c) (RoverNativeView?, Bool) -> Void
typealias RoverSetStyleFn = @convention(c) (RoverNativeView?, UInt32, UInt32, UInt32, UInt32, UInt16) -> Void
typealias RoverSetWindowFn = @convention(c) (RoverNativeView?, UnsafePointer<CChar>?, Int, Float, Float) -> Void
typealias RoverStopAppFn = @convention(c) () -> Void

@_silgen_name("rover_ios_init_with_callbacks")
func rover_ios_init_with_callbacks(
    _ createView: RoverCreateViewFn?,
    _ appendChild: RoverAppendChildFn?,
    _ removeView: RoverRemoveViewFn?,
    _ setFrame: RoverSetFrameFn?,
    _ setText: RoverSetTextFn?,
    _ setBool: RoverSetBoolFn?,
    _ setStyle: RoverSetStyleFn?,
    _ setWindow: RoverSetWindowFn?,
    _ stopApp: RoverStopAppFn?
) -> RoverRuntime?

@_silgen_name("rover_ios_free")
func rover_ios_free(_ runtime: RoverRuntime?)

@_silgen_name("rover_ios_load_lua")
func rover_ios_load_lua(_ runtime: RoverRuntime?, _ source: UnsafePointer<CChar>?) -> Int32

@_silgen_name("rover_ios_tick")
func rover_ios_tick(_ runtime: RoverRuntime?) -> Int32

@_silgen_name("rover_ios_next_wake_ms")
func rover_ios_next_wake_ms(_ runtime: RoverRuntime?) -> Int32

@_silgen_name("rover_ios_dispatch_click")
func rover_ios_dispatch_click(_ runtime: RoverRuntime?, _ id: UInt32) -> Int32

@_silgen_name("rover_ios_dispatch_input")
func rover_ios_dispatch_input(_ runtime: RoverRuntime?, _ id: UInt32, _ value: UnsafePointer<CChar>?, _ len: Int) -> Int32

@_silgen_name("rover_ios_dispatch_submit")
func rover_ios_dispatch_submit(_ runtime: RoverRuntime?, _ id: UInt32, _ value: UnsafePointer<CChar>?, _ len: Int) -> Int32

@_silgen_name("rover_ios_dispatch_toggle")
func rover_ios_dispatch_toggle(_ runtime: RoverRuntime?, _ id: UInt32, _ checked: Bool) -> Int32

@_silgen_name("rover_ios_set_viewport")
func rover_ios_set_viewport(_ runtime: RoverRuntime?, _ width: UInt16, _ height: UInt16) -> Int32

@_silgen_name("rover_ios_last_error")
func rover_ios_last_error(_ runtime: RoverRuntime?) -> UnsafePointer<CChar>?

final class RoverButton: UIButton {
    var nodeID: UInt32 = 0
}

final class RoverTextField: UITextField {
    var nodeID: UInt32 = 0
}

final class RoverSwitch: UISwitch {
    var nodeID: UInt32 = 0
}

final class RoverIosHost: NSObject, UITextFieldDelegate {
    static let shared = RoverIosHost()

    private weak var rootView: UIView?
    private var views: [UIView?] = []
    private var runtime: RoverRuntime?
    private var timer: Timer?

    func start(rootView: UIView) {
        self.rootView = rootView
        runtime = rover_ios_init_with_callbacks(
            roverCreateView,
            roverAppendChild,
            roverRemoveView,
            roverSetFrame,
            roverSetText,
            roverSetBool,
            roverSetStyle,
            roverSetWindow,
            roverStopApp
        )
        guard let runtime else { fatalError("failed to initialize rover iOS runtime") }
        guard let sourceURL = Bundle.main.url(forResource: "bundle", withExtension: "lua") else {
            fatalError("missing bundled Lua source")
        }

        let source: String
        do {
            source = try String(contentsOf: sourceURL, encoding: .utf8)
        } catch {
            fatalError("failed to read Lua source: \(error)")
        }

        let code = source.withCString { rover_ios_load_lua(runtime, $0) }
        if code != 0 { fatalError(lastError()) }
        updateViewport()
        tick()
    }

    func stop() {
        timer?.invalidate()
        rover_ios_free(runtime)
        runtime = nil
    }

    func updateViewport() {
        guard let rootView else { return }
        let width = UInt16(max(1, min(rootView.bounds.width, CGFloat(UInt16.max))))
        let height = UInt16(max(1, min(rootView.bounds.height, CGFloat(UInt16.max))))
        _ = rover_ios_set_viewport(runtime, width, height)
        tick()
    }

    func createView(nodeID: UInt32, kind: Int32) -> RoverNativeView? {
        let view: UIView
        switch kind {
        case 0:
            view = rootView ?? UIView()
        case 4:
            let label = UILabel()
            label.numberOfLines = 0
            label.lineBreakMode = .byWordWrapping
            view = label
        case 5:
            let button = RoverButton(type: .system)
            button.nodeID = nodeID
            button.titleLabel?.numberOfLines = 0
            button.titleLabel?.lineBreakMode = .byWordWrapping
            button.addTarget(self, action: #selector(buttonClicked(_:)), for: .touchUpInside)
            view = button
        case 6:
            let input = RoverTextField(frame: .zero)
            input.nodeID = nodeID
            input.borderStyle = .roundedRect
            input.delegate = self
            input.addTarget(self, action: #selector(inputChanged(_:)), for: .editingChanged)
            view = input
        case 7:
            let toggle = RoverSwitch(frame: .zero)
            toggle.nodeID = nodeID
            toggle.addTarget(self, action: #selector(switchChanged(_:)), for: .valueChanged)
            view = toggle
        case 9:
            let scrollView = UIScrollView()
            scrollView.showsHorizontalScrollIndicator = true
            scrollView.showsVerticalScrollIndicator = true
            scrollView.alwaysBounceHorizontal = true
            scrollView.alwaysBounceVertical = true
            view = scrollView
        default:
            view = UIView()
        }
        setView(view, at: nodeID)
        return Unmanaged.passUnretained(view).toOpaque()
    }

    func appendChild(parent: RoverNativeView?, child: RoverNativeView?) {
        guard let parent, let child else { return }
        let parentView = Unmanaged<UIView>.fromOpaque(parent).takeUnretainedValue()
        let childView = Unmanaged<UIView>.fromOpaque(child).takeUnretainedValue()
        if childView.superview !== parentView {
            parentView.addSubview(childView)
        }
        updateScrollContentSizeChain(from: childView)
    }

    func removeView(view: RoverNativeView?) {
        guard let view else { return }
        let uiView = Unmanaged<UIView>.fromOpaque(view).takeUnretainedValue()
        uiView.removeFromSuperview()
    }

    func setFrame(view: RoverNativeView?, x: Float, y: Float, width: Float, height: Float) {
        guard let view else { return }
        let uiView = Unmanaged<UIView>.fromOpaque(view).takeUnretainedValue()
        let intrinsic = uiView.intrinsicContentSize
        let frameWidth = intrinsic.width > 0 ? max(CGFloat(width), intrinsic.width) : CGFloat(width)
        let frameHeight = intrinsic.height > 0 ? max(CGFloat(height), intrinsic.height) : CGFloat(height)
        uiView.frame = CGRect(x: CGFloat(x), y: CGFloat(y), width: frameWidth, height: frameHeight)
        updateScrollContentSizeChain(from: uiView)
    }

    private func updateScrollContentSizeChain(from view: UIView) {
        if let scrollView = view as? UIScrollView {
            updateScrollContentSize(scrollView)
        }

        var parent = view.superview
        while let current = parent {
            if let scrollView = current as? UIScrollView {
                updateScrollContentSize(scrollView)
            }
            parent = current.superview
        }
    }

    private func updateScrollContentSize(_ scrollView: UIScrollView) {
        var contentRect = CGRect(origin: .zero, size: scrollView.bounds.size)
        for child in scrollView.subviews {
            contentRect = contentRect.union(child.frame)
        }
        scrollView.contentSize = CGSize(
            width: max(scrollView.bounds.width + 1, contentRect.maxX),
            height: max(scrollView.bounds.height + 1, contentRect.maxY)
        )
    }

    func setText(view: RoverNativeView?, ptr: UnsafePointer<CChar>?, len: Int) {
        guard let view, let ptr else { return }
        let bytes = UnsafeRawPointer(ptr).assumingMemoryBound(to: UInt8.self)
        let buffer = UnsafeBufferPointer(start: bytes, count: len)
        let value = String(decoding: buffer, as: UTF8.self)
        let uiView = Unmanaged<UIView>.fromOpaque(view).takeUnretainedValue()
        if let label = uiView as? UILabel {
            label.text = value
        } else if let button = uiView as? UIButton {
            button.setTitle(value, for: .normal)
        } else if let input = uiView as? UITextField {
            input.text = value
        }
    }

    func setBool(view: RoverNativeView?, value: Bool) {
        guard let view else { return }
        let uiView = Unmanaged<UIView>.fromOpaque(view).takeUnretainedValue()
        if let toggle = uiView as? UISwitch {
            toggle.isOn = value
        }
    }

    func setStyle(view: RoverNativeView?, flags: UInt32, bgRgba: UInt32, borderRgba: UInt32, textRgba: UInt32, borderWidth: UInt16) {
        guard let view else { return }
        let uiView = Unmanaged<UIView>.fromOpaque(view).takeUnretainedValue()
        if flags & 1 != 0 { uiView.backgroundColor = color(bgRgba) }
        if flags & 2 != 0 { uiView.layer.borderColor = color(borderRgba).cgColor }
        if flags & 8 != 0 { uiView.layer.borderWidth = CGFloat(borderWidth) }
        if flags & 4 != 0 {
            let text = color(textRgba)
            if let label = uiView as? UILabel {
                label.textColor = text
            } else if let button = uiView as? UIButton {
                button.tintColor = text
            } else if let input = uiView as? UITextField {
                input.textColor = text
            }
        }
    }

    func setWindow(view: RoverNativeView?, titlePtr: UnsafePointer<CChar>?, len: Int, width: Float, height: Float) {}

    @objc private func buttonClicked(_ sender: RoverButton) {
        _ = rover_ios_dispatch_click(runtime, sender.nodeID)
        tick()
    }

    @objc private func inputChanged(_ sender: RoverTextField) {
        dispatchText(sender.text ?? "", nodeID: sender.nodeID, submit: false)
        tick()
    }

    func textFieldShouldReturn(_ textField: UITextField) -> Bool {
        guard let input = textField as? RoverTextField else { return true }
        dispatchText(input.text ?? "", nodeID: input.nodeID, submit: true)
        input.resignFirstResponder()
        tick()
        return true
    }

    @objc private func switchChanged(_ sender: RoverSwitch) {
        _ = rover_ios_dispatch_toggle(runtime, sender.nodeID, sender.isOn)
        tick()
    }

    private func tick() {
        let code = rover_ios_tick(runtime)
        if code != 0 { fatalError(lastError()) }
        scheduleNextWake()
    }

    private func dispatchText(_ value: String, nodeID: UInt32, submit: Bool) {
        let sent = value.utf8.withContiguousStorageIfAvailable { buffer -> Bool in
            guard let base = buffer.baseAddress else { return false }
            base.withMemoryRebound(to: CChar.self, capacity: buffer.count) { ptr in
                if submit {
                    _ = rover_ios_dispatch_submit(runtime, nodeID, ptr, buffer.count)
                } else {
                    _ = rover_ios_dispatch_input(runtime, nodeID, ptr, buffer.count)
                }
            }
            return true
        } ?? false

        if !sent {
            value.withCString { ptr in
                if submit {
                    _ = rover_ios_dispatch_submit(runtime, nodeID, ptr, strlen(ptr))
                } else {
                    _ = rover_ios_dispatch_input(runtime, nodeID, ptr, strlen(ptr))
                }
            }
        }
    }

    private func scheduleNextWake() {
        timer?.invalidate()
        let ms = rover_ios_next_wake_ms(runtime)
        if ms < 0 { return }
        timer = Timer.scheduledTimer(withTimeInterval: Double(ms) / 1000.0, repeats: false) { [weak self] _ in
            self?.tick()
        }
    }

    private func setView(_ view: UIView, at nodeID: UInt32) {
        let index = Int(nodeID)
        if index >= views.count {
            views.append(contentsOf: repeatElement(nil, count: index - views.count + 1))
        }
        views[index] = view
    }

    private func color(_ rgba: UInt32) -> UIColor {
        return UIColor(
            red: CGFloat((rgba >> 24) & 0xff) / 255.0,
            green: CGFloat((rgba >> 16) & 0xff) / 255.0,
            blue: CGFloat((rgba >> 8) & 0xff) / 255.0,
            alpha: CGFloat(rgba & 0xff) / 255.0
        )
    }

    private func lastError() -> String {
        guard let ptr = rover_ios_last_error(runtime) else { return "unknown rover iOS error" }
        return String(cString: ptr)
    }
}

final class RoverViewController: UIViewController {
    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground
        RoverIosHost.shared.start(rootView: view)
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        RoverIosHost.shared.updateViewport()
    }
}

@main
final class RoverAppDelegate: UIResponder, UIApplicationDelegate {
    var window: UIWindow?

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
    ) -> Bool {
        let window = UIWindow(frame: UIScreen.main.bounds)
        window.rootViewController = RoverViewController()
        window.makeKeyAndVisible()
        self.window = window
        return true
    }

    func applicationWillTerminate(_ application: UIApplication) {
        RoverIosHost.shared.stop()
    }
}

func roverCreateView(nodeID: UInt32, kind: Int32) -> RoverNativeView? {
    RoverIosHost.shared.createView(nodeID: nodeID, kind: kind)
}

func roverAppendChild(parent: RoverNativeView?, child: RoverNativeView?) {
    RoverIosHost.shared.appendChild(parent: parent, child: child)
}

func roverRemoveView(view: RoverNativeView?) {
    RoverIosHost.shared.removeView(view: view)
}

func roverSetFrame(view: RoverNativeView?, x: Float, y: Float, width: Float, height: Float) {
    RoverIosHost.shared.setFrame(view: view, x: x, y: y, width: width, height: height)
}

func roverSetText(view: RoverNativeView?, ptr: UnsafePointer<CChar>?, len: Int) {
    RoverIosHost.shared.setText(view: view, ptr: ptr, len: len)
}

func roverSetBool(view: RoverNativeView?, value: Bool) {
    RoverIosHost.shared.setBool(view: view, value: value)
}

func roverSetStyle(view: RoverNativeView?, flags: UInt32, bgRgba: UInt32, borderRgba: UInt32, textRgba: UInt32, borderWidth: UInt16) {
    RoverIosHost.shared.setStyle(view: view, flags: flags, bgRgba: bgRgba, borderRgba: borderRgba, textRgba: textRgba, borderWidth: borderWidth)
}

func roverSetWindow(view: RoverNativeView?, title: UnsafePointer<CChar>?, len: Int, width: Float, height: Float) {
    RoverIosHost.shared.setWindow(view: view, titlePtr: title, len: len, width: width, height: height)
}

func roverStopApp() {}
