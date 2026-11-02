// The list, driven headless: never shown on screen.

import AppKit
import XCTest
@testable import ClipsCore
@testable import ClipsUI

final class ListTests: XCTestCase {
    private var history = History()
    private var picked: [(String, Bool)] = []
    private var closes: [Bool] = []

    private func makeList(_ texts: [String]) -> ListWindow {
        history = History()
        for (i, text) in texts.reversed().enumerated() {
            _ = history.add(Capture(kind: .text, text: text), at: Date(timeIntervalSince1970: Double(i)))
        }
        let list = ListWindow(hooks: ListHooks(
            items: { [unowned self] in self.history.items },
            thumbnail: { _ in nil },
            pick: { [unowned self] item, plain in self.picked.append((item.text, plain)) },
            pin: { [unowned self] item in self.history.pin(item.id, !item.pinned) },
            delete: { [unowned self] item in self.history.delete(item.id) },
            closed: { [unowned self] picked in self.closes.append(picked) }
        ))
        list.prepare()
        return list
    }

    private func key(_ code: UInt16, _ characters: String, _ flags: NSEvent.ModifierFlags = []) throws -> NSEvent {
        try XCTUnwrap(NSEvent.keyEvent(
            with: .keyDown, location: .zero, modifierFlags: flags, timestamp: 0, windowNumber: 0, context: nil,
            characters: characters, charactersIgnoringModifiers: characters, isARepeat: false, keyCode: code
        ))
    }

    func testTypingFilters() {
        let list = makeList(["git push origin main", "Café menu", "git status"])
        XCTAssertEqual(list.shown.map(\.text), ["git push origin main", "Café menu", "git status"])
        list.search("git")
        XCTAssertEqual(list.shown.map(\.text), ["git push origin main", "git status"])
        list.search("main git")
        XCTAssertEqual(list.shown.map(\.text), ["git push origin main"])
        list.search("nothing")
        XCTAssertTrue(list.shown.isEmpty)
    }

    func testReturnPicksTheSelected() throws {
        let list = makeList(["one", "two", "three"])
        XCTAssertTrue(list.handle(try key(125, "")))  // ↓
        XCTAssertEqual(list.selected, 1)
        XCTAssertTrue(list.handle(try key(36, "\r")))
        XCTAssertEqual(picked.map(\.0), ["two"])
        XCTAssertEqual(picked.first?.1, false)
        XCTAssertEqual(closes, [true])
        XCTAssertFalse(list.isOpen)
        // ⌥Return: without formatting.
        let again = makeList(["rich"])
        XCTAssertTrue(again.handle(try key(36, "\r", .option)))
        XCTAssertEqual(picked.last?.1, true)
        // Arrows stop at the ends.
        let ends = makeList(["a", "b"])
        _ = ends.handle(try key(126, ""))
        XCTAssertEqual(ends.selected, 0)
        for _ in 0..<5 { _ = ends.handle(try key(125, "")) }
        XCTAssertEqual(ends.selected, 1)
    }

    func testCommandNumberPicks() throws {
        let list = makeList(["one", "two", "three"])
        XCTAssertTrue(list.handle(try key(20, "3", .command)))
        XCTAssertEqual(picked.map(\.0), ["three"])
        // A number with no row picks nothing; typing goes to the field.
        let short = makeList(["only"])
        XCTAssertTrue(short.handle(try key(25, "9", .command)))
        XCTAssertEqual(picked.count, 1)
        XCTAssertFalse(short.handle(try key(0, "a")))
    }

    func testPinMovesToTheTop() throws {
        let list = makeList(["one", "two", "three"])
        _ = list.handle(try key(125, ""))
        _ = list.handle(try key(125, ""))
        XCTAssertTrue(list.handle(try key(35, "π", .option)))  // ⌥P
        XCTAssertEqual(list.shown.map(\.text), ["three", "one", "two"])
        XCTAssertTrue(list.shown[0].pinned)
        // ⌥⌫ deletes the selected.
        list.search("")
        XCTAssertTrue(list.handle(try key(51, "", .option)))
        XCTAssertEqual(list.shown.map(\.text), ["one", "two"])
    }

    func testEscClosesWithoutPasting() throws {
        let list = makeList(["one"])
        XCTAssertTrue(list.handle(try key(53, "\u{1b}")))
        XCTAssertTrue(picked.isEmpty)
        XCTAssertEqual(closes, [false])
        XCTAssertFalse(list.isOpen)
    }
}
