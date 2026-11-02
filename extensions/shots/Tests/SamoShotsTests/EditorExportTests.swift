import CoreGraphics
import ImageIO
import XCTest
@testable import ShotsCore

private final class Disk: ShotsCore.Disk {
    var files: [URL: Data] = [:]
    var written: [URL] = []
    var replaced: [URL] = []
    var fails = false
    func makeFolder(_ url: URL) throws {}
    func exists(_ url: URL) -> Bool { files[url] != nil }
    func write(_ data: Data, to url: URL) throws {
        if fails { throw CocoaError(.fileWriteNoPermission) }
        XCTAssertNil(files[url], "write never replaces a file")
        files[url] = data
        written.append(url)
    }
    func replace(_ data: Data, at url: URL) throws {
        if fails { throw CocoaError(.fileWriteNoPermission) }
        files[url] = data
        replaced.append(url)
    }
}

private final class Clipboard: ShotsCore.Clipboard {
    var png: Data?
    func put(png: Data) -> Bool {
        self.png = png
        return true
    }
    func put(text: String) -> Bool { false }
}

final class EditorExportTests: XCTestCase {
    private func edited() throws -> (Document, Shot) {
        let shot = Shot(image: try stripes(120, 80), scale: 2, takenAt: Date(timeIntervalSince1970: 1_793_543_712))
        let doc = Document(size: CGSize(width: 120, height: 80), scale: 2)
        doc.add(.rectangle, points: [CGPoint(x: 10, y: 10), CGPoint(x: 60, y: 50)], style: .standard)
        return (doc, shot)
    }

    func testSaveWritesTheShotsOwnFile() throws {
        let (doc, shot) = try edited()
        let disk = Disk()
        let own = URL(fileURLWithPath: "/tmp/Shots/Shot 2026-11-02 at 00.08.02.png")
        disk.files[own] = Data([1])
        shot.savedTo = own
        let result = Export.save(doc, shot: shot, redaction: Redaction(), settings: Settings(), disk: disk)
        XCTAssertNil(result.problem)
        XCTAssertEqual(result.url, own)
        XCTAssertEqual(disk.replaced, [own], "over the shot's own file")
        XCTAssertTrue(disk.written.isEmpty, "no near-duplicate beside it")
        XCTAssertFalse(doc.dirty)
        XCTAssertTrue(doc.matchesSaved)
        // It holds the marks, at Retina, with its density.
        let saved = try XCTUnwrap(disk.files[own])
        XCTAssertEqual(Encoding.size(of: saved)?.width, 120)
        XCTAssertNotEqual(saved, Encoding.data(shot.image, format: .png, scale: 2))

        // A JPEG shot stays JPEG.
        let jpeg = URL(fileURLWithPath: "/tmp/Shots/Shot.jpg")
        disk.files[jpeg] = Data([1])
        shot.savedTo = jpeg
        doc.add(.step, points: [CGPoint(x: 90, y: 40)], style: .standard)
        _ = Export.save(doc, shot: shot, redaction: Redaction(), settings: Settings(), disk: disk)
        let bytes = try XCTUnwrap(disk.files[jpeg])
        XCTAssertEqual(Array(bytes.prefix(2)), [0xFF, 0xD8], "a JPEG")
    }

    func testSaveOfANeverSavedShotNamesANewFile() throws {
        let (doc, shot) = try edited()
        let disk = Disk()
        var settings = Settings()
        settings.folder = "/tmp/Shots"
        // Its usual name is taken already.
        disk.files[URL(fileURLWithPath: "/tmp/Shots/" + FileNaming.name(at: shot.takenAt, format: .png))] = Data([1])
        let result = Export.save(doc, shot: shot, redaction: Redaction(), settings: settings, disk: disk)
        XCTAssertNil(result.problem)
        XCTAssertEqual(disk.written.count, 1)
        XCTAssertTrue(disk.replaced.isEmpty)
        XCTAssertTrue(result.url?.lastPathComponent.hasSuffix(" (2).png") == true, "\(String(describing: result.url))")
        XCTAssertEqual(shot.savedTo, result.url)
        XCTAssertGreaterThan(result.bytes, 0)
    }

