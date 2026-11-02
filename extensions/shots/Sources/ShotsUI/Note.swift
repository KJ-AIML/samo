// The note near the pointer after a capture (*Copied and saved*), on the readout's glass. It never takes a click or
// the keyboard, and fades by itself. A problem stays a little longer, in Signal.

import AppKit
import ShotsCore

public enum Note {
    private static var current: NSPanel?

    public static func show(_ words: String, problem: Bool) {
        current?.orderOut(nil)
        let palette = Palette.current(NSApplication.shared.effectiveAppearance)
        let attrs: [NSAttributedString.Key: Any] = [
            .font: Fonts.body(Metrics.textBody, weight: .medium),
            .foregroundColor: problem ? palette.signalText.color : palette.glassText.color,
        ]
        let text = NSAttributedString(string: words, attributes: attrs)
        let size = text.size()
        let frameSize = CGSize(width: min(size.width + 28, 520), height: size.height + 16)
        let pointer = NSEvent.mouseLocation
        let panel = NSPanel(
            contentRect: CGRect(origin: CGPoint(x: pointer.x + 16, y: pointer.y - frameSize.height - 16), size: frameSize),
            styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false
        )
        panel.level = .popUpMenu
        panel.ignoresMouseEvents = true
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        let box = NSView(frame: CGRect(origin: .zero, size: frameSize))
        box.wantsLayer = true
        box.layer?.backgroundColor = palette.glass.color.withAlphaComponent(0.95).cgColor
        box.layer?.cornerRadius = Metrics.radiusControl
        box.layer?.borderColor = palette.glassLine.color.cgColor
        box.layer?.borderWidth = 1
        let label = NSTextField(labelWithAttributedString: text)
        label.frame = CGRect(x: 14, y: 8, width: frameSize.width - 28, height: size.height)
        label.lineBreakMode = .byTruncatingTail
        box.addSubview(label)
        panel.contentView = box
        panel.orderFrontRegardless()
        current = panel
        DispatchQueue.main.asyncAfter(deadline: .now() + (problem ? 3.5 : 1.4)) {
            NSAnimationContext.runAnimationGroup({ context in
                context.duration = 0.2
                panel.animator().alphaValue = 0
            }, completionHandler: {
                panel.orderOut(nil)
                if current === panel { current = nil }
            })
        }
    }
}
