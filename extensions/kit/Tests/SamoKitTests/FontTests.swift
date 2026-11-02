import AppKit
import XCTest
@testable import SamoKit

final class FontTests: XCTestCase {
    func testFontsLoadFromShareOrFallBack() throws {
        // A helper at `<package>/bin/<helper>` finds `<package>/share/fonts`.
        let packaged = Fonts.directory(forExecutable: URL(fileURLWithPath: "/x/shots/bin/samo-shots"))
        XCTAssertEqual(packaged.path, "/x/shots/share/fonts")
        // Each extension ships a copy of Samo's own fonts; register those.
        let repo = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
        let dir = repo.appendingPathComponent("apps/samo-gpui/assets/fonts")
        XCTAssertTrue(FileManager.default.fileExists(atPath: dir.path), dir.path)
        // Missing fonts register nothing, and every face still comes back.
        XCTAssertEqual(Fonts.register(directory: URL(fileURLWithPath: "/nonexistent")), 0)
        XCTAssertEqual(Fonts.register(directory: dir), 2)
        XCTAssertEqual(Fonts.mono(12).fontName, "IBMPlexMono-Regular")
        XCTAssertEqual(Fonts.mono(12, weight: .medium).fontName, "IBMPlexMono-Medium")
        XCTAssertGreaterThan(Fonts.body(14).pointSize, 0)
        XCTAssertGreaterThan(Fonts.display(17).pointSize, 0)
        // The palette's two themes differ where they must.
        XCTAssertNotEqual(Palette.light.card, Palette.dark.card)
        XCTAssertEqual(Palette.light.glassText, Palette.dark.glassText)
    }
}
