// The editor's document (ADR-004 S3): one shot's marks, crop and backdrop, with undo and redo. History keeps marks,
// never pixels (R17), and a change is dirty until it is copied or saved (R19).

import CoreGraphics
import SamoKit
import Foundation

/// What undo steps through: everything a person changed.
public struct Canvas: Equatable {
    public var marks: [Mark] = []
    /// What the export keeps, in the shot's pixels; `nil` for all of it.
    public var crop: CGRect?
    public var backdrop: Backdrop?

    public init() {}
}

public final class Document {
    public static let historyLimit = 500

    /// The shot's size, in pixels.
    public let size: CGSize
    /// Pixels per point: a mark's width in points is this many pixels wide.
    public let scale: CGFloat
    public private(set) var canvas = Canvas()
    /// The selected mark's id. It is cleared when that mark goes.
    public var selected: Int? {
        didSet {
            if let id = selected, mark(id) == nil { selected = nil }
        }
    }

    private var undoStack: [Canvas] = []
    private var redoStack: [Canvas] = []
    private var exported = Canvas()
    /// What the shot's own file holds: as captured, until Save writes the marks into it.
    private var saved = Canvas()
    /// The canvas before a drag began.
    private var pending: Canvas?
    private var lastId = 0

    public init(size: CGSize, scale: CGFloat) {
        self.size = size
        self.scale = max(scale, 1)
    }

    // MARK: Dirty

    /// Changed since it was last copied or saved. Undoing back to that point is clean again.
    public var dirty: Bool { canvas != exported }

    /// It was just copied, or saved elsewhere, as it is now.
    public func markExported() {
        exported = canvas
    }

    /// It was just saved into the shot's own file, as it is now.
    public func markSaved() {
        exported = canvas
        saved = canvas
    }

    /// The shot's own file shows exactly this.
    public var matchesSaved: Bool { canvas == saved }

    /// Nothing has been drawn, cropped or added: the shot as captured.
    public var untouched: Bool { canvas == Canvas() }

    // MARK: Changes and history

    public var canUndo: Bool { !undoStack.isEmpty || pending != nil }
    public var canRedo: Bool { !redoStack.isEmpty }

    /// One change, one undo step. A change that changes nothing isn't recorded.
    public func change(_ edit: (inout Canvas) -> Void) {
        end()
        let before = canvas
        edit(&canvas)
        if canvas != before { record(before) }
        validateSelection()
    }

    /// A change made over a drag: shown live, and one undo step when `end` is called.
    public func live(_ edit: (inout Canvas) -> Void) {
        if pending == nil { pending = canvas }
        edit(&canvas)
    }

    /// The drag is over.
    public func end() {
        guard let before = pending else { return }
        pending = nil
        if canvas != before { record(before) }
        validateSelection()
    }

    /// The drag is called off: back to where it began, nothing recorded.
    public func cancelLive() {
        guard let before = pending else { return }
        pending = nil
        canvas = before
        validateSelection()
    }

    public func undo() {
        end()
        guard let previous = undoStack.popLast() else { return }
        redoStack.append(canvas)
        canvas = previous
        validateSelection()
    }

    public func redo() {
        end()
        guard let next = redoStack.popLast() else { return }
        undoStack.append(canvas)
        canvas = next
        validateSelection()
    }

    private func record(_ before: Canvas) {
        undoStack.append(before)
        if undoStack.count > Document.historyLimit {
            undoStack.removeFirst(undoStack.count - Document.historyLimit)
        }
        redoStack.removeAll()
    }

    private func validateSelection() {
        if let id = selected, mark(id) == nil { selected = nil }
    }

    // MARK: Marks

    /// An id no mark has had, even one undone away.
    public func newId() -> Int {
        lastId += 1
        return lastId
    }

    public func mark(_ id: Int) -> Mark? {
        canvas.marks.first { $0.id == id }
    }

    /// Adds a mark as one undo step, and returns its id.
    @discardableResult
    public func add(_ kind: Mark.Kind, points: [CGPoint], style: Style, text: String = "") -> Int {
        let id = newId()
        change { $0.marks.append(Mark(id: id, kind: kind, points: points, style: style, text: text)) }
        return id
    }

    /// Changes one mark as one undo step.
    public func update(_ id: Int, _ edit: (inout Mark) -> Void) {
        change { canvas in
            guard let i = canvas.marks.firstIndex(where: { $0.id == id }) else { return }
            edit(&canvas.marks[i])
        }
    }

    /// Changes one mark live, during a drag.
    public func updateLive(_ id: Int, _ edit: (inout Mark) -> Void) {
        live { canvas in
            guard let i = canvas.marks.firstIndex(where: { $0.id == id }) else { return }
            edit(&canvas.marks[i])
        }
    }

    public func delete(_ id: Int) {
        change { $0.marks.removeAll { $0.id == id } }
    }

    /// A step's number: its place among the steps, so deleting one renumbers the rest.
    public func stepNumber(of id: Int) -> Int? {
        Document.stepNumbers(canvas.marks)[id]
    }

    public static func stepNumbers(_ marks: [Mark]) -> [Int: Int] {
        var numbers: [Int: Int] = [:]
        for mark in marks where mark.kind == .step {
            numbers[mark.id] = numbers.count + 1
        }
        return numbers
    }

    /// The topmost mark at `point`, by `Geometry.hits`.
    public func hitTest(_ point: CGPoint, tolerance: CGFloat, interiors: Bool) -> Int? {
        canvas.marks.last { Geometry.hits($0, point, scale: scale, tolerance: tolerance, interiors: interiors) }?.id
    }

    // MARK: Crop and backdrop

    public var fullRect: CGRect { CGRect(origin: .zero, size: size) }

    /// What the export keeps.
    public var visibleRect: CGRect { canvas.crop ?? fullRect }

    /// Crops to `rect` (within the shot, at least 4 pixels a side), or back to the whole shot with `nil`.
    public func setCrop(_ rect: CGRect?) {
        change { canvas in
            guard let rect else {
                canvas.crop = nil
                return
            }
            let clipped = rect.standardized.intersection(fullRect).integral
            guard !clipped.isNull, clipped.width >= 4, clipped.height >= 4 else { return }
            canvas.crop = clipped == fullRect ? nil : clipped
        }
    }

    public func setBackdrop(_ backdrop: Backdrop?) {
        change { $0.backdrop = backdrop }
    }
}