    func testCopyAndSaveUseTheSameRender() throws {
        let (doc, shot) = try edited()
        doc.add(.blur, points: [CGPoint(x: 70, y: 10), CGPoint(x: 115, y: 70)], style: .standard)
        let clipboard = Clipboard()
        XCTAssertNil(Export.copy(doc, shot: shot, redaction: Redaction(), retina: true, clipboard: clipboard))
        XCTAssertFalse(doc.dirty, "copied counts")
        let disk = Disk()
        var settings = Settings()
        settings.folder = "/tmp/Shots"
        let saved = Export.save(doc, shot: shot, redaction: Redaction(), settings: settings, disk: disk)
        let file = try XCTUnwrap(saved.url.flatMap { disk.files[$0] })
        // The same pixels, redaction and all.
        let copied = try Pixels(XCTUnwrap(CGImageSourceCreateWithData(XCTUnwrap(clipboard.png) as CFData, nil)
            .flatMap { CGImageSourceCreateImageAtIndex($0, 0, nil) }))
        let written = try Pixels(XCTUnwrap(CGImageSourceCreateWithData(file as CFData, nil)
            .flatMap { CGImageSourceCreateImageAtIndex($0, 0, nil) }))
        XCTAssertTrue(copied.changed(from: written, tolerance: 0).isNull)
        // And the blur is in it.
        XCTAssertLessThan(written.spread(in: CGRect(x: 80, y: 20, width: 25, height: 40)), 0.05)
        // 1x halves both.
        XCTAssertNil(Export.copy(doc, shot: shot, redaction: Redaction(), retina: false, clipboard: clipboard))
        XCTAssertEqual(clipboard.png.flatMap(Encoding.size(of:))?.width, 60)
    }

    func testDragFileIsNamedAfterTheShot() throws {
        let (doc, shot) = try edited()
        let disk = Disk()
        let drags = URL(fileURLWithPath: "/tmp/data/drag")
        // Never saved: named for when it was taken.
        let fresh = try XCTUnwrap(Export.dragFile(doc, shot: shot, redaction: Redaction(), retina: true,
                                                  folder: drags, disk: disk))
        XCTAssertEqual(fresh.deletingLastPathComponent().path, drags.path)
        XCTAssertEqual(fresh.lastPathComponent, FileNaming.name(at: shot.takenAt, format: .png))
        // Saved, and edited since: the export, under the saved file's name.
        let own = URL(fileURLWithPath: "/tmp/Shots/Bug report.jpg")
        disk.files[own] = Data([1])
        shot.savedTo = own
        let edited = try XCTUnwrap(Export.dragFile(doc, shot: shot, redaction: Redaction(), retina: true,
                                                   folder: drags, disk: disk))
        XCTAssertEqual(edited, drags.appendingPathComponent("Bug report.png"))
        // Saved as it is: the saved file itself, not a copy.
        _ = Export.save(doc, shot: shot, redaction: Redaction(), settings: Settings(), disk: disk)
        XCTAssertEqual(Export.dragFile(doc, shot: shot, redaction: Redaction(), retina: true, folder: drags,
                                       disk: disk), own)
        // Copied isn't saved: the file doesn't have this mark, so the drag makes one that does.
        doc.add(.line, points: [.zero, CGPoint(x: 9, y: 9)], style: .standard)
        _ = Export.copy(doc, shot: shot, redaction: Redaction(), retina: true, clipboard: Clipboard())
        XCTAssertNotEqual(Export.dragFile(doc, shot: shot, redaction: Redaction(), retina: true, folder: drags,
                                          disk: disk), own)
    }

    func testDragFolderIsEmptiedAtStartAndStop() throws {
        let data = FileManager.default.temporaryDirectory.appendingPathComponent("shots-\(UUID().uuidString)")
        addTeardownBlock { try? FileManager.default.removeItem(at: data) }
        let drags = data.appendingPathComponent("drag")
        let (doc, shot) = try edited()
        let url = try XCTUnwrap(Export.dragFile(doc, shot: shot, redaction: Redaction(), retina: true, folder: drags,
                                                disk: FileDisk()))
        XCTAssertTrue(FileManager.default.fileExists(atPath: url.path))
        // Another drag of the same shot replaces it.
        doc.add(.step, points: [CGPoint(x: 90, y: 40)], style: .standard)
        XCTAssertEqual(Export.dragFile(doc, shot: shot, redaction: Redaction(), retina: true, folder: drags,
                                       disk: FileDisk()), url)
        Export.clearDrags(drags)
        XCTAssertFalse(FileManager.default.fileExists(atPath: drags.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: data.path), "only the drag folder goes")
    }

