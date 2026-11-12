import XCTest
@testable import ExactKit

// Which background tasks an abort of a registration cancels: its own, read
// by the description `start` gives them, and none of a registration whose id
// begins with the aborted one and a space.
final class TransferCarrierTests: XCTestCase {
    func testAnAbortSelectsOnlyItsOwnRegistrationsTasks() {
        XCTAssertTrue(TransferCarrier.belongs("album 0", to: "album"))
        XCTAssertTrue(TransferCarrier.belongs("album 1", to: "album"))
        XCTAssertFalse(TransferCarrier.belongs("album 2 0", to: "album"))
        XCTAssertTrue(TransferCarrier.belongs("album 2 0", to: "album 2"))
        XCTAssertFalse(TransferCarrier.belongs("album 0", to: "album 2"))
        XCTAssertFalse(TransferCarrier.belongs(nil, to: "album"))
        XCTAssertFalse(TransferCarrier.belongs("album", to: "album"))
    }
}
