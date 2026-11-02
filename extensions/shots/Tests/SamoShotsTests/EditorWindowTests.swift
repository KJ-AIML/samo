// The editor window, driven headless: real views and real events, never shown on screen.

import AppKit
import XCTest
@testable import ShotsCore
@testable import ShotsUI

private final class MemoryDisk: Disk {
    var files: [URL: Data] = [:]
    func makeFolder(_ url: URL) throws {}
    func exists(_ url: URL) -> Bool { files[url] != nil }
    func write(_ data: Data, to url: URL) throws { files[url] = data }
    func replace(_ data: Data, at url: URL) throws { files[url] = data }
}

private final class MemoryClipboard: Clipboard {
    var png: Data?
    var text: String?
    func put(png: Data) -> Bool {
        self.png = png
        return true
    }
    func put(text: String) -> Bool {
        self.text = text
        return true
    }
}

/// What the editor told the rest of Shots.
private final class Calls {
    var saved: [URL] = []
    var pinned: [Shot] = []
    var closed = 0
    var notes: [String] = []
    var edits: (() -> CGImage?)?
}

final class EditorWindowTests: XCTestCase {
    private let disk = MemoryDisk()
    private let clipboard = MemoryClipboard()
    private let calls = Calls()

    private func makeEditor(_ shot: Shot? = nil, text: [String] = [], codes: [String] = []) throws -> EditorWindow {
        let shot = try shot ?? Shot(image: stripes(400, 300), scale: 2, takenAt: Date(timeIntervalSince1970: 1_793_543_712))
        var settings = Settings()
        settings.folder = "/tmp/Shots"
        let calls = calls
        let hooks = EditorHooks(
            settings: { settings }, clipboard: clipboard, disk: disk,
            dragFolder: URL(fileURLWithPath: "/tmp/shots-drag-test"),
            saved: { _, result in result.url.map { calls.saved.append($0) } },
            pin: { calls.pinned.append($0) },
            opened: { _, edits in calls.edits = edits },
            closed: { _ in calls.closed += 1 },
            note: { words, _ in calls.notes.append(words) },
            readText: { _ in text },
            readCodes: { _ in codes }
        )
        let editor = EditorWindow(shot: shot, hooks: hooks)
        editor.prepare()
        return editor
    }

    private func mouse(_ type: NSEvent.EventType, _ editor: EditorWindow, _ p: CGPoint,
                       flags: NSEvent.ModifierFlags = [], clicks: Int = 1) throws -> NSEvent {
        try XCTUnwrap(NSEvent.mouseEvent(
            with: type, location: editor.canvas.windowPoint(forShot: p), modifierFlags: flags, timestamp: 0,
            windowNumber: editor.window.windowNumber, context: nil, eventNumber: 0, clickCount: clicks, pressure: 1
        ))
    }

    private func click(_ editor: EditorWindow, _ p: CGPoint, clicks: Int = 1) throws {
        editor.canvas.marks.mouseDown(with: try mouse(.leftMouseDown, editor, p, clicks: clicks))
        editor.canvas.marks.mouseUp(with: try mouse(.leftMouseUp, editor, p, clicks: clicks))
    }

    private func drag(_ editor: EditorWindow, _ from: CGPoint, _ to: CGPoint, flags: NSEvent.ModifierFlags = []) throws {
        let marks = editor.canvas.marks
        marks.mouseDown(with: try mouse(.leftMouseDown, editor, from, flags: flags))
        for i in 1...8 {
            let t = CGFloat(i) / 8
            let p = CGPoint(x: from.x + (to.x - from.x) * t, y: from.y + (to.y - from.y) * t + sin(t * 3) * 6)
            marks.mouseDragged(with: try mouse(.leftMouseDragged, editor, p, flags: flags))
        }
        marks.mouseDragged(with: try mouse(.leftMouseDragged, editor, to, flags: flags))
        marks.mouseUp(with: try mouse(.leftMouseUp, editor, to, flags: flags))
    }

    private func key(_ editor: EditorWindow, _ characters: String, code: UInt16,
                     flags: NSEvent.ModifierFlags = []) throws -> NSEvent {
        try XCTUnwrap(NSEvent.keyEvent(
            with: .keyDown, location: .zero, modifierFlags: flags, timestamp: 0,
            windowNumber: editor.window.windowNumber, context: nil, characters: characters,
            charactersIgnoringModifiers: characters, isARepeat: false, keyCode: code
        ))
    }

