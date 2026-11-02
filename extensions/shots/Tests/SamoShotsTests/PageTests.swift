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
        // Edit is the fourth, the most a row may carry.
        XCTAssertEqual(row.actions.map(\.id), ["copy", "reveal", "pin", "edit"])
        XCTAssertTrue(row.detail.contains("MB"), row.detail)
        XCTAssertTrue(page.contains(.section(title: "RECENT", about: "1 today")))
        XCTAssertEqual(Page.badge(recent, now: now), .badge(count: 0, summary: "Shots · 1 today"))
        XCTAssertEqual(Page.badge(Recent()), .badge(count: 0, summary: "Shots"))
    }

    func testChoiceUpdatesSettings() {
        var s = Settings()
        XCTAssertTrue(s.apply(choice: "after", "save"))
        XCTAssertEqual(s.after, .save)
        XCTAssertTrue(s.apply(choice: "format", "jpeg"))
        XCTAssertEqual(s.format, .jpeg)
        XCTAssertFalse(s.apply(choice: "after", "explode"))
        XCTAssertFalse(s.apply(choice: "colour", "red"))
        XCTAssertFalse(s.apply(choice: "after", "pin"), "pin is a Then now")
        XCTAssertEqual(s.after, .save, "a bad value changes nothing")
        XCTAssertTrue(s.apply(setting: "sound", true))
        XCTAssertFalse(s.apply(setting: "other", true))
        let page = Page.build(PageState(access: .allowed, shared: false, settings: s, recent: Recent()))
        XCTAssertTrue(page.contains { widget in
            if case let .choice(key, _, _, selected) = widget { return key == "after" && selected == "save" }
            return false
        })
        // A folder in iCloud Drive is called out.
        let home = URL(fileURLWithPath: "/Users/kj")
        XCTAssertTrue(Page.inICloud(home.appendingPathComponent("Library/Mobile Documents/x"), home: home, desktopSynced: false))
        XCTAssertTrue(Page.inICloud(home.appendingPathComponent("Desktop/Shots"), home: home, desktopSynced: true))
        XCTAssertFalse(Page.inICloud(home.appendingPathComponent("Pictures/Shots"), home: home, desktopSynced: true))
    }

    func testThenChoiceUpdatesSettings() {
        var s = Settings()
        let choices = Page.build(PageState(access: .allowed, shared: false, settings: s, recent: Recent()))
            .compactMap { widget -> (String, String, [String])? in
                if case let .choice(key, label, options, _) = widget { return (key, label, options.map(\.id)) }
                return nil
            }
        XCTAssertTrue(choices.contains { $0.0 == "after" && $0.1 == "First" && $0.2 == ["copy", "save", "both"] })
        XCTAssertTrue(choices.contains { $0.0 == "then" && $0.1 == "Then" && $0.2 == ["edit", "pin", "nothing"] })
        XCTAssertTrue(s.apply(choice: "then", "pin"))
        XCTAssertEqual(s.then, .pin)
        XCTAssertFalse(s.apply(choice: "then", "both"))
        XCTAssertEqual(s.then, .pin)
        let page = Page.build(PageState(access: .allowed, shared: false, settings: s, recent: Recent()))
        XCTAssertTrue(page.contains { widget in
            if case let .choice(key, _, _, selected) = widget { return key == "then" && selected == "pin" }
            return false
        })
    }

    func testPageHasReadAndMeasureButtons() {
        let page = Page.build(PageState(access: .allowed, shared: false, settings: Settings(), recent: Recent()))
        let actions = page.compactMap { widget -> String? in
            if case let .button(action, _, _) = widget { return action }
            return nil
        }
        for action in ["area", "window", "screen", "scroll", "text", "colour", "ruler"] {
            XCTAssertTrue(actions.contains(action), action)
        }
        // Every mode has a shortcut row; the new ones say they have none.
        let rows = page.compactMap { widget -> [RowItem]? in
            if case let .rows(items) = widget, items.first?.id == nil { return items }
            return nil
        }.first ?? []
        XCTAssertEqual(rows.map(\.title), ["Whole screen", "An area", "Scrolling capture", "Copy text", "Pick a colour",
                                           "Measure"])
        XCTAssertEqual(rows[2].detail, "No shortcut")
        XCTAssertEqual(rows[2].lamp, .off)
    }

    func testRecorderRefusesTypingKeys() {
        // ⇧2 types "@": refused. ⇧⌘2 is fine. Cocoa flags: shift 0x20000, command 0x100000.
        XCTAssertNil(Shortcut.recorded(keyCode: 0x13, cocoaModifiers: 0x20000))
        XCTAssertNil(Shortcut.recorded(keyCode: 0x13, cocoaModifiers: 0))
        XCTAssertEqual(Shortcut.recorded(keyCode: 0x13, cocoaModifiers: 0x120000), Shortcut.area)
        XCTAssertEqual(Shortcut.recorded(keyCode: 0x13, cocoaModifiers: 0x80000)?.display, "⌥2")
    }
}
