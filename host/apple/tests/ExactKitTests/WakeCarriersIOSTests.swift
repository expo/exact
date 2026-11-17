#if os(iOS)
import XCTest
@testable import ExactKit

// Which BackgroundTasks identifier carries a wake request (LLP 1133.017 D8):
// the task `schedule` submits for a request is the one whose arrival takes
// it, including a processing request on the refresh identifier when the
// processing ones are not permitted (stage 2).
final class WakeCarriersIOSTests: XCTestCase {
    private let suffixes = ["refresh", "processing.network", "processing.network-power"]

    private func carriers(_ kind: String, power: Bool, processing: Bool) -> [String] {
        suffixes.filter { WakeStore.carries($0, kind: kind, power: power, processing: processing) }
    }

    func testEachKindHasItsOwnIdentifierWhenAllArePermitted() {
        XCTAssertEqual(carriers("refresh", power: false, processing: true), ["refresh"])
        XCTAssertEqual(carriers("processing", power: false, processing: true), ["processing.network"])
        XCTAssertEqual(carriers("processing", power: true, processing: true), ["processing.network-power"])
    }

    func testTheRefreshIdentifierCarriesProcessingWhenTheProcessingOnesAreNotPermitted() {
        XCTAssertEqual(carriers("refresh", power: false, processing: false), ["refresh"])
        XCTAssertTrue(carriers("processing", power: false, processing: false).contains("refresh"))
        XCTAssertTrue(carriers("processing", power: true, processing: false).contains("refresh"))
    }

    func testTransfersAndRegionsRideNoTask() {
        for kind in ["transfer", "region"] {
            XCTAssertEqual(carriers(kind, power: false, processing: false), [])
            XCTAssertEqual(carriers(kind, power: false, processing: true), [])
        }
    }
}
#endif