    func testShutdownSavesUnsavedEdits() throws {
        let store = ShotStore()
        let disk = Disk()
        let (doc, shot) = try edited()
        let own = URL(fileURLWithPath: "/tmp/Shots/Shot 2026-11-02 at 00.08.02.png")
        disk.files[own] = Data([7])
        shot.savedTo = own
        store.retain(shot)
        let redaction = Redaction()
        store.setEdits(for: shot) { doc.dirty ? Renderer.export(doc, base: shot.image, redaction: redaction,
                                                                retina: true) : nil }
        let written = store.saveUnsaved(settings: Settings(), disk: disk)
        // Beside the shot's file, never over it.
        XCTAssertEqual(written, [URL(fileURLWithPath: "/tmp/Shots/Shot 2026-11-02 at 00.08.02 (unsaved).png")])
        XCTAssertEqual(disk.files[own], Data([7]))
        XCTAssertTrue(disk.replaced.isEmpty)
        XCTAssertEqual(shot.savedTo, own)

        // Nothing unsaved: nothing written.
        doc.markExported()
        XCTAssertTrue(store.saveUnsaved(settings: Settings(), disk: disk).isEmpty)
        // A closed editor hands its edits back.
        doc.add(.step, points: [CGPoint(x: 9, y: 9)], style: .standard)
        store.setEdits(for: shot, nil)
        XCTAssertTrue(store.saveUnsaved(settings: Settings(), disk: disk).isEmpty)
    }

    func testFailedSaveIsShownAndStaysDirty() throws {
        let (doc, shot) = try edited()
        let disk = Disk()
        disk.fails = true
        var settings = Settings()
        settings.folder = "/tmp/Shots"
        let result = Export.save(doc, shot: shot, redaction: Redaction(), settings: settings, disk: disk)
        XCTAssertEqual(result.problem?.hasPrefix("Couldn't save the shot to /tmp/Shots"), true,
                       String(describing: result.problem))
        XCTAssertTrue(doc.dirty)
        XCTAssertNil(shot.savedTo)
        let elsewhere = Export.saveAs(doc, shot: shot, redaction: Redaction(), retina: true,
                                      to: URL(fileURLWithPath: "/tmp/x.png"), disk: disk)
        XCTAssertNotNil(elsewhere.problem)
        XCTAssertTrue(doc.dirty)
    }

    func testPinnedIsTheExportWhereItWas() throws {
        let shot = Shot(image: try stripes(120, 80), scale: 2, takenAt: Date(), origin: CGPoint(x: 100, y: 200))
        let doc = Document(size: CGSize(width: 120, height: 80), scale: 2)
        // Untouched: the very same shot.
        XCTAssertTrue(Export.pinned(doc, shot: shot, redaction: Redaction()) === shot)
        doc.setCrop(CGRect(x: 20, y: 10, width: 60, height: 40))
        let pinned = try XCTUnwrap(Export.pinned(doc, shot: shot, redaction: Redaction()))
        XCTAssertEqual(pinned.image.width, 60)
        XCTAssertEqual(pinned.origin, CGPoint(x: 110, y: 205))
        XCTAssertNil(pinned.savedTo)
    }

    func testDensityRoundTrips() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("shots-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: folder) }
        let image = try stripes(40, 20)
        for (scale, expected) in [(CGFloat(2), CGFloat(2)), (1, 1)] {
            let url = folder.appendingPathComponent("\(scale).png")
            try XCTUnwrap(Encoding.data(image, format: .png, scale: scale)).write(to: url)
            XCTAssertEqual(Encoding.load(url, fallbackScale: 3)?.scale, expected)
        }
        // A file without a density (Shots 0.1.0's) is taken at this Mac's scale.
        let old = folder.appendingPathComponent("old.png")
        let data = NSMutableData()
        let destination = try XCTUnwrap(CGImageDestinationCreateWithData(data, "public.png" as CFString, 1, nil))
        CGImageDestinationAddImage(destination, image, nil)
        XCTAssertTrue(CGImageDestinationFinalize(destination))
        try (data as Data).write(to: old)
        XCTAssertEqual(Encoding.load(old, fallbackScale: 2)?.scale, 2)
    }
}
