import AppKit
import XCTest
@testable import ShotsCore

final class FontTests: XCTestCase {
    func testFontsLoadFromShareOrFallBack() throws {
        let package = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let dir = Fonts.directory(forExecutable: package.appendingPathComponent("bin/samo-shots"))
        XCTAssertEqual(dir.lastPathComponent, "fonts")
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
