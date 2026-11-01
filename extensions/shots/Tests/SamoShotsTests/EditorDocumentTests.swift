import CoreGraphics
import XCTest
@testable import ShotsCore

final class EditorDocumentTests: XCTestCase {
    private func document() -> Document {
        Document(size: CGSize(width: 400, height: 300), scale: 2)
    }

    func testUndoRedoIsExact() {
        let doc = document()
        var states: [Canvas] = [doc.canvas]

        // Add.
        let arrow = doc.add(.arrow, points: [CGPoint(x: 10, y: 10), CGPoint(x: 100, y: 80)], style: .standard)
        states.append(doc.canvas)
        // Move, over a drag: one step however many moves.
        for i in 1...5 {
            doc.updateLive(arrow) { $0 = Geometry.moved($0, by: CGVector(dx: CGFloat(i), dy: 0)) }
        }
        doc.end()
        states.append(doc.canvas)
        // Resize by a handle.
        let box = doc.add(.rectangle, points: [CGPoint(x: 50, y: 50), CGPoint(x: 150, y: 120)], style: .standard)
        states.append(doc.canvas)
        doc.update(box) { $0 = Geometry.resized($0, handle: 2, to: CGPoint(x: 200, y: 200), square: false) }
        states.append(doc.canvas)
        // Recolour and rewidth.
        doc.update(arrow) {
            $0.style = Style(colour: Swatch.cobalt, width: .l)
        }
        states.append(doc.canvas)
        // Text, typed.
        let text = doc.add(.text, points: [CGPoint(x: 20, y: 250)], style: .standard)
        states.append(doc.canvas)
        doc.update(text) { $0.text = "Look here" }
        states.append(doc.canvas)
        // Delete.
        doc.delete(box)
        states.append(doc.canvas)
        // Crop and backdrop.
        doc.setCrop(CGRect(x: 10, y: 10, width: 300, height: 200))
        states.append(doc.canvas)
        doc.setBackdrop(.cobalt)
        states.append(doc.canvas)

        XCTAssertEqual(doc.canvas.marks.count, 2)
        XCTAssertEqual(doc.canvas.crop, CGRect(x: 10, y: 10, width: 300, height: 200))

        // Undo, step by step, back to the start: every state exactly as it was.
        for expected in states.reversed().dropFirst() {
            doc.undo()
            XCTAssertEqual(doc.canvas, expected)
        }
        XCTAssertFalse(doc.canUndo)
        // And redo to the end.
        for expected in states.dropFirst() {
            doc.redo()
            XCTAssertEqual(doc.canvas, expected)
        }
        XCTAssertFalse(doc.canRedo)

        // A new change after an undo drops what could be redone.
        doc.undo()
        doc.add(.line, points: [.zero, CGPoint(x: 5, y: 5)], style: .standard)
        XCTAssertFalse(doc.canRedo)
    }

    func testIdsAreNeverReused() {
        let doc = document()
        let first = doc.add(.step, points: [CGPoint(x: 10, y: 10)], style: .standard)
        doc.undo()
        let second = doc.add(.step, points: [CGPoint(x: 10, y: 10)], style: .standard)
        XCTAssertNotEqual(first, second)
    }

    func testStepsNumberThemselves() {
        let doc = document()
        let one = doc.add(.step, points: [CGPoint(x: 10, y: 10)], style: .standard)
        doc.add(.arrow, points: [.zero, CGPoint(x: 9, y: 9)], style: .standard)
        let two = doc.add(.step, points: [CGPoint(x: 50, y: 10)], style: .standard)
        let three = doc.add(.step, points: [CGPoint(x: 90, y: 10)], style: .standard)
        XCTAssertEqual([doc.stepNumber(of: one), doc.stepNumber(of: two), doc.stepNumber(of: three)], [1, 2, 3])
        doc.delete(two)
        XCTAssertEqual(doc.stepNumber(of: three), 2)
        XCTAssertNil(doc.stepNumber(of: two))
        doc.undo()
        XCTAssertEqual(doc.stepNumber(of: three), 3)
    }

    func testHitTestPicksTheTopmost() {
        let doc = document()
        let below = doc.add(.rectangle, points: [CGPoint(x: 0, y: 0), CGPoint(x: 100, y: 100)], style: .standard)
        let above = doc.add(.line, points: [CGPoint(x: 0, y: 50), CGPoint(x: 100, y: 50)], style: .standard)
        // Where both are, the later one wins.
        XCTAssertEqual(doc.hitTest(CGPoint(x: 0, y: 50), tolerance: 4, interiors: false), above)
        // On the rectangle's edge only.
        XCTAssertEqual(doc.hitTest(CGPoint(x: 50, y: 99), tolerance: 4, interiors: false), below)
        // Inside a rectangle: a drawing tool may draw there; the select tool picks it.
        XCTAssertNil(doc.hitTest(CGPoint(x: 30, y: 30), tolerance: 4, interiors: false))
        XCTAssertEqual(doc.hitTest(CGPoint(x: 30, y: 30), tolerance: 4, interiors: true), below)
        // Nowhere near.
        XCTAssertNil(doc.hitTest(CGPoint(x: 300, y: 250), tolerance: 4, interiors: true))

        let step = doc.add(.step, points: [CGPoint(x: 200, y: 200)], style: .standard)
        XCTAssertEqual(doc.hitTest(CGPoint(x: 210, y: 200), tolerance: 2, interiors: false), step)
        let ring = doc.add(.ellipse, points: [CGPoint(x: 250, y: 0), CGPoint(x: 350, y: 100)], style: .standard)
        XCTAssertEqual(doc.hitTest(CGPoint(x: 350, y: 50), tolerance: 3, interiors: false), ring)
        XCTAssertNil(doc.hitTest(CGPoint(x: 300, y: 50), tolerance: 3, interiors: false))
    }