    /// A key as the window delivers it: ⌘ keys first as key equivalents, the rest to whatever has the keyboard.
    private func press(_ editor: EditorWindow, _ characters: String, code: UInt16,
                       flags: NSEvent.ModifierFlags = []) throws {
        let event = try key(editor, characters, code: code, flags: flags)
        if flags.contains(.command), editor.window.performKeyEquivalent(with: event) { return }
        editor.window.firstResponder?.keyDown(with: event)
    }

    private func type(_ editor: EditorWindow, _ words: String) throws {
        let view = try XCTUnwrap(editor.canvas.marks.text?.view)
        view.insertText(words, replacementRange: NSRange(location: NSNotFound, length: 0))
    }

    // MARK: Tests

    func testEditorWindowDrawsEveryTool() throws {
        let editor = try makeEditor()
        let doc = editor.document
        // Every tool that makes a mark, through the pointer.
        let strokes: [(Tool, CGPoint, CGPoint)] = [
            (.arrow, CGPoint(x: 30, y: 30), CGPoint(x: 150, y: 120)),
            (.line, CGPoint(x: 30, y: 250), CGPoint(x: 150, y: 200)),
            (.rectangle, CGPoint(x: 200, y: 30), CGPoint(x: 300, y: 100)),
            (.ellipse, CGPoint(x: 200, y: 130), CGPoint(x: 300, y: 190)),
            (.highlighter, CGPoint(x: 40, y: 160), CGPoint(x: 180, y: 160)),
            (.pen, CGPoint(x: 220, y: 220), CGPoint(x: 380, y: 280)),
            (.blur, CGPoint(x: 320, y: 20), CGPoint(x: 390, y: 80)),
            (.pixelate, CGPoint(x: 320, y: 100), CGPoint(x: 390, y: 160)),
            (.spotlight, CGPoint(x: 395, y: 5), CGPoint(x: 5, y: 295)),
        ]
        for (tool, from, to) in strokes {
            editor.select(tool)
            try drag(editor, from, to)
            XCTAssertEqual(doc.canvas.marks.last?.kind, tool.kind, "\(tool)")
            // A new mark is selected, so the next drag on it would move it; click away first.
            doc.selected = nil
        }
        editor.select(.step)
        try click(editor, CGPoint(x: 100, y: 280))
        editor.select(.text)
        try click(editor, CGPoint(x: 160, y: 270))
        try type(editor, "Look")
        try press(editor, "\r", code: 36)
        XCTAssertNil(editor.canvas.marks.text, "Return ends typing")
        XCTAssertEqual(Set(doc.canvas.marks.map(\.kind)), Set(Mark.Kind.allCases))
        XCTAssertEqual(doc.canvas.marks.first { $0.kind == .text }?.text, "Look")
        // A pen line follows the pointer; a highlighter too.
        XCTAssertGreaterThan(doc.canvas.marks.first { $0.kind == .pen }?.points.count ?? 0, 3)

        // Crop: a box, then Return keeps it.
        editor.select(.crop)
        try drag(editor, CGPoint(x: 20, y: 20), CGPoint(x: 380, y: 280))
        try press(editor, "\r", code: 36)
        XCTAssertEqual(editor.tool, .arrow)
        XCTAssertNotNil(doc.canvas.crop)
        editor.setBackdrop(.ink)
        XCTAssertEqual(doc.canvas.backdrop, .ink)
        let full = doc.canvas

        // Undo to the start, and redo to the end.
        var steps = 0
        while doc.canUndo {
            editor.undo()
            steps += 1
        }
        XCTAssertEqual(doc.canvas, Canvas())
        XCTAssertEqual(steps, strokes.count + 2 + 2, "one step a mark, the crop and the backdrop")
        while doc.canRedo { editor.redo() }
        XCTAssertEqual(doc.canvas, full)

        // Save writes it, marks and all; the editor is clean.
        XCTAssertTrue(editor.save())
        XCTAssertEqual(calls.saved.count, 1)
        XCTAssertFalse(doc.dirty)
        let size = try XCTUnwrap(calls.saved.first.flatMap { disk.files[$0] }.flatMap(Encoding.size(of:)))
        XCTAssertGreaterThan(size.width, 360, "cropped, then padded by the backdrop")
        XCTAssertEqual(FooterView.status(editor).hasSuffix("Saved"), true, FooterView.status(editor))
        editor.close()
    }

