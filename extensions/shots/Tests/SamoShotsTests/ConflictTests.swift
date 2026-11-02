import XCTest
import SamoKit
@testable import ShotsCore

final class ConflictTests: XCTestCase {
    func testKnownScreenshotAppIsReported() {
        let running = ["com.apple.finder", "cc.ffitch.shottr", "com.TechSmith.Snagit2024"]
        XCTAssertEqual(Conflicts.runningScreenshotApps(bundleIds: running), ["Shottr", "Snagit"])
        XCTAssertEqual(Conflicts.runningScreenshotApps(bundleIds: ["com.apple.finder"]), [])

        let ours = Settings().shortcuts
        let clashes = Conflicts.clashes(ours: ours, macOS: [], apps: ["Shottr"])
        XCTAssertEqual(clashes["area"], "May clash with Shottr, which is running")
        XCTAssertEqual(Conflicts.clashes(ours: ours, macOS: [], apps: []), [:])
        // macOS's own shortcut on ⇧⌘2 is named as that.
        let clash = Conflicts.clashes(ours: ours, macOS: [Shortcut.area], apps: [])
        XCTAssertEqual(clash["area"], "⇧⌘2 is also macOS's screenshot shortcut")
    }

    func testMacScreenshotShortcutsAreRead() {
        // No settings: macOS's defaults, all on: ⇧⌘3, ⌃⇧⌘3, ⇧⌘4, ⌃⇧⌘4, ⇧⌘5.
        XCTAssertEqual(
            Conflicts.macScreenshotShortcuts(symbolicHotKeys: nil).map(\.display).sorted(),
            ["⇧⌘3", "⇧⌘4", "⇧⌘5", "⌃⇧⌘3", "⌃⇧⌘4"]
        )
        // One moved to ⇧⌘2 (keycode 19, shift+command), one turned off.
        let moved: [String: Any] = [
            "28": ["enabled": true, "value": ["parameters": [50, 19, 1_179_648], "type": "standard"]],
            "184": ["enabled": false],
        ]
        let found = Conflicts.macScreenshotShortcuts(symbolicHotKeys: moved)
        XCTAssertTrue(found.contains(Shortcut.area))
        XCTAssertFalse(found.map(\.display).contains("⇧⌘5"))
    }

    func testPermissionNeedsARestartOnceAsked() {
        var granted = false
        let permission = Permission(preflight: { granted }, ask: { false })
        XCTAssertEqual(permission.state, .notYet)
        permission.request()
        XCTAssertEqual(permission.state, .askedRestart)
        granted = true
        XCTAssertEqual(permission.state, .allowed)
    }
}
