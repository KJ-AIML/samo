// Clips' own small window, for what Samo's widgets can't do: recording the shortcut that opens the list. A shortcut
// needs ⌘, ⌃ or ⌥, so it never takes a key you type. While recording, Delete leaves Clips without a shortcut and Esc
// keeps the one it has. The shortcut is let go while recording, so pressing it records it instead of opening the list.

import AppKit
import ClipsCore
import SamoKit

final class ShortcutWindow: NSObject, NSWindowDelegate {
    private var window: NSWindow?
    private var button: NSButton?
    private var monitor: Any?
    private let get: () -> Shortcut?
    private let set: (Shortcut?) -> Void
    private let pause: (Bool) -> Void

    init(get: @escaping () -> Shortcut?, set: @escaping (Shortcut?) -> Void, pause: @escaping (Bool) -> Void) {
        self.get = get
        self.set = set
        self.pause = pause
    }

    func show() {
        if let window {
            Activation.bringForward()
            window.makeKeyAndOrderFront(nil)
            return
        }
        let palette = Palette.current(NSApp.effectiveAppearance)
        let height: CGFloat = 130
        let window = NSWindow(contentRect: CGRect(x: 0, y: 0, width: 460, height: height),
                              styleMask: [.titled, .closable], backing: .buffered, defer: false)
        window.title = "Clips"
        window.isReleasedWhenClosed = false
        window.delegate = self
        let content = NSView(frame: window.contentLayoutRect)
        content.wantsLayer = true
        content.layer?.backgroundColor = palette.card.color.cgColor

        var y: CGFloat = height - 50
        content.addSubview(heading("Shortcut", y: y, palette: palette))
        y -= 34
        content.addSubview(text("Open the list", y: y, palette: palette))
        let button = NSButton(title: title(), target: self, action: #selector(record))
        button.frame = CGRect(x: 300, y: y - 4, width: 140, height: 28)
        button.bezelStyle = .rounded
        content.addSubview(button)
        self.button = button
        y -= 34
        let hint = text("Click, then press the keys. ⌫ for none, Esc to keep it.", y: y, palette: palette)
        hint.frame.size.width = 420
        hint.textColor = palette.text2.color
        content.addSubview(hint)

        window.contentView = content
        window.center()
        self.window = window
        Activation.bringForward()
        window.makeKeyAndOrderFront(nil)
    }

    private func title() -> String {
        get()?.display ?? "Set…"
    }

    private func heading(_ words: String, y: CGFloat, palette: Palette) -> NSTextField {
        let label = NSTextField(labelWithString: words.uppercased())
        label.font = Fonts.mono(Metrics.textLabel, weight: .medium)
        label.textColor = palette.text2.color
        label.frame = CGRect(x: 20, y: y, width: 420, height: 16)
        return label
    }

    private func text(_ words: String, y: CGFloat, palette: Palette) -> NSTextField {
        let label = NSTextField(labelWithString: words)
        label.font = Fonts.body(Metrics.textBody)
        label.textColor = palette.text.color
        label.frame = CGRect(x: 20, y: y, width: 260, height: 20)
        return label
    }

    @objc private func record() {
        stopRecording()
        button?.title = "Press keys…"
        pause(true)
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            self?.recorded(event)
            return nil
        }
    }

    private func recorded(_ event: NSEvent) {
        if event.keyCode == 53 {
            stopRecording()
            return
        }
        if event.keyCode == 51 || event.keyCode == 117 {
            set(nil)
            stopRecording()
            return
        }
        guard let shortcut = Shortcut.recorded(keyCode: event.keyCode, cocoaModifiers: event.modifierFlags.rawValue)
        else {
            button?.title = "Needs ⌘, ⌃ or ⌥"
            return
        }
        set(shortcut)
        stopRecording()
    }

    private func stopRecording() {
        guard monitor != nil else { return }
        if let monitor {
            NSEvent.removeMonitor(monitor)
        }
        monitor = nil
        button?.title = title()
        pause(false)
    }

    func windowWillClose(_ notification: Notification) {
        stopRecording()
        window = nil
        button = nil
    }
}
