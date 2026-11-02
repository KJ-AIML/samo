import AppKit
import CoreImage
import XCTest
import SamoKit
@testable import ShotsCore

/// Black text on white, one line each, as a web page would show it.
func page(_ lines: [String], size: CGFloat = 40, width: Int = 1400) throws -> CGImage {
    let height = Int(size * 2.25) * lines.count + 60
    let space = try XCTUnwrap(CGColorSpace(name: CGColorSpace.sRGB))
    let context = try XCTUnwrap(CGContext(
        data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0, space: space,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    ))
    context.setFillColor(CGColor(gray: 1, alpha: 1))
    context.fill(CGRect(x: 0, y: 0, width: width, height: height))
    Renderer.withAppKit(context) {
        for (i, line) in lines.enumerated() {
            (line as NSString).draw(at: CGPoint(x: 30, y: CGFloat(height) - 40 - size * 2.25 * CGFloat(i + 1)),
                                    withAttributes: [.font: NSFont.systemFont(ofSize: size)])
        }
    }
    return try XCTUnwrap(context.makeImage())
}

func qrCode(_ text: String, on size: CGSize = CGSize(width: 900, height: 500)) throws -> CGImage {
    let filter = try XCTUnwrap(CIFilter(name: "CIQRCodeGenerator"))
    filter.setValue(Data(text.utf8), forKey: "inputMessage")
    let code = try XCTUnwrap(filter.outputImage).transformed(by: CGAffineTransform(scaleX: 8, y: 8))
    let image = try XCTUnwrap(CIContext().createCGImage(code, from: code.extent))
    let space = try XCTUnwrap(CGColorSpace(name: CGColorSpace.sRGB))
    let context = try XCTUnwrap(CGContext(
        data: nil, width: Int(size.width), height: Int(size.height), bitsPerComponent: 8, bytesPerRow: 0,
        space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    ))
    context.setFillColor(CGColor(gray: 0.93, alpha: 1))
    context.fill(CGRect(origin: .zero, size: size))
    context.draw(image, in: CGRect(x: size.width - 300, y: 150, width: 200, height: 200))
    return try XCTUnwrap(context.makeImage())
}

final class RecognizeTests: XCTestCase {
    func testThaiAndEnglishReadExactly() async throws {
        let lines = [
            "สวัสดีครับ วันนี้อากาศดีมาก",
            "ราคาสินค้า 1,250 บาท ส่งฟรีทั่วประเทศ",
            "Samo วัดพื้นที่ดิสก์และสุขภาพเครื่อง Mac",
            "The quick brown fox jumps over 13 lazy dogs",
        ]
        let read = await TextReader.read(try page(lines))
        XCTAssertEqual(read, lines)
    }

    func testLinesInReadingOrder() {
        // Two pieces of one line, found right first; and the lower line found first.
        let found = [
            FoundText(text: "second line", box: CGRect(x: 0.1, y: 0.2, width: 0.5, height: 0.1)),
            FoundText(text: "world", box: CGRect(x: 0.5, y: 0.62, width: 0.2, height: 0.1)),
            FoundText(text: "Hello", box: CGRect(x: 0.1, y: 0.6, width: 0.3, height: 0.1)),
        ]
        XCTAssertEqual(TextReader.lines(found), ["Hello world", "second line"])
        XCTAssertEqual(TextReader.lines([]), [])
    }

    func testNoTextIsEmpty() async throws {
        let blank = try plain(400, 200)
        let read = await TextReader.read(blank)
        XCTAssertEqual(read, [])
        let codes = await QRReader.read(blank)
        XCTAssertEqual(codes, [])
    }

    func testLanguagesPutThaiFirst() {
        let supported = ["en-US", "fr-FR", "th-TH", "ja-JP", "zh-Hans"]
        XCTAssertEqual(TextReader.languages(preferred: ["en-GB", "ja-JP", "xx-YY"], supported: supported),
                       ["th-TH", "en-US", "ja-JP"])
        XCTAssertEqual(TextReader.languages(preferred: ["fr-CA"], supported: supported), ["th-TH", "en-US", "fr-FR"])
        // A Mac whose Vision lacks Thai still reads English.
        XCTAssertEqual(TextReader.languages(preferred: [], supported: ["en-US"]), ["en-US"])
    }

    func testQRPayloadIsRead() async throws {
        let codes = await QRReader.read(try qrCode("https://github.com/KJ-AIML/samo"))
        XCTAssertEqual(codes, ["https://github.com/KJ-AIML/samo"])
    }

    func testOnlyWebLinksOpen() {
        XCTAssertEqual(Links.web("https://github.com/KJ-AIML/samo")?.host, "github.com")
        XCTAssertNotNil(Links.web("  HTTP://example.com/a?b=c  "))
        for text in ["javascript:alert(1)", "file:///etc/passwd", "x-apple.systempreferences:com.apple.preference",
                     "ftp://example.com", "http://", "hello world", "https://exa mple.com", "mailto:kj@example.com",
                     "samo://open"] {
            XCTAssertNil(Links.web(text), text)
            XCTAssertFalse(Links.open(text), text)
        }
    }
}