    func testShiftKeepsLinesStraightAndBoxesSquare() throws {
        let editor = try makeEditor()
        editor.select(.line)
        try drag(editor, CGPoint(x: 50, y: 50), CGPoint(x: 200, y: 60), flags: .shift)
        let line = try XCTUnwrap(editor.document.canvas.marks.last)
        XCTAssertEqual(line.points[1].y, 50, accuracy: 0.5)
        editor.document.selected = nil
        editor.select(.rectangle)
        try drag(editor, CGPoint(x: 220, y: 40), CGPoint(x: 300, y: 200), flags: .shift)
        let box = try XCTUnwrap(editor.document.canvas.marks.last).box
        XCTAssertEqual(box.width, box.height, accuracy: 0.5)
        editor.close()
    }

    func testMarksMoveAndResize() throws {
        let editor = try makeEditor()
        let doc = editor.document
        editor.select(.rectangle)
        try drag(editor, CGPoint(x: 100, y: 100), CGPoint(x: 200, y: 180))
        let id = try XCTUnwrap(doc.selected)
        // Dragging its edge moves it.
        try drag(editor, CGPoint(x: 100, y: 140), CGPoint(x: 130, y: 150))
        XCTAssertEqual(try XCTUnwrap(doc.mark(id)).box.origin, CGPoint(x: 130, y: 110))
        // Dragging its top right handle resizes it from the bottom left.
        try drag(editor, CGPoint(x: 230, y: 190), CGPoint(x: 280, y: 250))
        XCTAssertEqual(try XCTUnwrap(doc.mark(id)).box, CGRect(x: 130, y: 110, width: 150, height: 140))
        // ⌫ deletes it.
        try press(editor, "\u{7f}", code: 51)
        XCTAssertNil(doc.mark(id))
        // A click without a drag makes nothing.
        try click(editor, CGPoint(x: 50, y: 50))
        XCTAssertTrue(doc.canvas.marks.isEmpty)
        editor.close()
    }

    func testUndoRedoFromTheKeyboard() throws {
        let editor = try makeEditor()
        editor.select(.rectangle)
        try drag(editor, CGPoint(x: 100, y: 100), CGPoint(x: 200, y: 180))
        XCTAssertEqual(editor.document.canvas.marks.count, 1)
        try press(editor, "z", code: 6, flags: .command)
        XCTAssertTrue(editor.document.canvas.marks.isEmpty)
        try press(editor, "z", code: 6, flags: [.command, .shift])
        XCTAssertEqual(editor.document.canvas.marks.count, 1)
        // ⌘C copies, as the editor shows it.
        try press(editor, "c", code: 8, flags: .command)
        XCTAssertNotNil(clipboard.png)
        XCTAssertFalse(editor.document.dirty)
        editor.close()
    }

    func testKeysPickToolsButNotWhileTyping() throws {
        let editor = try makeEditor()
        let marks = editor.canvas.marks
        editor.window.makeFirstResponder(marks)
        try press(editor, "r", code: 15)
        XCTAssertEqual(editor.tool, .rectangle)
        try press(editor, "a", code: 0)
        XCTAssertEqual(editor.tool, .arrow)
        try press(editor, "]", code: 30)
        XCTAssertEqual(editor.style.width, .l)
        try press(editor, "[", code: 33)
        XCTAssertEqual(editor.style.width, .m)

        // Typing: R is a letter, not the rectangle.
        try press(editor, "t", code: 17)
        try click(editor, CGPoint(x: 100, y: 200))
        XCTAssertTrue(editor.window.firstResponder is NSTextView)
        try press(editor, "R", code: 15, flags: .shift)
        try type(editor, "ed")
        XCTAssertEqual(editor.tool, .text)
        XCTAssertEqual(editor.document.canvas.marks.last?.text, "Red")
        // ⌘ keys belong to the text while typing.
        XCTAssertFalse(editor.window.performKeyEquivalent(with: try key(editor, "z", code: 6, flags: .command)))
        // Esc ends typing and keeps the text, as one undo step.
        try press(editor, "\u{1b}", code: 53)
        XCTAssertTrue(editor.window.firstResponder === marks)
        XCTAssertEqual(editor.document.canvas.marks.last?.text, "Red")
        editor.undo()
        XCTAssertTrue(editor.document.canvas.marks.isEmpty)
        // A text mark left empty leaves nothing.
        try click(editor, CGPoint(x: 100, y: 200))
        try press(editor, "\u{1b}", code: 53)
        XCTAssertTrue(editor.document.canvas.marks.isEmpty)
        editor.close()
    }

