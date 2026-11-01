// Asked before an editor with changes not copied or saved closes (R19), in Samo's tokens: Save and close is the
// Ink action, Discard is Signal text (it can't be undone), Cancel keeps editing. Return saves, Esc cancels, ⌘D
// discards.

import AppKit
import ShotsCore

final class DiscardSheet {
    enum Choice {
        case saveAndClose, discard, cancel
    }

    let window: NSWindow

    init(palette: Palette, answer: @escaping (Choice) -> Void) {
        let size = CGSize(width: 420, height: 158)
        window = NSWindow(contentRect: CGRect(origin: .zero, size: size), styleMask: [.titled], backing: .buffered,
                          defer: true)
        window.isReleasedWhenClosed = false
        let content = NSView(frame: CGRect(origin: .zero, size: size))
        content.wantsLayer = true
        content.layer?.backgroundColor = palette.card.color.cgColor

        let title = NSTextField(labelWithAttributedString: NSAttributedString(
            string: "Discard your changes?",
            attributes: [.font: Fonts.display(17), .foregroundColor: palette.text.color]
        ))
        title.frame = CGRect(x: 22, y: size.height - 44, width: size.width - 44, height: 24)
        let body = NSTextField(wrappingLabelWithString: "What you changed since you last copied or saved this shot "
            + "will be lost. The shot stays as it was saved.")
        body.font = Fonts.body(Metrics.textBody)
        body.textColor = palette.text2.color
        body.frame = CGRect(x: 22, y: size.height - 94, width: size.width - 44, height: 44)

        let save = ActionButton("Save and close", kind: .ink)
        let discard = ActionButton("Discard", kind: .danger)
        let cancel = ActionButton("Cancel", kind: .plain)
        save.keyEquivalent = "\r"
        cancel.keyEquivalent = "\u{1b}"
        discard.keyEquivalent = "d"
        discard.keyEquivalentModifierMask = [.command]
        save.onPress = { answer(.saveAndClose) }
        discard.onPress = { answer(.discard) }
        cancel.onPress = { answer(.cancel) }
        var x = size.width - 22
        for button in [save, cancel] {
            button.paint(palette: palette)
            x -= button.width
            button.frame = CGRect(x: x, y: 20, width: button.width, height: 30)
            x -= 8
        }
        discard.paint(palette: palette)
        discard.frame = CGRect(x: 22, y: 20, width: discard.width, height: 30)
        for view in [title, body, save, discard, cancel] { content.addSubview(view) }
        window.contentView = content
    }
}
