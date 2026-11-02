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
