import XCTest
@testable import ShotsCore

final class SettingsTests: XCTestCase {
    private func temporaryDirectory() throws -> URL {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("shots-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: dir) }
        return dir
    }

    func testSettingsRoundTrip() throws {
        let dir = try temporaryDirectory()
        // Nothing saved yet: the defaults.
        let fresh = Settings.load(dataDir: dir)
        XCTAssertEqual(fresh, Settings())
        XCTAssertEqual(fresh.after, .both)
        XCTAssertEqual(fresh.shortcuts["area"]?.display, "⇧⌘2")
        XCTAssertEqual(fresh.shortcuts["screen"]?.display, "⇧⌘1")
        XCTAssertTrue(fresh.folderURL.path.hasSuffix("/Pictures/Shots"))

        var changed = fresh
        changed.after = .pin
        changed.format = .jpeg
        changed.sound = true
        changed.shortcuts["area"] = Shortcut(keyCode: 0x13, modifiers: Shortcut.control | Shortcut.command | Shortcut.shift)
        try changed.save(dataDir: dir)
        XCTAssertEqual(Settings.load(dataDir: dir), changed)
        XCTAssertEqual(changed.shortcuts["area"]?.display, "⌃⇧⌘2")

        // A damaged file reads as the defaults, never as a crash.
        try Data("{not json".utf8).write(to: dir.appendingPathComponent("settings.json"))
        XCTAssertEqual(Settings.load(dataDir: dir), Settings())
        // A shortcut without ⌘, ⌃ or ⌥ would take a key you type.
        XCTAssertFalse(Shortcut(keyCode: 0x13, modifiers: Shortcut.shift).canBeGlobal)
        XCTAssertTrue(Shortcut.area.canBeGlobal)
    }

    func testRecentKeepsTwenty() throws {
        var recent = Recent()
        let start = Date(timeIntervalSince1970: 1_000_000)
        for i in 0..<25 {
            recent.add(RecentShot(path: "/shots/\(i).png", bytes: i, at: start.addingTimeInterval(Double(i))))
        }
        XCTAssertEqual(recent.shots.count, Recent.keep)
        XCTAssertEqual(recent.shots.first?.path, "/shots/24.png", "newest first")
        XCTAssertEqual(recent.shots.last?.path, "/shots/5.png")
        // Adding a path again moves it to the front, once.
        recent.add(RecentShot(path: "/shots/10.png", bytes: 10, at: start.addingTimeInterval(100)))
        XCTAssertEqual(recent.shots.first?.path, "/shots/10.png")
        XCTAssertEqual(recent.shots.filter { $0.path == "/shots/10.png" }.count, 1)
        XCTAssertEqual(recent.count(since: start.addingTimeInterval(20)), 6, "20-24, and 10 taken again")
        // Files that are gone are forgotten.
        recent.forgetMissing { $0 != "/shots/24.png" }
        XCTAssertFalse(recent.shots.contains { $0.path == "/shots/24.png" })

        let dir = try temporaryDirectory()
        try recent.save(dataDir: dir)
        XCTAssertEqual(Recent.load(dataDir: dir), recent)
    }
}
