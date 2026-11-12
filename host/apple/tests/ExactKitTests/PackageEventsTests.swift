import XCTest
import ExactPackage

/// A view's declared event (`ExactPackageEvents.emit`) with arguments JSON
/// cannot hold is not sent, and the process lives: `JSONSerialization` raises
/// an uncaught Objective-C exception for a NaN or an infinity.
final class PackageEventsTests: XCTestCase {
    func testAnEventWhoseArgumentsAreNotJSONIsNotSent() {
        var sent: [String] = []
        let events = ExactPackageEvents(send: { _, text in sent.append(text) }, size: { _ in })
        events.emit("level", [Double.nan])
        events.emit("level", [Double.infinity])
        events.emit("level", [0.5])
        XCTAssertEqual(sent.count, 1, "\(sent)")
        let body = try? JSONSerialization.jsonObject(with: Data(sent.first?.utf8 ?? "".utf8)) as? [String: Any]
        XCTAssertEqual(body?["exact:event"] as? String, "level")
        XCTAssertEqual(body?["args"] as? [Double], [0.5])
    }
}
