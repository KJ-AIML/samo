// What the capture overlay is doing (ADR-004 S1, S4, S5): capturing an area or a window, choosing an area to scroll,
// or reading text, picking a colour or measuring. Keys switch modes only before a drag, so a key pressed mid-selection never changes what the
// selection is for. Esc always leaves.

import CoreGraphics
import SamoKit
import Foundation

public enum OverlayMode: String, CaseIterable {
    case area, window, text, colour, ruler, scroll

    /// A drag selects (area, text) or measures a box (ruler).
    public var drags: Bool { self == .area || self == .text || self == .ruler || self == .scroll }
    /// The overlay is dimmed around the selection. Never while picking a colour or measuring: they need true colours.
    public var dims: Bool { self == .area || self == .window || self == .text || self == .scroll }
    /// The magnifier follows the pointer.
    public var magnifies: Bool { self == .area || self == .text || self == .colour || self == .scroll }

    /// The words at the foot of the overlay.
    public var hint: String {
        switch self {
        case .area:
            return "Drag to capture  ·  Space for a window  ·  T text  ·  C colour  ·  R ruler  ·  S scroll  ·  Esc to cancel"
        case .window: return "Click a window  ·  Space for an area  ·  Esc to cancel"
        case .text: return "Drag over text to copy it  ·  T to capture instead  ·  Esc to cancel"
        case .colour: return "Click to copy the hex  ·  ⇧-click for rgb()  ·  C to capture instead  ·  Esc to cancel"
        case .ruler: return "Hover to measure the gaps  ·  Drag for a box  ·  Click to copy  ·  R to capture instead  ·  Esc"
        case .scroll: return "Drag over what you'll scroll  ·  then scroll it yourself  ·  S to capture instead  ·  Esc to cancel"
        }
    }
}

public struct OverlayModes: Equatable {
    public private(set) var mode: OverlayMode

    public init(_ mode: OverlayMode) {
        self.mode = mode
    }

    /// What a key did.
    public enum Result: Equatable {
        case switched
        case nothing
    }

    /// A key while the overlay is up: Space swaps area and window; T, C, R and S go to text, colour, ruler and
    /// scrolling, or back to capturing an area if already there. Nothing changes during a drag.
    public mutating func key(_ key: Character, dragging: Bool) -> Result {
        guard !dragging else { return .nothing }
        let next: OverlayMode
        switch Character(key.lowercased()) {
        case " ":
            guard mode == .area || mode == .window else { return .nothing }
            next = mode == .area ? .window : .area
        case "t": next = mode == .text ? .area : .text
        case "c": next = mode == .colour ? .area : .colour
        case "r": next = mode == .ruler ? .area : .ruler
        case "s": next = mode == .scroll ? .area : .scroll
        default: return .nothing
        }
        mode = next
        return .switched
    }
}

/// What reading a selection did.
public struct ReadingOutcome: Equatable {
    /// The text copied, if any.
    public var copied: String?
    /// QR codes found, for the card.
    public var codes: [String]
    public var note: String
    public var problem: Bool
}

/// Reading a selection (T): its text is copied, and any QR codes go to the card. Nothing is saved or listed.
public enum Reading {
    public static func run(
        _ image: CGImage, clipboard: Clipboard,
        readText: (CGImage) async -> [String] = { await TextReader.read($0) },
        readCodes: (CGImage) async -> [String] = { await QRReader.read($0) }
    ) async -> ReadingOutcome {
        async let lines = readText(image)
        async let codes = readCodes(image)
        return outcome(lines: await lines, codes: await codes, clipboard: clipboard)
    }

    public static func outcome(lines: [String], codes: [String], clipboard: Clipboard) -> ReadingOutcome {
        guard let first = lines.first else {
            return ReadingOutcome(copied: nil, codes: codes, note: codes.isEmpty ? "No text found" : "Found a QR code",
                                  problem: false)
        }
        let text = lines.joined(separator: "\n")
        guard clipboard.put(text: text) else {
            return ReadingOutcome(copied: nil, codes: codes, note: "Couldn't copy the text", problem: true)
        }
        let shown = first.count > 48 ? String(first.prefix(47)) + "…" : first
        let more = lines.count > 1 ? " (+\(lines.count - 1) more \(lines.count == 2 ? "line" : "lines"))" : ""
        return ReadingOutcome(copied: text, codes: codes, note: "Copied: \(shown)\(more)", problem: false)
    }
}

/// How a scrolling capture ended.
public enum ScrollEnd: Equatable {
    /// Esc, or a click outside the area: the person is done.
    case done
    /// A frame didn't overlap the last: the page jumped.
    case lost
    /// As tall as a shot goes.
    case full
    /// The screen stopped coming (macOS ended the stream).
    case failed

    /// What to say beside the usual note, if anything.
    public var note: String? {
        switch self {
        case .done: return nil
        case .lost: return "The page jumped, so Shots kept what it had up to there"
        case .full: return "That's as tall as a shot goes: Shots kept 30,000 px"
        case .failed: return "macOS stopped sending the screen, so Shots kept what it had"
        }
    }
}