    func testEditorAsksBeforeDiscarding() throws {
        // Clean: it closes without asking.
        let clean = try makeEditor()
        clean.requestClose()
        XCTAssertTrue(clean.isClosed)
        XCTAssertEqual(calls.closed, 1)

        // Changed: it asks; Cancel keeps it open.
        let editor = try makeEditor()
        editor.select(.arrow)
        try drag(editor, CGPoint(x: 50, y: 50), CGPoint(x: 150, y: 150))
        editor.requestClose()
        XCTAssertTrue(editor.asking)
        XCTAssertFalse(editor.isClosed)
        editor.cancelClose()
        XCTAssertFalse(editor.asking)
        XCTAssertFalse(editor.isClosed)
        // Esc twice: deselect, then close, which asks.
        try press(editor, "\u{1b}", code: 53)
        try press(editor, "\u{1b}", code: 53)
        XCTAssertTrue(editor.asking)
        // Discard closes.
        editor.discardAndClose()
        XCTAssertTrue(editor.isClosed)

        // Copied since the change: nothing to lose, so no question.
        let copied = try makeEditor()
        copied.select(.line)
        try drag(copied, CGPoint(x: 50, y: 50), CGPoint(x: 150, y: 150))
        copied.copyShot()
        copied.requestClose()
        XCTAssertTrue(copied.isClosed)

        // Save and close saves, then closes.
        let saving = try makeEditor()
        saving.select(.line)
        try drag(saving, CGPoint(x: 50, y: 50), CGPoint(x: 150, y: 150))
        saving.requestClose()
        saving.saveAndClose()
        XCTAssertTrue(saving.isClosed)
        XCTAssertEqual(calls.saved.count, 1)
    }

    func testUnsavedEditsAreHandedOver() throws {
        let editor = try makeEditor()
        let edits = try XCTUnwrap(calls.edits)
        XCTAssertNil(edits(), "nothing to save yet")
        editor.select(.rectangle)
        try drag(editor, CGPoint(x: 100, y: 100), CGPoint(x: 200, y: 180))
        XCTAssertNotNil(edits())
        editor.copyShot()
        XCTAssertNil(edits())
        editor.close()
    }

    func testPinAndDragFromTheEditor() throws {
        let editor = try makeEditor()
        // Untouched: the shot itself is pinned.
        editor.pin()
        XCTAssertTrue(calls.pinned.last === editor.shot)
        editor.select(.step)
        try click(editor, CGPoint(x: 100, y: 100))
        editor.pin()
        XCTAssertFalse(calls.pinned.last === editor.shot)
        // A drag makes a file in the drag folder, and once it lands, the edits count as out.
        let url = try XCTUnwrap(editor.dragFile())
        XCTAssertEqual(url.deletingLastPathComponent().path, "/tmp/shots-drag-test")
        XCTAssertNotNil(disk.files[url])
        XCTAssertTrue(editor.document.dirty)
        editor.dragLanded()
        XCTAssertFalse(editor.document.dirty)
        editor.close()
    }

    func testToolbarFitsAtTheSmallestWindow() throws {
        let editor = try makeEditor()
        editor.window.setContentSize(editor.window.minSize)
        editor.window.contentView?.layoutSubtreeIfNeeded()
        let buttons = editor.toolbar.subviews.compactMap { $0 as? NSButton }.map(\.frame)
        XCTAssertGreaterThan(buttons.count, 20)
        for (i, a) in buttons.enumerated() {
            XCTAssertTrue(editor.toolbar.bounds.contains(a), "\(a) is cut off")
            for b in buttons[(i + 1)...] {
                XCTAssertFalse(a.intersects(b), "\(a) overlaps \(b)")
            }
        }
        editor.close()
    }

    /// Lets the main run loop turn until `done` or a second passes.
    private func wait(_ done: () -> Bool) {
        let deadline = Date().addingTimeInterval(1)
        while !done(), Date() < deadline {
            RunLoop.main.run(until: Date().addingTimeInterval(0.01))
        }
    }

