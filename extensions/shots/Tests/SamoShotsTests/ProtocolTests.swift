import XCTest
@testable import ShotsCore

final class ProtocolTests: XCTestCase {
    private func object(_ line: String?) throws -> [String: Any] {
        let data = try XCTUnwrap(line?.data(using: .utf8))
        return try XCTUnwrap(try JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    func testProtocolRoundTrip() throws {
        // Samo's messages, as samo-ext-protocol writes them.
        XCTAssertEqual(
            HostMsg.decode(#"{"type":"hello","protocol":1,"samo_version":"0.3.0","data_dir":"/tmp/x y"}"#),
            .hello(protocolVersion: 1, samoVersion: "0.3.0", dataDir: "/tmp/x y")
        )
        XCTAssertEqual(HostMsg.decode(#"{"type":"action","id":"ping"}"#), .action(id: "ping", row: nil))
        XCTAssertEqual(
            HostMsg.decode(#"{"type":"action","id":"copy","row":"shot-1"}"#),
            .action(id: "copy", row: "shot-1")
        )
        XCTAssertEqual(HostMsg.decode(#"{"type":"choice","key":"after","value":"pin"}"#), .choice(key: "after", value: "pin"))
        XCTAssertEqual(HostMsg.decode(#"{"type":"setting","key":"sound","value":true}"#), .setting(key: "sound", value: true))
        XCTAssertEqual(HostMsg.decode(#"{"type":"shutdown"}"#), .shutdown)
        // Anything else is ignored, never fatal.
        for line in [#"{"type":"explode"}"#, "not json", "", "[]", #"{"type":"setting","key":"x"}"#] {
            XCTAssertNil(HostMsg.decode(line), line)
        }

        // Shots' messages, in the shape samo-ext-protocol reads.
        let view = HelperMsg.view([
            .section(title: "RECENT", about: "3 today"),
            .rows([RowItem(id: "shot-1", lamp: .done, title: "Shot.png", detail: "2.1 MB",
                           actions: [RowAction(id: "copy", label: "Copy")])]),
            .choice(key: "after", label: "After a capture",
                    options: [ChoiceOption(id: "copy", label: "Copy")], selected: "copy"),
            .toggle(key: "sound", label: "Sound", on: false, note: nil),
        ])
        let line = try XCTUnwrap(view.line())
        XCTAssertFalse(line.contains("\n"))
        let decoded = try object(line)
        XCTAssertEqual(decoded["type"] as? String, "view")
        let sections = try XCTUnwrap(decoded["sections"] as? [[String: Any]])
        XCTAssertEqual(sections.map { $0["widget"] as? String }, ["section", "rows", "choice", "switch"])
        let row = try XCTUnwrap((sections[1]["items"] as? [[String: Any]])?.first)
        XCTAssertEqual(row["id"] as? String, "shot-1")
        XCTAssertEqual((row["actions"] as? [[String: String]])?.first?["id"], "copy")
        XCTAssertEqual(try object(HelperMsg.capturing.line())["type"] as? String, "capturing")
        XCTAssertEqual(try object(HelperMsg.captured.line())["type"] as? String, "captured")
        // A title with a newline stays one line.
        XCTAssertFalse(try XCTUnwrap(HelperMsg.log(event: "x", detail: "a\nb").line()).contains("\n"))
    }

    func testHelloComesFirst() {
        let out = MemoryOutput()
        var shutDown = false
        let session = Session(out: out, shared: false, handlers: Session.Handlers(
            page: { [.note(text: "hi", kind: .plain)] },
            shutdown: { shutDown = true }
        ))
        session.start()
        XCTAssertEqual(out.sent.first, .hello)
        XCTAssertEqual(out.sent.dropFirst().first, .view([.note(text: "hi", kind: .plain)]))
        XCTAssertTrue(session.handle(.hello(protocolVersion: 1, samoVersion: "0.3.0", dataDir: "/tmp/d")))
        XCTAssertEqual(session.dataDir, "/tmp/d")
        XCTAssertFalse(session.handle(.shutdown))
        XCTAssertTrue(shutDown)
    }
}
