// Typing a text mark in place: a borderless text view over the mark, in its face, size and colour, that grows as
// you type. Return or Esc ends it (⇧↩ or ⌥↩ for a new line); so does a click elsewhere. The whole typing is one
// undo step, and a mark left empty goes.

import AppKit
import ShotsCore

final class TextMarkEditor: NSObject, NSTextViewDelegate {
    let id: Int
    private let isNew: Bool
    private weak var editor: EditorWindow?
    private weak var canvas: CanvasView?
    let view: NSTextView
    private var finished = false

    init(id: Int, isNew: Bool, editor: EditorWindow, canvas: CanvasView) {
        self.id = id
        self.isNew = isNew
        self.editor = editor
        self.canvas = canvas
        view = NSTextView(frame: .zero)
        super.init()
        guard let mark = editor.document.mark(id) else { return }
        let scale = editor.document.scale
        view.isRichText = false
        view.importsGraphics = false
        view.allowsUndo = true
        view.drawsBackground = false
        view.isVerticallyResizable = true
        view.isHorizontallyResizable = true
        view.textContainerInset = .zero
        view.textContainer?.lineFragmentPadding = 0
        view.textContainer?.widthTracksTextView = false
        view.textContainer?.containerSize = CGSize(width: CGFloat.greatestFiniteMagnitude,
                                                   height: CGFloat.greatestFiniteMagnitude)
        view.maxSize = CGSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        let font = Geometry.textFont(mark, scale: scale)
        view.font = NSFont(descriptor: font.fontDescriptor, size: font.pointSize / scale) ?? font
        view.textColor = mark.style.colour.color
        view.insertionPointColor = mark.style.colour.color
        view.string = mark.text
        view.delegate = self
        canvas.marks.addSubview(view)
        place()
        canvas.window?.makeFirstResponder(view)
        view.selectAll(nil)
    }

    /// Over the mark: its top left corner where the mark's is.
    private func place() {
        guard let editor, let canvas, let mark = editor.document.mark(id) else { return }
        view.sizeToFit()
        let box = canvas.viewRect(forShot: Geometry.textBox(mark, scale: editor.document.scale))
        let size = CGSize(width: max(view.frame.width, box.width, 8), height: max(view.frame.height, box.height))
        let topLeft = canvas.viewPoint(forShot: mark.points.first ?? .zero)
        view.frame = CGRect(x: topLeft.x, y: topLeft.y - size.height, width: size.width + 4, height: size.height)
    }

    func textDidChange(_ notification: Notification) {
        guard let editor else { return }
        let typed = view.string
        editor.document.updateLive(id) { $0.text = typed }
        place()
        editor.changed()
    }

    func textView(_ textView: NSTextView, doCommandBy selector: Selector) -> Bool {
        switch selector {
        case #selector(NSResponder.insertNewline(_:)):
            let flags = NSApp.currentEvent?.modifierFlags ?? []
            if flags.contains(.shift) || flags.contains(.option) {
                textView.insertNewlineIgnoringFieldEditor(nil)
                return true
            }
            canvas?.marks.endTextEditing()
            return true
        case #selector(NSResponder.cancelOperation(_:)):
            canvas?.marks.endTextEditing()
            return true
        default:
            return false
        }
    }

    /// Keeps what was typed as one undo step; an empty mark goes (a new one leaves nothing behind).
    func finish() {
        guard !finished, let editor else { return }
        finished = true
        let doc = editor.document
        let typed = view.string.trimmingCharacters(in: .whitespacesAndNewlines)
        view.delegate = nil
        view.removeFromSuperview()
        if typed.isEmpty {
            if isNew {
                doc.cancelLive()
            } else {
                doc.live { $0.marks.removeAll { $0.id == self.id } }
                doc.end()
            }
        } else {
            doc.updateLive(id) { $0.text = self.view.string }
            doc.end()
            doc.selected = id
        }
        if let marks = canvas?.marks { canvas?.window?.makeFirstResponder(marks) }
    }
}