    func testTabCopiesTheHexUnderThePointer() throws {
        let space = try XCTUnwrap(CGColorSpace(name: CGColorSpace.sRGB))
        let context = try XCTUnwrap(CGContext(data: nil, width: 200, height: 100, bitsPerComponent: 8, bytesPerRow: 0,
                                              space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        context.setFillColor(CGColor(srgbRed: 0xF1 / 255.0, green: 0x5B / 255.0, blue: 0x2A / 255.0, alpha: 1))
        context.fill(CGRect(x: 0, y: 0, width: 200, height: 100))
        context.setFillColor(CGColor(srgbRed: 0, green: 0, blue: 1, alpha: 1))
        context.fill(CGRect(x: 0, y: 0, width: 200, height: 10))
        let editor = try makeEditor(Shot(image: XCTUnwrap(context.makeImage()), scale: 2, takenAt: Date()))
        editor.window.makeFirstResponder(editor.canvas.marks)
        // Nothing under the pointer yet.
        try press(editor, "\t", code: 48)
        XCTAssertNil(clipboard.text)
        editor.canvas.marks.pointer(at: CGPoint(x: 50, y: 50))
        XCTAssertEqual(editor.colourUnderPointer?.hex, "#F15B2A")
        try press(editor, "\t", code: 48)
        XCTAssertEqual(clipboard.text, "#F15B2A")
        try press(editor, "\t", code: 48, flags: .shift)
        XCTAssertEqual(clipboard.text, "rgb(241, 91, 42)")
        // The bottom rows, origin bottom-left as the shot's points run.
        editor.canvas.marks.pointer(at: CGPoint(x: 50, y: 5))
        XCTAssertEqual(editor.colourUnderPointer?.hex, "#0000FF")
        editor.canvas.marks.pointer(at: CGPoint(x: 500, y: 5))
        XCTAssertNil(editor.colourUnderPointer, "off the shot")
        editor.close()
    }

    func testMeasureIsNeverExported() throws {
        let editor = try makeEditor()
        editor.window.makeFirstResponder(editor.canvas.marks)
        try press(editor, "m", code: 46)
        XCTAssertEqual(editor.tool, .measure)
        try drag(editor, CGPoint(x: 40, y: 40), CGPoint(x: 160, y: 100))
        XCTAssertEqual(editor.canvas.marks.measureBox, CGRect(x: 40, y: 40, width: 120, height: 60))
        XCTAssertEqual(clipboard.text, "120 × 60 px · 60 × 30 pt")
        // Never a mark, never in an export, never dirty.
        XCTAssertEqual(editor.document.canvas, Canvas())
        XCTAssertFalse(editor.document.dirty)
        XCTAssertFalse(editor.document.canUndo)
        // A click measures the gaps around it: the stripes are 3 pixels wide.
        try click(editor, CGPoint(x: 1, y: 50))
        XCTAssertNil(editor.canvas.marks.measureBox)
        XCTAssertEqual(clipboard.text, "3 × 300 px · 1.5 × 150 pt")
        // Esc clears a box; leaving the tool does too.
        try drag(editor, CGPoint(x: 40, y: 40), CGPoint(x: 160, y: 100))
        try press(editor, "\u{1b}", code: 53)
        XCTAssertNil(editor.canvas.marks.measureBox)
        XCTAssertFalse(editor.asking, "Esc cleared the box; it didn't close")
        editor.close()
    }

    func testCopyTextCopiesTheShotsText() throws {
        let editor = try makeEditor(text: ["สวัสดี", "world"])
        try press(editor, "c", code: 8, flags: [.command, .shift])
        wait { clipboard.text != nil }
        XCTAssertEqual(clipboard.text, "สวัสดี\nworld")
        XCTAssertEqual(calls.notes.last, "Copied: สวัสดี (+1 more line)")
        XCTAssertNil(clipboard.png, "the text, not the shot")
        // Nothing to read: said, not copied.
        clipboard.text = nil
        let blank = try makeEditor()
        blank.copyText()
        wait { calls.notes.last == "No text found" }
        XCTAssertEqual(calls.notes.last, "No text found")
        XCTAssertNil(clipboard.text)
        editor.close()
        blank.close()
    }

    func testQRChipAppearsForAShotWithACode() throws {
        let editor = try makeEditor(codes: ["https://github.com/KJ-AIML/samo"])
        wait { !editor.codes.isEmpty }
        XCTAssertEqual(editor.codes, ["https://github.com/KJ-AIML/samo"])
        XCTAssertFalse(editor.footer.codeChip.isHidden)
        XCTAssertEqual(editor.footer.codeChip.title, "QR · github.com")
        let none = try makeEditor()
        wait { false }
        XCTAssertTrue(none.footer.codeChip.isHidden)
        editor.close()
        none.close()
    }

    func testEditorReleasesItsShot() throws {
        weak var gone: Shot?
        weak var goneEditor: EditorWindow?
        try autoreleasepool {
            let shot = Shot(image: try stripes(200, 100), scale: 2, takenAt: Date())
            gone = shot
            let editor = try makeEditor(shot)
            goneEditor = editor
            editor.select(.pixelate)
            try drag(editor, CGPoint(x: 20, y: 20), CGPoint(x: 120, y: 80))
            editor.discardAndClose()
            editor.close()
            calls.edits = nil
        }
        XCTAssertNil(goneEditor, "the editor is gone once closed")
        XCTAssertNil(gone, "and so is its shot")
    }
}
