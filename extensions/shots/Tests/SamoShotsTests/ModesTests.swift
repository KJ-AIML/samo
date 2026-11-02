import CoreGraphics
import XCTest
@testable import ShotsCore

private final class TextBoard: Clipboard {
    var text: String?
    var works = true
    func put(png: Data) -> Bool {
        XCTFail("reading never copies an image")
        return false
    }
    func put(text: String) -> Bool {
        self.text = text
        return works
    }
}

final class ModesTests: XCTestCase {
    func testModeKeysOnlyBeforeADrag() {
        var modes = OverlayModes(.area)
        XCTAssertEqual(modes.key("t", dragging: false), .switched)
        XCTAssertEqual(modes.mode, .text)
        // Mid-drag, a key changes nothing: the selection stays what it was for.
        XCTAssertEqual(modes.key("c", dragging: true), .nothing)
        XCTAssertEqual(modes.mode, .text)
        XCTAssertEqual(modes.key("C", dragging: false), .switched)
        XCTAssertEqual(modes.mode, .colour)
        // The same key again goes back to capturing.
        XCTAssertEqual(modes.key("c", dragging: false), .switched)
        XCTAssertEqual(modes.mode, .area)
        XCTAssertEqual(modes.key("r", dragging: false), .switched)
        XCTAssertEqual(modes.mode, .ruler)
        // Space swaps area and window only.
        XCTAssertEqual(modes.key(" ", dragging: false), .nothing)
        modes = OverlayModes(.area)
        XCTAssertEqual(modes.key(" ", dragging: false), .switched)
        XCTAssertEqual(modes.mode, .window)
        XCTAssertEqual(modes.key(" ", dragging: false), .switched)
        XCTAssertEqual(modes.mode, .area)
        // S scrolls; pressed again, back to capturing. Other keys do nothing.
        XCTAssertEqual(modes.key("s", dragging: false), .switched)
        XCTAssertEqual(modes.mode, .scroll)
        XCTAssertEqual(modes.key("s", dragging: true), .nothing, "not mid-drag")
        XCTAssertEqual(modes.key("s", dragging: false), .switched)
        XCTAssertEqual(modes.mode, .area)
        XCTAssertEqual(modes.key("q", dragging: false), .nothing)
        XCTAssertEqual(modes.mode, .area)
    }

    func testScrollModeKey() {
        XCTAssertTrue(OverlayMode.area.hint.contains("S scroll"))
        XCTAssertTrue(OverlayMode.scroll.hint.contains("scroll it yourself"))
        XCTAssertTrue(OverlayMode.scroll.drags)
        XCTAssertTrue(OverlayMode.scroll.dims)
        // How a scroll ended says what happened only when it wasn't the person ending it.
        XCTAssertNil(ScrollEnd.done.note)
        XCTAssertTrue(ScrollEnd.lost.note?.contains("jumped") == true)
        XCTAssertTrue(ScrollEnd.full.note?.contains("30,000") == true)
    }

    func testHintNamesTheMode() {
        XCTAssertTrue(OverlayMode.area.hint.contains("T text"))
        XCTAssertTrue(OverlayMode.text.hint.hasPrefix("Drag over text"))
        XCTAssertTrue(OverlayMode.colour.hint.contains("hex"))
        XCTAssertTrue(OverlayMode.ruler.hint.hasPrefix("Hover to measure"))
        for mode in OverlayMode.allCases {
            XCTAssertTrue(mode.hint.contains("Esc"), "\(mode) says how to leave")
        }
        // Colours and measuring need the screen's true colours.
        XCTAssertFalse(OverlayMode.colour.dims)
        XCTAssertFalse(OverlayMode.ruler.dims)
        XCTAssertTrue(OverlayMode.text.dims)
    }

    func testNothingIsSavedOrListed() async throws {
        // Reading takes a clipboard and nothing else: no disk, no recent list, no shot.
        let board = TextBoard()
        let outcome = await Reading.run(try plain(10, 10), clipboard: board, readText: { _ in ["สวัสดี", "two"] },
                                        readCodes: { _ in [] })
        XCTAssertEqual(board.text, "สวัสดี\ntwo")
        XCTAssertEqual(outcome.copied, "สวัสดี\ntwo")
        XCTAssertEqual(outcome.note, "Copied: สวัสดี (+1 more line)")
        XCTAssertFalse(outcome.problem)
    }

    func testReadingSaysWhatItFound() {
        let board = TextBoard()
        XCTAssertEqual(Reading.outcome(lines: [], codes: [], clipboard: board).note, "No text found")
        XCTAssertNil(board.text, "nothing found, nothing copied")
        let code = Reading.outcome(lines: [], codes: ["https://x.com"], clipboard: board)
        XCTAssertEqual(code.note, "Found a QR code")
        XCTAssertEqual(code.codes, ["https://x.com"])
        let long = Reading.outcome(lines: [String(repeating: "ก", count: 80)], codes: [], clipboard: board)
        XCTAssertEqual(long.note.count, "Copied: ".count + 48)
        board.works = false
        let failed = Reading.outcome(lines: ["a"], codes: [], clipboard: board)
        XCTAssertTrue(failed.problem)
        XCTAssertNil(failed.copied)
    }
}
