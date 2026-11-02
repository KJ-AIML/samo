import XCTest
@testable import ClipsCore

final class HistoryTests: XCTestCase {
    private func text(_ s: String) -> Capture { Capture(kind: .text, text: s) }

    func testRepeatsMoveToTheTopAndCount() {
        var history = History()
        let t0 = Date(timeIntervalSince1970: 1_000)
        let a = history.add(text("alpha"), at: t0)
        _ = history.add(text("beta"), at: t0.addingTimeInterval(1))
        let again = history.add(text("alpha"), at: t0.addingTimeInterval(2))
        XCTAssertFalse(again.isNew)
        XCTAssertEqual(again.item.id, a.item.id)
        XCTAssertEqual(history.items.map(\.text), ["alpha", "beta"])
        XCTAssertEqual(history.items[0].copies, 2)
        XCTAssertEqual(history.items[0].first, t0)
        XCTAssertEqual(history.items[0].last, t0.addingTimeInterval(2))
    }

    func testHistoryKeepsTheLimitButNotPins() {
        var history = History()
        history.limit = 3
        let pinned = history.add(text("pinned"), at: Date())
        history.pin(pinned.item.id, true)
        var dropped: [Item] = []
        for i in 0..<5 {
            dropped += history.add(text("t\(i)"), at: Date()).dropped
        }
        XCTAssertEqual(history.recent.map(\.text), ["t4", "t3", "t2"])
        XCTAssertEqual(history.pinned.map(\.text), ["pinned"], "pins outlive the limit")
        XCTAssertEqual(dropped.map(\.text), ["t0", "t1"])
        // Pins come first; a repeat of a pin keeps its place.
        XCTAssertEqual(history.items.first?.text, "pinned")
        _ = history.add(text("pinned"), at: Date())
        XCTAssertEqual(history.items.map(\.text), ["pinned", "t4", "t3", "t2"])
        // Unpinned, it goes to the top of the rest; clear keeps pins.
        let second = history.add(text("second pin"), at: Date())
        history.pin(second.item.id, true)
        XCTAssertEqual(history.pinned.map(\.text), ["pinned", "second pin"])
        history.pin(pinned.item.id, false)
        XCTAssertEqual(history.items.map(\.text).prefix(2), ["second pin", "pinned"])
        // (Adding "second pin" pushed t2 out under the limit of 3.)
        XCTAssertEqual(history.clear().map(\.text), ["pinned", "t4", "t3"])
        XCTAssertEqual(history.items.map(\.text), ["second pin"])
        XCTAssertNotNil(history.delete(second.item.id))
        XCTAssertTrue(history.items.isEmpty)
    }

    func testStoreRoundTripsEveryKind() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("clips-\(UUID().uuidString)")
        addTeardownBlock { try? FileManager.default.removeItem(at: dir) }
        let store = Store(dir: dir)
        var history = History()
        let long = String(repeating: "ก", count: 40_000)  // 120 KB in UTF-8: kept whole on disk
        let captures = [
            Capture(kind: .text, text: "hello", source: "com.apple.TextEdit"),
            Capture(kind: .rich, text: "bold", data: Data("{\\rtf1 \\b bold}".utf8), dataType: ClipType.rtf),
            Capture(kind: .image, text: "", data: Data([0x89, 0x50, 0x4E, 0x47]), dataType: ClipType.png),
            Capture(kind: .files, text: "/tmp/a.pdf", files: ["/tmp/a.pdf"]),
            Capture(kind: .text, text: long),
        ]
        for capture in captures {
            let added = history.add(capture, at: Date(timeIntervalSince1970: 1_793_543_712))
            try store.keep(capture, as: added, in: &history)
        }
        try store.save(history)
        let back = Store(dir: dir).load()
        XCTAssertEqual(back, history)
        let image = try XCTUnwrap(back.items.first { $0.kind == .image })
        XCTAssertEqual(image.dataFile.flatMap(store.read), Data([0x89, 0x50, 0x4E, 0x47]))
        XCTAssertTrue(image.dataFile?.hasSuffix(".png") == true)
        let longItem = try XCTUnwrap(back.items.first { $0.textFile != nil })
        XCTAssertEqual(store.fullText(longItem), long)
        XCTAssertLessThanOrEqual(longItem.text.utf8.count, History.inlineText)
        XCTAssertFalse(longItem.text.contains("\u{FFFD}"), "cut between characters")
        XCTAssertTrue(long.hasPrefix(longItem.text))
        // Deleting an item deletes its files.
        store.delete(image)
        XCTAssertNil(image.dataFile.flatMap(store.read))
        // Nothing on disk yet: an empty history.
        XCTAssertEqual(Store(dir: dir.appendingPathComponent("none")).load(), History())
    }

    func testSearchFindsWordsInAnyOrder() {
        var history = History()
        for s in ["Café au lait recipe", "git push origin main", "ราคาสินค้า 1,250 บาท"] {
            _ = history.add(Capture(kind: .text, text: s), at: Date())
        }
        _ = history.add(Capture(kind: .files, text: "/Users/kj/Desktop/Invoice-2026.pdf",
                                files: ["/Users/kj/Desktop/Invoice-2026.pdf"]), at: Date())
        let titles = { (q: String) in Search.filter(history.items, q).map(\.title) }
        XCTAssertEqual(titles("main git"), ["git push origin main"])
        XCTAssertEqual(titles("CAFE"), ["Café au lait recipe"], "case and accents don't matter")
        XCTAssertEqual(titles("บาท"), ["ราคาสินค้า 1,250 บาท"])
        XCTAssertEqual(titles("invoice"), ["Invoice-2026.pdf"])
        XCTAssertEqual(titles("nothing like this"), [])
        XCTAssertEqual(titles("  ").count, 4, "no words, everything")
    }
}
