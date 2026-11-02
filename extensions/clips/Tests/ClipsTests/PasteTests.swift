import XCTest
@testable import ClipsCore

/// A Mac for the paste flow: who is in front, and what was pressed.
private final class FakeMac {
    var front: Int32? = 900
    var trusted = true
    var activated: [Int32] = []
    var pastes = 0
    /// The app comes back after this many looks.
    var comesBackAfter = 3
    private var looks = 0

    lazy var mac = PasteFlow.Mac(
        front: { [unowned self] in
            self.looks += 1
            if let target = self.activated.last, self.looks > self.comesBackAfter { return target }
            return self.front
        },
        activate: { [unowned self] pid in self.activated.append(pid) },
        trusted: { [unowned self] in self.trusted },
        pressPaste: { [unowned self] in self.pastes += 1 },
        after: { _, block in block() }
    )
}

final class PasteTests: XCTestCase {
    func testPasteOnlyFollowsAPick() {
        let fake = FakeMac()
        let flow = PasteFlow(mac: fake.mac)
        // Opening and closing the list presses nothing.
        flow.remember(42)
        flow.cancelled()
        XCTAssertEqual(fake.pastes, 0)
        var outcomes: [PasteFlow.Outcome] = []
        // A pick with nothing remembered (the list closed) only copies.
        flow.picked { outcomes.append($0) }
        XCTAssertEqual(fake.pastes, 0)
        XCTAssertEqual(outcomes, [.copiedOnly("Copied: press ⌘V")])
        // A pick after opening pastes, once, into the app that was in front.
        flow.remember(42)
        flow.picked { outcomes.append($0) }
        XCTAssertEqual(fake.activated, [42])
        XCTAssertEqual(fake.pastes, 1)
        XCTAssertEqual(outcomes.last, .pasted)
        // A second pick without opening the list again doesn't paste again.
        flow.picked { outcomes.append($0) }
        XCTAssertEqual(fake.pastes, 1)
    }

    func testPasteWaitsForTheRightApp() {
        let fake = FakeMac()
        fake.comesBackAfter = 10
        let flow = PasteFlow(mac: fake.mac)
        flow.remember(42)
        var outcome: PasteFlow.Outcome?
        flow.picked { outcome = $0 }
        // It waited until 42 was in front, then pressed ⌘V.
        XCTAssertEqual(outcome, .pasted)
        XCTAssertEqual(fake.pastes, 1)
        // An app that never comes back gets nothing pressed: the item is on the clipboard.
        let gone = FakeMac()
        gone.comesBackAfter = 10_000
        let flow2 = PasteFlow(mac: gone.mac)
        flow2.remember(42)
        flow2.picked { outcome = $0 }
        XCTAssertEqual(outcome, .copiedOnly("Copied: press ⌘V"))
        XCTAssertEqual(gone.pastes, 0)
        // Without Accessibility: nothing activated, nothing pressed, and it says how to allow it.
        let untrusted = FakeMac()
        untrusted.trusted = false
        let flow3 = PasteFlow(mac: untrusted.mac)
        flow3.remember(42)
        flow3.picked { outcome = $0 }
        XCTAssertTrue(untrusted.activated.isEmpty)
        XCTAssertEqual(untrusted.pastes, 0)
        if case let .copiedOnly(words)? = outcome {
            XCTAssertTrue(words.contains("Accessibility"), words)
        } else {
            XCTFail("\(String(describing: outcome))")
        }
    }
}
