#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactImages

/// LLP 1031 D3: resources follow the containing view, including a display
/// scale change that leaves every logical point and the live runner intact.
final class DisplayIOSTests: XCTestCase {
    func testDisplayScaleChangesUpdateTheSameSessionsBudgetWithoutResizingIt() {
        let session = ExactApp.shared.makeSession(label: "display")
        defer { session.destroy() }
        let view = ExactView(session: session)
        view.frame = CGRect(x: 0, y: 0, width: 1200, height: 1000)
        view.traitOverrides.displayScale = 1
        view.updateTraitsIfNeeded()
        view.fit()
        XCTAssertTrue(session.booted, session.bootError ?? "did not boot")
        let runtime = session.runtime.rt, raster = session.loader.id
        let points = session.viewportSize
        XCTAssertEqual(session.loader.budget, 38_400_000)
        view.traitOverrides.displayScale = 3
        view.updateTraitsIfNeeded()
        view.fit()
        XCTAssertEqual(session.viewportSize, points)
        XCTAssertEqual(session.runtime.rt, runtime)
        XCTAssertEqual(session.loader.id, raster)
        XCTAssertEqual(session.loader.budget, 192 * 1024 * 1024)
        view.frame.size = CGSize(width: 402, height: 700)
        view.fit()
        XCTAssertEqual(session.viewportSize, CGSize(width: 402, height: 700))
        XCTAssertEqual(session.loader.budget, 81_043_200)
        view.traitOverrides.displayScale = 1
        view.updateTraitsIfNeeded()
        view.fit()
        XCTAssertEqual(session.loader.budget, 32 * 1024 * 1024)
        XCTAssertEqual(session.runtime.rt, runtime)
        XCTAssertEqual(session.loader.id, raster)
    }
}
#endif
