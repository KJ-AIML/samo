import XCTest
@testable import SamoKit

final class SpawnTests: XCTestCase {
    func testDisclaimFallbackIsReported() throws {
        // Without the private attribute, the worker is told it shares Samo's permission...
        XCTAssertEqual(Spawn.workerArguments(executable: "/x/samo-shots", disclaimed: false),
                       ["/x/samo-shots", "--worker", "--shared"])
        XCTAssertEqual(Spawn.workerArguments(executable: "/x/samo-shots", disclaimed: true),
                       ["/x/samo-shots", "--worker"])
        let started = try XCTUnwrap(Spawn.startWorker(executable: "/usr/bin/true", setter: { nil }))
        XCTAssertFalse(started.disclaimed)
        var status: Int32 = 0
        waitpid(started.pid, &status, 0)

        // ...and says so to Samo, first thing after hello.
        let out = MemoryOutput()
        Session(out: out, shared: true, sharedNote: "Shots couldn't start as its own app, so Screen Recording is "
                    + "shared with Samo", handlers: Session.Handlers(page: { [] })).start()
        guard case let .log(event, detail) = out.sent.dropFirst().first else {
            return XCTFail("no log after hello: \(out.sent)")
        }
        XCTAssertEqual(event, "permission")
        XCTAssertTrue(detail.contains("shared with Samo"), detail)

        // On this Mac the attribute exists.
        XCTAssertNotNil(Spawn.disclaimSetter())
    }
}
