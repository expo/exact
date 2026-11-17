import CoreGraphics
import XCTest
import ExactPackage

final class TrackPictureTests: XCTestCase {
    func testAThrowingObserverDoesNotSkipTheRestOrTheDeliveryCompletion() throws {
        let entry = ExactTrackEntry(id: "video", kind: "video")
        let context = try XCTUnwrap(CGContext(data: nil, width: 1, height: 1, bitsPerComponent: 8, bytesPerRow: 4,
                                             space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        let picture = try XCTUnwrap(context.makeImage())
        var calls: [String] = []
        var subscriptions: [ExactSubscription] = []
        defer { subscriptions.forEach { $0.cancel() } }
        for name in ["first", "second", "third"] {
            subscriptions.append(entry.observePicture {
                XCTAssertTrue(entry.picture === picture)
                calls.append(name)
                // Dictionary order is unspecified: whichever observer runs first throws.
                if calls.count == 1 { throw ExactTrackError("observer failed") }
            })
        }

        entry.picture = picture
        calls.append("completed")

        XCTAssertEqual(Set(calls.dropLast()), Set(["first", "second", "third"]))
        XCTAssertEqual(calls.last, "completed")
        entry.picture = picture
        XCTAssertEqual(calls.count, 4)
    }
}
