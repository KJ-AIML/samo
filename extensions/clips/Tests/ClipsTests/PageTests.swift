import XCTest
@testable import ClipsCore
@testable import SamoKit

final class PageTests: XCTestCase {
    func testHelloComesFirst() {
        let out = MemoryOutput()
        let session = Session(out: out, shared: true, sharedNote: ClipsPage.sharedNote,
                              handlers: Session.Handlers(page: { ClipsPage.starting() }))
        session.start()
        guard out.sent.count == 3, case .hello = out.sent[0], case let .log(event, detail) = out.sent[1],
              case let .view(page) = out.sent[2]
        else { return XCTFail("\(out.sent)") }
        XCTAssertEqual(event, "permission")
        XCTAssertTrue(detail.contains("Paste from Other Apps") && detail.contains("Accessibility"), detail)
        XCTAssertEqual(page.first, .section(title: "CLIPBOARD", about: nil))
    }
}

final class ClipsPageTests: XCTestCase {
    private func history(_ texts: [String]) -> History {
        var h = History()
        for (i, t) in texts.reversed().enumerated() {
            _ = h.add(Capture(kind: .text, text: t, source: "com.apple.Safari"), at: Date(timeIntervalSince1970: Double(i)))
        }
        return h
    }

    private func rows(_ page: [Widget], after title: String) -> [RowItem] {
        guard let i = page.firstIndex(where: { if case let .section(t, _) = $0 { return t == title }; return false }),
              i + 1 < page.count, case let .rows(items) = page[i + 1]
        else { return [] }
        return items
    }

    private func buttons(_ page: [Widget]) -> [String] {
        page.compactMap { if case let .button(action, _, _) = $0 { return action }; return nil }
    }

    func testPageShowsBothPermissions() {
        let waiting = ClipsPage.build(PageState(paste: .asks, accessibility: false, shared: false, history: History()))
        let perms = rows(waiting, after: "PERMISSIONS")
        XCTAssertEqual(perms.map(\.title), ["Paste from Other Apps", "Accessibility"])
        XCTAssertEqual(perms.map(\.lamp), [.needsYou, .needsYou])
        XCTAssertTrue(perms[0].detail.contains("Always Allow"))
        XCTAssertTrue(buttons(waiting).contains("open-paste-settings"))
        XCTAssertTrue(buttons(waiting).contains("allow-accessibility"))
        // Both allowed: no buttons to System Settings.
        let ready = ClipsPage.build(PageState(paste: .allowed, accessibility: true, shared: false, history: History()))
        XCTAssertEqual(rows(ready, after: "PERMISSIONS").map(\.lamp), [.done, .done])
        XCTAssertFalse(buttons(ready).contains("open-paste-settings"))
        XCTAssertFalse(buttons(ready).contains("allow-accessibility"))
        // Denied is an error; shared is said.
        let denied = ClipsPage.build(PageState(paste: .denied, accessibility: true, shared: true, history: History()))
        XCTAssertEqual(rows(denied, after: "PERMISSIONS").first?.lamp, .error)
        XCTAssertTrue(denied.contains(.note(text: ClipsPage.sharedNote, kind: .error)))
    }

    func testRowsHaveCopyPinDelete() throws {
        var h = history(["alpha", "beta"])
        let beta = try XCTUnwrap(h.items.first { $0.text == "beta" })
        h.pin(beta.id, true)
        let page = ClipsPage.build(PageState(paste: .allowed, accessibility: true, shared: false, history: h,
                                             appName: { $0 == "com.apple.Safari" ? "Safari" : nil },
                                             now: Date(timeIntervalSince1970: 1)))
        let pinned = rows(page, after: "PINNED")
        XCTAssertEqual(pinned.map(\.title), ["beta"])
        XCTAssertEqual(pinned.first?.actions.map(\.id), ["copy", "unpin", "delete"])
        let recent = rows(page, after: "RECENT")
        XCTAssertEqual(recent.map(\.title), ["alpha"])
        XCTAssertEqual(recent.first?.actions.map(\.id), ["copy", "pin", "delete"])
        XCTAssertEqual(recent.first?.id, h.items.first { $0.text == "alpha" }?.id.uuidString)
        XCTAssertTrue(recent.first?.detail.contains("Safari") == true, recent.first?.detail ?? "")
        // The latest 20 only.
        let many = history((0..<30).map { "item \($0)" })
        XCTAssertEqual(rows(ClipsPage.build(PageState(paste: .allowed, accessibility: true, shared: false,
                                                      history: many)), after: "RECENT").count, 20)
        XCTAssertEqual(ClipsPage.badge(many), .badge(count: 0, summary: "Clips · 30 items"))
        XCTAssertEqual(ClipsPage.badge(History()), .badge(count: 0, summary: "Clips"))
    }

    func testPauseStopsKeeping() {
        let now = Date(timeIntervalSince1970: 1_000)
        let paused = ClipsPage.build(PageState(paste: .allowed, accessibility: true, shared: false, history: History(),
                                               pausedUntil: now.addingTimeInterval(600), now: now))
        XCTAssertTrue(buttons(paused).contains("resume"))
        XCTAssertFalse(buttons(paused).contains("pause"))
        let running = ClipsPage.build(PageState(paste: .allowed, accessibility: true, shared: false, history: History(),
                                                pausedUntil: now.addingTimeInterval(-1), now: now))
        XCTAssertTrue(buttons(running).contains("pause"))
        // And the watcher keeps nothing meanwhile.
        let board = FakeBoard()
        let watcher = Watcher(pasteboard: board, frontmost: { nil })
        watcher.pausedUntil = now.addingTimeInterval(600)
        board.copyText("paused")
        XCTAssertNil(watcher.tick(now: now))
        // Clearing asks first, and pins stay.
        let asking = ClipsPage.build(PageState(paste: .allowed, accessibility: true, shared: false,
                                               history: history(["a", "b"]), confirmClear: true))
        XCTAssertTrue(buttons(asking).contains("clear-confirm"))
        XCTAssertTrue(buttons(asking).contains("clear-cancel"))
    }

    func testKnownClipboardAppsAreReported() {
        XCTAssertEqual(ClipsPage.runningClipboardApp(bundleIds: ["com.apple.Safari", "org.p0deje.Maccy"]), "Maccy")
        XCTAssertNil(ClipsPage.runningClipboardApp(bundleIds: ["com.apple.Safari"]))
        let page = ClipsPage.build(PageState(paste: .allowed, accessibility: true, shared: false, history: History(),
                                             clash: "Maccy"))
        let shortcut = rows(page, after: "SHORTCUT")
        XCTAssertEqual(shortcut.first?.lamp, .needsYou)
        XCTAssertEqual(shortcut.first?.detail, "May clash with Maccy, which is running")
        XCTAssertEqual(shortcut.first?.trailing, "⇧⌘C")
        let refused = ClipsPage.build(PageState(paste: .allowed, accessibility: true, shared: false, history: History(),
                                                refused: true))
        XCTAssertEqual(rows(refused, after: "SHORTCUT").first?.lamp, .error)
    }
}
