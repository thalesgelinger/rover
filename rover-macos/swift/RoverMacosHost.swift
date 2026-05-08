import AppKit
import Foundation

typealias RoverRuntime = UnsafeMutableRawPointer
typealias RoverNativeView = UnsafeMutableRawPointer

typealias RoverCreateViewFn = @convention(c) (UInt32, Int32) -> RoverNativeView?
typealias RoverAppendChildFn = @convention(c) (RoverNativeView?, RoverNativeView?) -> Void
typealias RoverRemoveViewFn = @convention(c) (RoverNativeView?) -> Void
typealias RoverSetFrameFn = @convention(c) (RoverNativeView?, Float, Float, Float, Float) -> Void
typealias RoverSetTextFn = @convention(c) (RoverNativeView?, UnsafePointer<CChar>?, Int) -> Void
typealias RoverSetBoolFn = @convention(c) (RoverNativeView?, Bool) -> Void
typealias RoverSetStyleFn = @convention(c) (RoverNativeView?, UnsafePointer<CChar>?, Int, UnsafePointer<CChar>?, Int, Float, UnsafePointer<CChar>?, Int) -> Void
typealias RoverSetWindowFn = @convention(c) (RoverNativeView?, UnsafePointer<CChar>?, Int, Float, Float) -> Void
typealias RoverStopAppFn = @convention(c) () -> Void

