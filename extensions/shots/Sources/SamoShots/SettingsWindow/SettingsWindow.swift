// Shots' own small settings window, for what Samo's widgets can't do: choosing a folder, and recording a shortcut.
// A shortcut needs ⌘, ⌃ or ⌥, so it never takes a key you type. While recording, Delete clears it and Esc leaves it
// as it was.

import AppKit
import SamoKit
import ShotsCore

final class SettingsWindow: NSObject, NSWindowDelegate {
    private var window: NSWindow?
    private var recording: String?
    private var monitor: Any?
    private var buttons: [String: NSButton] = [:]
    private var folderLabel: NSTextField?
    private let get: () -> Settings
    private let set: (Settings) -> Void
    private let pauseShortcuts: (Bool) -> Void

    init(get: @escaping () -> Settings, set: @escaping (Settings) -> Void, pauseShortcuts: @escaping (Bool) -> Void) {
        self.get = get
        self.set = set
        self.pauseShortcuts = pauseShortcuts
    }

    func show() {
        if let window {
            NSApp.activate(ignoringOtherApps: true)
            window.makeKeyAndOrderFront(nil)
            return
        }
        let palette = Palette.current(NSApp.effectiveAppearance)
        let height = CGFloat(130 + 34 * Settings.modes.count)
        let window = NSWindow(contentRect: CGRect(x: 0, y: 0, width: 460, height: height),
                              styleMask: [.titled, .closable], backing: .buffered, defer: false)
        window.title = "Shots"
        window.isReleasedWhenClosed = false
        window.delegate = self
        let content = NSView(frame: window.contentLayoutRect)
        content.wantsLayer = true
        content.layer?.backgroundColor = palette.card.color.cgColor

        var y: CGFloat = height - 50
        content.addSubview(heading("Shortcuts", y: y, palette: palette))
        for (mode, label) in Settings.modes {
            y -= 34
            content.addSubview(text(label, y: y, palette: palette))
            let button = NSButton(title: get().shortcuts[mode]?.display ?? "Set…", target: self,
                                  action: #selector(record(_:)))
            button.identifier = NSUserInterfaceItemIdentifier(mode)
            button.frame = CGRect(x: 300, y: y - 4, width: 140, height: 28)
            button.bezelStyle = .rounded
            content.addSubview(button)
            buttons[mode] = button
        }
        y -= 48
        content.addSubview(heading("Saved to", y: y, palette: palette))
        y -= 30
        let folder = text(get().folder, y: y, palette: palette)
        folder.frame.size.width = 270
        folder.lineBreakMode = .byTruncatingMiddle
        content.addSubview(folder)
        folderLabel = folder
        let choose = NSButton(title: "Choose…", target: self, action: #selector(chooseFolder))
        choose.frame = CGRect(x: 300, y: y - 4, width: 140, height: 28)
        choose.bezelStyle = .rounded
        content.addSubview(choose)

        window.contentView = content
        window.center()
        self.window = window
        NSApp.activate(ignoringOtherApps: true)
        window.makeKeyAndOrderFront(nil)
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

    @objc private func record(_ sender: NSButton) {
        guard let mode = sender.identifier?.rawValue else { return }
        stopRecording()
        recording = mode
        sender.title = "Type a shortcut, or ⌫"
        pauseShortcuts(true)
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            self?.recorded(event)
            return nil
        }
    }

    private func recorded(_ event: NSEvent) {
        guard let mode = recording else { return }
        if event.keyCode == 53 {
            stopRecording()
            return
        }
        if event.keyCode == 51 || event.keyCode == 117 {
            var settings = get()
            settings.shortcuts[mode] = nil
            set(settings)
            stopRecording()
            return
        }
        guard let shortcut = Shortcut.recorded(keyCode: event.keyCode, cocoaModifiers: event.modifierFlags.rawValue)
        else {
            buttons[mode]?.title = "Needs ⌘, ⌃ or ⌥"
            return
        }
        var settings = get()
        settings.shortcuts[mode] = shortcut
        set(settings)
        stopRecording()
    }

    private func stopRecording() {
        if let monitor {
            NSEvent.removeMonitor(monitor)
        }
        monitor = nil
        recording = nil
        for (mode, button) in buttons {
            button.title = get().shortcuts[mode]?.display ?? "Set…"
        }
        pauseShortcuts(false)
    }

    @objc private func chooseFolder() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = true
        panel.prompt = "Save here"
        panel.directoryURL = get().folderURL
        guard panel.runModal() == .OK, let url = panel.url else { return }
        var settings = get()
        settings.folder = Settings.abbreviate(url.path)
        set(settings)
        folderLabel?.stringValue = settings.folder
    }

    func windowWillClose(_ notification: Notification) {
        stopRecording()
        window = nil
        buttons = [:]
    }
}