    func testHandlesResizeFromTheOppositeCorner() {
        let mark = Mark(id: 1, kind: .rectangle, points: [CGPoint(x: 10, y: 10), CGPoint(x: 50, y: 30)],
                        style: .standard)
        XCTAssertEqual(Geometry.handles(mark).count, 4)
        // Drag the top right corner (index 2): the bottom left stays.
        let bigger = Geometry.resized(mark, handle: 2, to: CGPoint(x: 90, y: 70), square: false)
        XCTAssertEqual(bigger.box, CGRect(x: 10, y: 10, width: 80, height: 60))
        let square = Geometry.resized(mark, handle: 2, to: CGPoint(x: 90, y: 70), square: true)
        XCTAssertEqual(square.box.width, square.box.height)
        // A segment's end, with ⇧, lies at 45°.
        let line = Mark(id: 2, kind: .line, points: [.zero, CGPoint(x: 10, y: 0)], style: .standard)
        let bent = Geometry.resized(line, handle: 1, to: CGPoint(x: 10, y: 9), square: true)
        XCTAssertEqual(bent.points[1].x, bent.points[1].y, accuracy: 0.001)
        // Steps and text only move.
        XCTAssertTrue(Geometry.handles(Mark(id: 3, kind: .step, points: [.zero], style: .standard)).isEmpty)
    }

    func testCropIsKeptUntilExport() {
        let doc = document()
        let arrow = doc.add(.arrow, points: [CGPoint(x: 10, y: 10), CGPoint(x: 100, y: 80)], style: .standard)
        doc.setCrop(CGRect(x: 50.4, y: 20.2, width: 200, height: 150))
        // Marks stay where they were in the shot; only the kept area changes.
        XCTAssertEqual(doc.mark(arrow)?.points[0], CGPoint(x: 10, y: 10))
        XCTAssertEqual(doc.visibleRect, CGRect(x: 50, y: 20, width: 201, height: 151))
        // Within the shot.
        doc.setCrop(CGRect(x: -50, y: -50, width: 1000, height: 100))
        XCTAssertEqual(doc.visibleRect, CGRect(x: 0, y: 0, width: 400, height: 50))
        // Too small is refused; the whole shot is no crop.
        doc.setCrop(CGRect(x: 0, y: 0, width: 2, height: 2))
        XCTAssertEqual(doc.visibleRect.height, 50)
        doc.setCrop(CGRect(x: 0, y: 0, width: 400, height: 300))
        XCTAssertNil(doc.canvas.crop)
        // Undo brings each crop back.
        doc.undo()
        XCTAssertEqual(doc.visibleRect, CGRect(x: 0, y: 0, width: 400, height: 50))
    }

    func testDirtyUntilCopiedOrSaved() {
        let doc = document()
        // A fresh shot was copied and saved at capture.
        XCTAssertFalse(doc.dirty)
        let id = doc.add(.rectangle, points: [.zero, CGPoint(x: 20, y: 20)], style: .standard)
        XCTAssertTrue(doc.dirty)
        doc.markExported()
        XCTAssertFalse(doc.dirty)
        doc.update(id) { $0.style.width = .l }
        XCTAssertTrue(doc.dirty)
        // Undo back to what was exported: clean again.
        doc.undo()
        XCTAssertFalse(doc.dirty)
        // A change that changes nothing isn't one.
        doc.update(id) { $0.style.width = $0.style.width }
        XCTAssertFalse(doc.dirty)
        // A drag called off leaves nothing behind.
        doc.updateLive(id) { $0 = Geometry.moved($0, by: CGVector(dx: 40, dy: 0)) }
        XCTAssertTrue(doc.dirty)
        doc.cancelLive()
        XCTAssertFalse(doc.dirty)
    }

    func testSelectionClearsWhenItsMarkGoes() {
        let doc = document()
        let id = doc.add(.line, points: [.zero, CGPoint(x: 9, y: 9)], style: .standard)
        doc.selected = id
        doc.undo()
        XCTAssertNil(doc.selected)
        doc.selected = 999
        XCTAssertNil(doc.selected)
    }

    func testHistoryIsCapped() {
        let doc = document()
        for i in 0..<(Document.historyLimit + 100) {
            doc.add(.step, points: [CGPoint(x: CGFloat(i % 400), y: 10)], style: .standard)
        }
        var undone = 0
        while doc.canUndo {
            doc.undo()
            undone += 1
        }
        XCTAssertEqual(undone, Document.historyLimit)
        XCTAssertEqual(doc.canvas.marks.count, 100)
    }

    func testToolsHaveTheirKeys() {
        XCTAssertEqual(Tool.forKey("A"), .arrow)
        XCTAssertEqual(Tool.forKey("x"), .pixelate)
        XCTAssertNil(Tool.forKey("q"))
        // No two tools share a key.
        XCTAssertEqual(Set(Tool.allCases.map(\.key)).count, Tool.allCases.count)
        XCTAssertNil(Tool.select.kind)
        XCTAssertNil(Tool.crop.kind)
        XCTAssertEqual(Tool.blur.kind, .blur)
    }
}