@_silgen_name("rover_macos_init_with_callbacks")
func rover_macos_init_with_callbacks(
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

@_silgen_name("rover_macos_free")
func rover_macos_free(_ runtime: RoverRuntime?)

@_silgen_name("rover_macos_load_lua")
func rover_macos_load_lua(_ runtime: RoverRuntime?, _ source: UnsafePointer<CChar>?) -> Int32

@_silgen_name("rover_macos_tick")
func rover_macos_tick(_ runtime: RoverRuntime?) -> Int32

@_silgen_name("rover_macos_next_wake_ms")
func rover_macos_next_wake_ms(_ runtime: RoverRuntime?) -> Int32

@_silgen_name("rover_macos_dispatch_click")
func rover_macos_dispatch_click(_ runtime: RoverRuntime?, _ id: UInt32) -> Int32

@_silgen_name("rover_macos_dispatch_input")
func rover_macos_dispatch_input(_ runtime: RoverRuntime?, _ id: UInt32, _ value: UnsafePointer<CChar>?) -> Int32

@_silgen_name("rover_macos_dispatch_submit")
func rover_macos_dispatch_submit(_ runtime: RoverRuntime?, _ id: UInt32, _ value: UnsafePointer<CChar>?) -> Int32

@_silgen_name("rover_macos_dispatch_toggle")
func rover_macos_dispatch_toggle(_ runtime: RoverRuntime?, _ id: UInt32, _ checked: Bool) -> Int32

@_silgen_name("rover_macos_set_viewport")
func rover_macos_set_viewport(_ runtime: RoverRuntime?, _ width: UInt16, _ height: UInt16) -> Int32

@_silgen_name("rover_macos_last_error")
func rover_macos_last_error(_ runtime: RoverRuntime?) -> UnsafePointer<CChar>?

final class RoverContainerView: NSView {
    override var isFlipped: Bool { true }
}

final class RoverButton: NSButton {
    var nodeID: UInt32 = 0
}

final class RoverTextField: NSTextField {
    var nodeID: UInt32 = 0
}

final class RoverCheckbox: NSButton {
    var nodeID: UInt32 = 0
}

final class RoverMacosHost: NSObject, NSApplicationDelegate, NSTextFieldDelegate, NSWindowDelegate {
    static let shared = RoverMacosHost()

    private var window: NSWindow?
    private var views: [UInt32: NSView] = [:]
    private var runtime: RoverRuntime?
    private var timer: Timer?
    private var applyingWindowFrame = false

    func start(sourcePath: String) {
        runtime = rover_macos_init_with_callbacks(
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
        guard let runtime else {
            fatalError("failed to initialize rover macOS runtime")
        }

        let source: String
        do {
            source = try String(contentsOfFile: sourcePath, encoding: .utf8)
        } catch {
            fatalError("failed to read Lua file: \(error)")
        }

        let code = source.withCString { rover_macos_load_lua(runtime, $0) }
        if code != 0 {
            fatalError(lastError())
        }

        tick()
        scheduleTimer()
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)
    }

    func applicationWillTerminate(_ notification: Notification) {
        timer?.invalidate()
        rover_macos_free(runtime)
    }

    func windowWillClose(_ notification: Notification) {
        NSApp.terminate(nil)
    }

    func windowDidResize(_ notification: Notification) {
        if applyingWindowFrame { return }
        guard let content = window?.contentView else { return }
        let width = UInt16(max(1, min(content.bounds.width, CGFloat(UInt16.max))))
        let height = UInt16(max(1, min(content.bounds.height, CGFloat(UInt16.max))))
        _ = rover_macos_set_viewport(runtime, width, height)
        tick()
    }

    func createView(nodeID: UInt32, kind: Int32) -> RoverNativeView? {
        let view: NSView
        switch kind {
        case 0:
            let window = NSWindow(
                contentRect: NSRect(x: 0, y: 0, width: 900, height: 640),
                styleMask: [.titled, .closable, .miniaturizable, .resizable],
                backing: .buffered,
                defer: false
            )
            window.contentView = RoverContainerView(frame: NSRect(x: 0, y: 0, width: 900, height: 640))
            window.delegate = self
            window.center()
            window.makeKeyAndOrderFront(nil)
            self.window = window
            view = window.contentView ?? RoverContainerView()
        case 4:
            let text = NSTextField(labelWithString: "")
            text.lineBreakMode = .byWordWrapping
            view = text
        case 5:
            let button = RoverButton(title: "", target: self, action: #selector(buttonClicked(_:)))
            button.nodeID = nodeID
            view = button
        case 6:
            let input = RoverTextField(string: "")
            input.nodeID = nodeID
            input.delegate = self
            input.target = self
            input.action = #selector(inputSubmitted(_:))
            view = input
        case 7:
            let checkbox = RoverCheckbox(checkboxWithTitle: "", target: self, action: #selector(checkboxToggled(_:)))
            checkbox.nodeID = nodeID
            view = checkbox
        case 9:
            let scroll = NSScrollView()
            scroll.hasVerticalScroller = true
            scroll.hasHorizontalScroller = false
            view = scroll
        default:
            view = RoverContainerView()
        }
        views[nodeID] = view
        return Unmanaged.passUnretained(view).toOpaque()
    }

    func appendChild(parent: RoverNativeView?, child: RoverNativeView?) {
        guard let parent, let child else { return }
        let parentView = Unmanaged<NSView>.fromOpaque(parent).takeUnretainedValue()
        let childView = Unmanaged<NSView>.fromOpaque(child).takeUnretainedValue()

        if let scroll = parentView as? NSScrollView {
            scroll.documentView = childView
        } else if childView.superview !== parentView {
            parentView.addSubview(childView)
        }
    }

    func setFrame(view: RoverNativeView?, x: Float, y: Float, width: Float, height: Float) {
        guard let view else { return }
        let nsView = Unmanaged<NSView>.fromOpaque(view).takeUnretainedValue()
        nsView.frame = NSRect(x: CGFloat(x), y: CGFloat(y), width: CGFloat(width), height: CGFloat(height))
    }

    func setText(view: RoverNativeView?, ptr: UnsafePointer<CChar>?, len: Int) {
        guard let view, let ptr else { return }
        let data = Data(bytes: ptr, count: len)
        let value = String(data: data, encoding: .utf8) ?? ""
        let nsView = Unmanaged<NSView>.fromOpaque(view).takeUnretainedValue()

        if let text = nsView as? NSTextField {
            text.stringValue = value
        } else if let button = nsView as? NSButton {
            button.title = value
        }
    }

    func setBool(view: RoverNativeView?, value: Bool) {
        guard let view else { return }
        let nsView = Unmanaged<NSView>.fromOpaque(view).takeUnretainedValue()
        if let button = nsView as? NSButton {
            button.state = value ? .on : .off
        }
    }

    func setStyle(view: RoverNativeView?, bgPtr: UnsafePointer<CChar>?, bgLen: Int, borderPtr: UnsafePointer<CChar>?, borderLen: Int, borderWidth: Float, textPtr: UnsafePointer<CChar>?, textLen: Int) {
        guard let view else { return }
        let nsView = Unmanaged<NSView>.fromOpaque(view).takeUnretainedValue()

        if let bg = color(ptr: bgPtr, len: bgLen) {
            nsView.wantsLayer = true
            nsView.layer?.backgroundColor = bg.cgColor
        }
        if let border = color(ptr: borderPtr, len: borderLen) {
            nsView.wantsLayer = true
            nsView.layer?.borderColor = border.cgColor
        }
        if borderWidth > 0 {
            nsView.wantsLayer = true
            nsView.layer?.borderWidth = CGFloat(borderWidth)
        }
        if let text = color(ptr: textPtr, len: textLen) {
            if let field = nsView as? NSTextField {
                field.textColor = text
            } else if let button = nsView as? NSButton {
                button.contentTintColor = text
            }
        }
    }

    private func color(ptr: UnsafePointer<CChar>?, len: Int) -> NSColor? {
        guard let ptr, len > 0 else { return nil }
        let raw = String(data: Data(bytes: ptr, count: len), encoding: .utf8) ?? ""
        guard raw.hasPrefix("#") else { return nil }
        let hex = String(raw.dropFirst())
        guard hex.count == 6, let value = Int(hex, radix: 16) else { return nil }
        return NSColor(
            red: CGFloat((value >> 16) & 0xff) / 255.0,
            green: CGFloat((value >> 8) & 0xff) / 255.0,
            blue: CGFloat(value & 0xff) / 255.0,
            alpha: 1.0
        )
    }

    func setWindow(view: RoverNativeView?, titlePtr: UnsafePointer<CChar>?, len: Int, width: Float, height: Float) {
        guard let titlePtr else { return }
        let title = String(data: Data(bytes: titlePtr, count: len), encoding: .utf8) ?? "Rover"
        window?.title = title
        applyingWindowFrame = true
        window?.setContentSize(NSSize(width: CGFloat(width), height: CGFloat(height)))
        applyingWindowFrame = false
    }

    func removeView(view: RoverNativeView?) {
        guard let view else { return }
        let nsView = Unmanaged<NSView>.fromOpaque(view).takeUnretainedValue()
        nsView.removeFromSuperview()
    }

    @objc private func buttonClicked(_ sender: RoverButton) {
        _ = rover_macos_dispatch_click(runtime, sender.nodeID)
        tick()
    }

    @objc private func checkboxToggled(_ sender: RoverCheckbox) {
        _ = rover_macos_dispatch_toggle(runtime, sender.nodeID, sender.state == .on)
        tick()
    }

    @objc private func inputSubmitted(_ sender: RoverTextField) {
        sender.stringValue.withCString { value in
            _ = rover_macos_dispatch_submit(runtime, sender.nodeID, value)
        }
        tick()
    }

    func controlTextDidChange(_ notification: Notification) {
        guard let input = notification.object as? RoverTextField else { return }
        input.stringValue.withCString { value in
            _ = rover_macos_dispatch_input(runtime, input.nodeID, value)
        }
        tick()
    }

    private func scheduleTimer() {
        timer?.invalidate()
        timer = Timer.scheduledTimer(withTimeInterval: 1.0 / 60.0, repeats: true) { [weak self] _ in
            self?.tick()
        }
    }

    private func tick() {
        let code = rover_macos_tick(runtime)
        if code != 0 {
            fatalError(lastError())
        }
    }

    private func lastError() -> String {
        guard let ptr = rover_macos_last_error(runtime) else { return "unknown rover macOS error" }
        return String(cString: ptr)
    }
}

func roverCreateView(nodeID: UInt32, kind: Int32) -> RoverNativeView? {
    RoverMacosHost.shared.createView(nodeID: nodeID, kind: kind)
}

func roverAppendChild(parent: RoverNativeView?, child: RoverNativeView?) {
    RoverMacosHost.shared.appendChild(parent: parent, child: child)
}

func roverRemoveView(view: RoverNativeView?) {
    RoverMacosHost.shared.removeView(view: view)
}

func roverSetFrame(view: RoverNativeView?, x: Float, y: Float, width: Float, height: Float) {
    RoverMacosHost.shared.setFrame(view: view, x: x, y: y, width: width, height: height)
}

func roverSetText(view: RoverNativeView?, ptr: UnsafePointer<CChar>?, len: Int) {
    RoverMacosHost.shared.setText(view: view, ptr: ptr, len: len)
}

func roverSetBool(view: RoverNativeView?, value: Bool) {
    RoverMacosHost.shared.setBool(view: view, value: value)
}

func roverSetStyle(view: RoverNativeView?, bgPtr: UnsafePointer<CChar>?, bgLen: Int, borderPtr: UnsafePointer<CChar>?, borderLen: Int, borderWidth: Float, textPtr: UnsafePointer<CChar>?, textLen: Int) {
    RoverMacosHost.shared.setStyle(view: view, bgPtr: bgPtr, bgLen: bgLen, borderPtr: borderPtr, borderLen: borderLen, borderWidth: borderWidth, textPtr: textPtr, textLen: textLen)
}

func roverSetWindow(view: RoverNativeView?, title: UnsafePointer<CChar>?, len: Int, width: Float, height: Float) {
    RoverMacosHost.shared.setWindow(view: view, titlePtr: title, len: len, width: width, height: height)
}

func roverStopApp() {
    NSApp.terminate(nil)
}
