import XCTest
@testable import ShotsCore

final class PageTests: XCTestCase {
    private func texts(_ page: [Widget]) -> [String] {
        page.compactMap {
            switch $0 {
            case let .note(text, _): return text
            case let .button(action, _, _): return "button:\(action)"
            default: return nil
            }
        }
    }

    func testPageShowsPermissionState() {
        let base = PageState(access: .notYet, shared: false, settings: Settings(), recent: Recent())
        XCTAssertTrue(texts(Page.build(base)).contains("button:allow"))
        var asked = base
        asked.access = .askedRestart
        let after = texts(Page.build(asked))
        XCTAssertTrue(after.contains("button:open-settings") && after.contains("button:restart"))
        var allowed = base
        allowed.access = .allowed
        XCTAssertFalse(texts(Page.build(allowed)).contains("button:allow"))
        // Shared with Samo is said plainly, as an error.
        var shared = allowed
        shared.shared = true
        XCTAssertTrue(Page.build(shared).contains { widget in
            if case let .note(text, .error) = widget { return text.contains("shared with Samo") }
            return false
        })
    }

    func testRecentRowsHaveActions() throws {
        var recent = Recent()
        let now = Date()
        recent.add(RecentShot(path: "/Users/kj/Pictures/Shots/Shot a.png", bytes: 2_100_000, at: now))
        let page = Page.build(PageState(access: .allowed, shared: false, settings: Settings(), recent: recent, now: now))
        let row = try XCTUnwrap(page.lazy.compactMap { widget -> RowItem? in
            if case let .rows(items) = widget, items.first?.id != nil { return items.first }
            return nil
        }.first)
        XCTAssertEqual(row.id, "/Users/kj/Pictures/Shots/Shot a.png")
        XCTAssertEqual(row.title, "Shot a.png")
        XCTAssertEqual(row.actions.map(\.id), ["copy", "reveal", "pin"])
        XCTAssertTrue(row.detail.contains("MB"), row.detail)
        XCTAssertTrue(page.contains(.section(title: "RECENT", about: "1 today")))
        XCTAssertEqual(Page.badge(recent, now: now), .badge(count: 0, summary: "Shots · 1 today"))
        XCTAssertEqual(Page.badge(Recent()), .badge(count: 0, summary: "Shots"))
    }

    func testChoiceUpdatesSettings() {
        var s = Settings()
        XCTAssertTrue(s.apply(choice: "after", "pin"))
        XCTAssertEqual(s.after, .pin)
        XCTAssertTrue(s.apply(choice: "format", "jpeg"))
        XCTAssertEqual(s.format, .jpeg)
        XCTAssertFalse(s.apply(choice: "after", "explode"))
        XCTAssertFalse(s.apply(choice: "colour", "red"))
        XCTAssertEqual(s.after, .pin, "a bad value changes nothing")
        XCTAssertTrue(s.apply(setting: "sound", true))
        XCTAssertFalse(s.apply(setting: "other", true))
        let page = Page.build(PageState(access: .allowed, shared: false, settings: s, recent: Recent()))
        XCTAssertTrue(page.contains { widget in
            if case let .choice(key, _, _, selected) = widget { return key == "after" && selected == "pin" }
            return false
        })
        // A folder in iCloud Drive is called out.
        let home = URL(fileURLWithPath: "/Users/kj")
        XCTAssertTrue(Page.inICloud(home.appendingPathComponent("Library/Mobile Documents/x"), home: home, desktopSynced: false))
        XCTAssertTrue(Page.inICloud(home.appendingPathComponent("Desktop/Shots"), home: home, desktopSynced: true))
        XCTAssertFalse(Page.inICloud(home.appendingPathComponent("Pictures/Shots"), home: home, desktopSynced: true))
    }

    func testRecorderRefusesTypingKeys() {
        // ⇧2 types "@": refused. ⇧⌘2 is fine. Cocoa flags: shift 0x20000, command 0x100000.
        XCTAssertNil(Shortcut.recorded(keyCode: 0x13, cocoaModifiers: 0x20000))
        XCTAssertNil(Shortcut.recorded(keyCode: 0x13, cocoaModifiers: 0))
        XCTAssertEqual(Shortcut.recorded(keyCode: 0x13, cocoaModifiers: 0x120000), Shortcut.area)
        XCTAssertEqual(Shortcut.recorded(keyCode: 0x13, cocoaModifiers: 0x80000)?.display, "⌥2")
    }
}
