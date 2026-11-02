// What the editor draws on a shot (ADR-004 S3): marks, kept as vectors in the shot's pixels, origin bottom-left as
// Core Graphics draws, so undo is exact and the export is sharp.

import CoreGraphics
import SamoKit
import Foundation

public enum Tool: String, CaseIterable {
    case select, arrow, line, rectangle, ellipse, text, highlighter, pen, step, blur, pixelate, spotlight, crop, measure

    /// The key that picks it, in the editor.
    public var key: Character {
        switch self {
        case .select: return "v"
        case .arrow: return "a"
        case .line: return "l"
        case .rectangle: return "r"
        case .ellipse: return "o"
        case .text: return "t"
        case .highlighter: return "h"
        case .pen: return "p"
        case .step: return "n"
        case .blur: return "b"
        case .pixelate: return "x"
        case .spotlight: return "s"
        case .crop: return "c"
        case .measure: return "m"
        }
    }

    public var label: String {
        switch self {
        case .select: return "Select"
        case .arrow: return "Arrow"
        case .line: return "Line"
        case .rectangle: return "Rectangle"
        case .ellipse: return "Ellipse"
        case .text: return "Text"
        case .highlighter: return "Highlighter"
        case .pen: return "Pen"
        case .step: return "Numbered step"
        case .blur: return "Blur"
        case .pixelate: return "Pixelate"
        case .spotlight: return "Spotlight"
        case .crop: return "Crop"
        case .measure: return "Measure"
        }
    }

    /// The mark it makes; `nil` for select, crop and measure (measuring is a view, never exported).
    public var kind: Mark.Kind? {
        Mark.Kind(rawValue: rawValue)
    }

    public static func forKey(_ key: Character) -> Tool? {
        allCases.first { $0.key == Character(key.lowercased()) }
    }
}

/// Three widths, in points; each kind of mark reads its own size from them.
public enum Width: Int, CaseIterable {
    case s, m, l

    /// Lines, arrows, outlines and the pen.
    public var stroke: CGFloat { [2, 4, 7][rawValue] }
    public var textSize: CGFloat { [16, 24, 36][rawValue] }
    public var stepDiameter: CGFloat { [22, 30, 42][rawValue] }
    public var highlighter: CGFloat { [12, 20, 32][rawValue] }

    public var bigger: Width { Width(rawValue: min(rawValue + 1, Width.l.rawValue)) ?? .l }
    public var smaller: Width { Width(rawValue: max(rawValue - 1, 0)) ?? .s }

    public var label: String { ["S", "M", "L"][rawValue] }
}

public struct Style: Equatable {
    public var colour: RGBA
    public var width: Width

    public init(colour: RGBA, width: Width) {
        self.colour = colour
        self.width = width
    }

    public static let standard = Style(colour: Swatch.signal, width: .m)
}

/// The colours a mark can be. These belong to the shot, not to Shots' interface: Talvyn's colours, plus a
/// highlighter yellow and a green, which Talvyn's palette doesn't have.
public enum Swatch {
    public static let signal = RGBA.hex(0xF15B2A)
    public static let cobalt = RGBA.hex(0x243EB5)
    public static let paleCobalt = RGBA.hex(0xABC0EF)
    public static let yellow = RGBA.hex(0xF7C948)
    public static let green = RGBA.hex(0x2E9E5B)
    public static let ink = RGBA.hex(0x242A33)
    public static let white = RGBA.hex(0xFFFFFF)

    public static let all: [(name: String, colour: RGBA)] = [
        ("Signal", signal), ("Cobalt", cobalt), ("Pale cobalt", paleCobalt), ("Yellow", yellow), ("Green", green),
        ("Ink", ink), ("White", white),
    ]
}

public struct Mark: Equatable, Identifiable {
    public enum Kind: String, CaseIterable {
        case arrow, line, rectangle, ellipse, text, highlighter, pen, step, blur, pixelate, spotlight

        /// Drawn from one corner to the other.
        public var isBox: Bool { [.rectangle, .ellipse, .blur, .pixelate, .spotlight].contains(self) }
        /// From a start to an end.
        public var isSegment: Bool { self == .arrow || self == .line }
        /// Freehand.
        public var isPath: Bool { self == .highlighter || self == .pen }
        /// Takes the pixels under it away.
        public var redacts: Bool { self == .blur || self == .pixelate }
    }

    public let id: Int
    public var kind: Kind
    /// Segments: start and end. Boxes: two opposite corners. Paths: every point. Steps: the centre. Text: its top
    /// left corner.
    public var points: [CGPoint]
    public var style: Style
    public var text: String

    public init(id: Int, kind: Kind, points: [CGPoint], style: Style, text: String = "") {
        self.id = id
        self.kind = kind
        self.points = points
        self.style = style
        self.text = text
    }

    /// The box between the first two points.
    public var box: CGRect {
        guard let a = points.first else { return .null }
        return Geometry.box(a, points.count > 1 ? points[1] : a)
    }
}

/// A backdrop around the exported shot: padding, a ground, rounded corners and a shadow.
public enum Backdrop: String, CaseIterable {
    case cobalt, pale, ink, white

    public var label: String {
        switch self {
        case .cobalt: return "Cobalt"
        case .pale: return "Pale cobalt"
        case .ink: return "Ink"
        case .white: return "White"
        }
    }

    /// The ground, top to bottom: a gentle two-stop gradient from Talvyn's colours.
    public var ground: (top: RGBA, bottom: RGBA) {
        switch self {
        case .cobalt: return (RGBA.hex(0x3550C8), RGBA.hex(0x182D83))
        case .pale: return (RGBA.hex(0xC9D6F5), RGBA.hex(0xABC0EF))
        case .ink: return (RGBA.hex(0x363E4A), RGBA.hex(0x1B2027))
        case .white: return (RGBA.hex(0xFFFFFF), RGBA.hex(0xE6E9EF))
        }
    }
}
