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
        changed.after = .save
        changed.then = .pin
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

    func testPhaseOneSettingsStillLoad() throws {
        let dir = try temporaryDirectory()
        // Shots 0.1.0's settings, with its "pin": pin without copying or saving.
        try Data(#"{"after":"pin","folder":"~/Desktop","format":"jpeg","retina":false,"sound":true,"shortcuts":{}}"#.utf8)
            .write(to: dir.appendingPathComponent("settings.json"))
        let loaded = Settings.load(dataDir: dir)
        XCTAssertEqual(loaded.after, .both)
        XCTAssertEqual(loaded.then, .pin)
        XCTAssertEqual(loaded.folder, "~/Desktop")
        XCTAssertEqual(loaded.format, .jpeg)
        XCTAssertFalse(loaded.retina)
        XCTAssertTrue(loaded.sound)
        XCTAssertTrue(loaded.shortcuts.isEmpty)
        // And the others keep their meaning, opening the editor after, as every shot now does.
        try Data(#"{"after":"copy"}"#.utf8).write(to: dir.appendingPathComponent("settings.json"))
        XCTAssertEqual(Settings.load(dataDir: dir).after, .copy)
        XCTAssertEqual(Settings.load(dataDir: dir).then, .edit)
    }

    func testOneBadValueKeepsTheRest() throws {
        let dir = try temporaryDirectory()
        try Data(#"{"after":"teleport","then":"dance","format":"webp","folder":"/Volumes/Shots","sound":true}"#.utf8)
            .write(to: dir.appendingPathComponent("settings.json"))
        let loaded = Settings.load(dataDir: dir)
        XCTAssertEqual(loaded.after, .both)
        XCTAssertEqual(loaded.then, .edit)
        XCTAssertEqual(loaded.format, .png)
        XCTAssertEqual(loaded.folder, "/Volumes/Shots", "what it knows is kept")
        XCTAssertTrue(loaded.sound)
        XCTAssertEqual(loaded.shortcuts["area"], .area)
        // What it saves, it loads.
        var changed = loaded
        changed.then = .nothing
        try changed.save(dataDir: dir)
        XCTAssertEqual(Settings.load(dataDir: dir), changed)
    }

    func testNewModesHaveNoShortcutByDefault() {
        let fresh = Settings()
        XCTAssertEqual(Set(fresh.shortcuts.keys), ["screen", "area"])
        for mode in ["scroll", "text", "colour", "ruler"] {
            XCTAssertNil(fresh.shortcuts[mode], mode)
            XCTAssertTrue(Settings.modes.contains { $0.key == mode }, mode)
        }
        // Letters are named, not numbered.
        XCTAssertEqual(Shortcut(keyCode: 17, modifiers: Shortcut.command | Shortcut.shift).display, "⇧⌘T")
        XCTAssertEqual(Shortcut(keyCode: 0x3A, modifiers: Shortcut.option).display, "⌥key 58")
    }

    func testSettingsFromPhase2StillLoad() throws {
        let dir = try temporaryDirectory()
        try Data(#"{"after":"both","then":"edit","folder":"~/Pictures/Shots","format":"png","retina":true,"sound":false,"shortcuts":{"screen":{"keyCode":18,"modifiers":768},"area":{"keyCode":19,"modifiers":768}}}"#.utf8)
            .write(to: dir.appendingPathComponent("settings.json"))
        let loaded = Settings.load(dataDir: dir)
        XCTAssertEqual(loaded.shortcuts["area"], .area)
        XCTAssertNil(loaded.shortcuts["text"])
        // A chosen one is kept.
        var chosen = loaded
        chosen.shortcuts["text"] = Shortcut(keyCode: 17, modifiers: Shortcut.control | Shortcut.shift)
        try chosen.save(dataDir: dir)
        XCTAssertEqual(Settings.load(dataDir: dir).shortcuts["text"]?.display, "⌃⇧T")
    }

    func testTildeIsHomeFromTheEnvironment() throws {
        XCTAssertEqual(Settings.expand("~/Pictures/Shots", home: "/tmp/h"), "/tmp/h/Pictures/Shots")
        XCTAssertEqual(Settings.expand("~", home: "/tmp/h"), "/tmp/h")
        XCTAssertEqual(Settings.expand("/Volumes/Shots", home: "/tmp/h"), "/Volumes/Shots")
        XCTAssertEqual(Settings.abbreviate("/tmp/h/Pictures", home: "/tmp/h"), "~/Pictures")
        // A sibling that merely starts the same isn't inside the home.
        XCTAssertEqual(Settings.abbreviate("/tmp/h2/Pictures", home: "/tmp/h"), "/tmp/h2/Pictures")

        // Samo runs helpers with their own `HOME`; the folder follows it, not the account's home.
        let before = ProcessInfo.processInfo.environment["HOME"]
        setenv("HOME", "/tmp/shots-home", 1)
        defer {
            if let before { setenv("HOME", before, 1) } else { unsetenv("HOME") }
        }
        XCTAssertEqual(Settings().folderURL.path, "/tmp/shots-home/Pictures/Shots")
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
