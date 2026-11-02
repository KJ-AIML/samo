// The result card, driven headless: never shown on screen.

import AppKit
import XCTest
@testable import ShotsCore
@testable import ShotsUI

private final class TextClipboard: Clipboard {
    var text: String?
    func put(png: Data) -> Bool { false }
    func put(text: String) -> Bool {
        self.text = text
        return true
    }
}

final class ResultCardTests: XCTestCase {
    func testCardOffersOpenOnlyForWebLinks() {
        let web = ResultCard(.code("https://github.com/KJ-AIML/samo"), clipboard: TextClipboard())
        XCTAssertNotNil(web.openButton)
        XCTAssertEqual(web.content.link?.host, "github.com")
        for code in ["WIFI:S:Talvyn;T:WPA;P:secret;;", "javascript:alert(1)", "file:///etc/hosts", "Just text"] {
            let card = ResultCard(.code(code), clipboard: TextClipboard())
            XCTAssertNil(card.openButton, code)
            XCTAssertNil(card.content.link, code)
        }
    }

    func testCardCopiesTheText() {
        let clipboard = TextClipboard()
        var closed = 0
        let card = ResultCard(.code("WIFI:S:Talvyn;;"), clipboard: clipboard, note: { _, _ in }) { closed += 1 }
        card.copyButton.performClick(nil)
        XCTAssertEqual(clipboard.text, "WIFI:S:Talvyn;;")
        XCTAssertTrue(card.isClosed)
        XCTAssertEqual(closed, 1)
        // Closing twice is once.
        card.close()
        XCTAssertEqual(closed, 1)
    }

    func testCardLeavesOnEsc() throws {
        let card = ResultCard(.code("hello"), clipboard: TextClipboard())
        let esc = try XCTUnwrap(NSEvent.keyEvent(
            with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0, windowNumber: 0, context: nil,
            characters: "\u{1b}", charactersIgnoringModifiers: "\u{1b}", isARepeat: false, keyCode: 53
        ))
        card.panel.keyDown(with: esc)
        XCTAssertTrue(card.isClosed)
    }
}
