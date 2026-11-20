#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation
@testable import ExactCollections
@testable import ExactNativeViews
@testable import ExactFlights

extension ContentOwnersIOSTests {
    /// A runtime replaced while a report waits behind a slice in flight
    /// (the plan booted again, as a dev reload does): the old runtime's
    /// slice lands and its waiters are dropped, but what the new runtime's
    /// routes report is not, and they keep reporting as they change.
    func testAReloadDuringASliceLeavesTheRoutesReporting() throws {
        let session = try fixture("owner-reload", CGSize(width: 402, height: 874))
        let p = session.presenter
        p.press(try node(session, "sheet").id)
        let sheet = try node(session, "route-sheet")
        let controller = try mountSheet()
        let sheetView = try XCTUnwrap(p.navigationHost.controllers[sheet.id]?.view)
        until("the sheet's route takes its size") { self.laidOut(session, sheet.id) == sheetView.bounds.size }
        // A slice in flight, and a report waiting behind it
        p.listHost.onFill?(sheet.id, Data())
        XCTAssertTrue(session.fillInFlight)
        controller.view.frame = CGRect(x: 0, y: 374, width: 402, height: 500)
        controller.view.layoutIfNeeded()
        sheetView.layoutIfNeeded()
        let before = try node(session, "route-home")
        let plan = try Data(contentsOf: URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_FIXTURE_PLAN"])))
        XCTAssertNil(session.boot(plan: plan, size: CGSize(width: 402, height: 874)).error)
        until("the plan booted again") { (try? self.node(session, "route-home")).map { $0 !== before } == true }
        until("the old slice is gone") { !session.fillInFlight }
        let home = try node(session, "route-home")
        until("the new route has its block") { p.owners.told[home.id] != nil }
        let homeView = try XCTUnwrap(p.navigationHost.controllers[home.id]?.view)
        homeView.frame.size.width = 380
        homeView.layoutIfNeeded()
        until("and follows its controller") { p.owners.told[home.id]?.block == .size(homeView.bounds.size) }
    }

    /// The root no longer projecting its routes (its `navigationBack` gone)
    /// takes the stack down: the routes that stay give their blocks and
    /// placements back, and are laid out in their parent again, under
    /// `viewport-fit="cover"` as under `auto`.
    func testRoutesGiveTheirBlocksBackWhenTheStackGoes() throws {
        let session = try fixture("owner-teardown", CGSize(width: 402, height: 874))
        let p = session.presenter
        let root = try XCTUnwrap(p.navigationHost.container)
        p.apply(wireBatch([["op": "props", "id": Int(root.id), "set": ["viewportFit": "cover"]]]))
        let home = try node(session, "route-home")
        until("the route has its block") { p.owners.told[home.id] != nil }
        p.apply(wireBatch([["op": "props", "id": Int(root.id), "set": [String: String](), "clear": ["navigationBack"]]]))
        until("the stack is down") { p.navigationHost.primaryOwner == nil }
        XCTAssertTrue(p.owners.told.isEmpty, "every block given back: \(p.owners.told.keys.sorted())")
        XCTAssertTrue(home.isDescendant(of: root), "the route is its parent's again")
    }
}
#endif
