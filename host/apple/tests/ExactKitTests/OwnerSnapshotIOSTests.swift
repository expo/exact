#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation
@testable import ExactCollections

/// LLP 1123 D2 ("Windows"): an owner's update is sampled in the pass that takes its block, when it is
/// told, so a resize step's block, band and targets describe one native layout (`OwnerBlocks.samplesOwners`).
final class OwnerSnapshotIOSTests: XCTestCase {
    private var window: UIWindow?
    private var host: UIViewController?
    private var sessions: [ExactSession] = []

    override func tearDown() {
        for session in sessions { session.destroy() }
        sessions = []
        window?.isHidden = true
        window = nil
        host = nil
        super.tearDown()
    }

    private func until(_ what: String, _ seconds: Double = 5, _ done: () -> Bool) {
        let deadline = Date().addingTimeInterval(seconds)
        while !done(), Date() < deadline { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.02)) }
        XCTAssertTrue(done(), what)
    }

    private func fixture(_ scene: UIWindowScene) throws -> ExactSession {
        let env = ProcessInfo.processInfo.environment
        let plan = try Data(contentsOf: URL(fileURLWithPath: try XCTUnwrap(env["EXACT_FIXTURE_PLAN"], "build.mjs --test --ios compiles EXACT_FIXTURE_PLAN")))
        let session = ExactApp.shared.makeSession(label: "owner-snapshot")
        sessions.append(session)
        let view = ExactView(session: session)
        let host = UIViewController(), size = CGSize(width: 402, height: 874)
        let window = UIWindow(windowScene: scene)
        window.frame = CGRect(origin: .zero, size: size)
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        self.window = window
        self.host = host
        XCTAssertNil(session.boot(plan: plan, size: size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        until("the stack is installed") { session.presenter.navigationHost.primaryOwner != nil }
        // A sheet waits for the first drawn frame, which this window never draws.
        if session.firstDrawMs == nil { session.firstDrawMs = ExactEnv.wall() }
        return session
    }

    private func node(_ session: ExactSession, _ testId: String) throws -> NodeView {
        try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == testId }, "no \(testId)")
    }

    private func mountSheet() throws -> UIViewController {
        let controller = try XCTUnwrap(host?.presentedViewController, "the sheet is presented")
        if controller.view.window == nil { window?.addSubview(controller.view) }
        controller.view.frame = CGRect(x: 0, y: 60, width: 402, height: 814)
        controller.view.layoutIfNeeded()
        return controller
    }

    func testAnOwnersUpdateIsTakenWithItsBlockWhenItIsTold() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a sheet is presented from a window a scene holds") }
        let session = try fixture(scene)
        let p = session.presenter
        p.owners.samplesOwners = true
        // The shown route's band moving at an unchanged size is an update, its block unchanged
        let home = try XCTUnwrap(p.navigationHost.controllers.first { $0.value.view.window != nil }, "the shown route")
        p.owners.publish()
        until("the route's update is taken") { p.owners.toldUpdates[home.key] != nil }
        let block = p.owners.told[home.key]?.block, top = home.value.view.safeAreaInsets.top
        _ = p.owners.takeUpdates()
        home.value.additionalSafeAreaInsets.top += 20
        window?.layoutIfNeeded()
        XCTAssertEqual(home.value.view.safeAreaInsets.top, top + 20)
        p.owners.publish()
        until("the band's move is an update") { p.owners.pending[home.key]?.band?.top == top + 20 }
        XCTAssertEqual(p.owners.told[home.key]?.block, block, "at the size it had")
        home.value.additionalSafeAreaInsets.top -= 20

        // A sheet resized twice while a slice holds its report is taken at the last size
        p.press(try node(session, "sheet").id)
        let sheet = try node(session, "route-sheet")
        let controller = try mountSheet()
        let sheetView = try XCTUnwrap(p.navigationHost.controllers[sheet.id]?.view)
        let resize = { (height: CGFloat) in
            controller.view.frame = CGRect(x: 0, y: 874 - height, width: 402, height: height)
            controller.view.layoutIfNeeded()
            sheetView.layoutIfNeeded()
        }
        let height = { p.owners.toldUpdates[sheet.id]?.chain.first?.target.frame?.height }
        resize(814)
        until("the sheet's update is taken with its block") { height() == sheetView.convert(sheetView.bounds, to: nil).height }
        // A slice in flight holds the report: two resizes wait behind it, and nothing is taken
        p.listHost.onFill?(sheet.id, Data())
        XCTAssertTrue(session.fillInFlight)
        let held = p.owners.toldUpdates[sheet.id]
        resize(500)
        resize(300)
        XCTAssertEqual(p.owners.toldUpdates[sheet.id], held, "no update taken while the report waits")
        until("the slice lands") { !session.fillInFlight }
        // Taken in one pass with its block, never at the size that waited first; UIKit may lay the
        // sheet out again after, which a later take samples whole again
        until("the update is taken with its block, past the held sizes") {
            // A sheet below its large detent floats scaled: its frame in the window is its block through the map
            guard let update = p.owners.toldUpdates[sheet.id], let frame = update.chain.first?.target.frame, let map = update.windowMap,
                  case .size(let blockHeight)? = p.owners.told[sheet.id]?.block.height else { return false }
            return abs(frame.height - blockHeight * map.d) < 0.5 && abs(blockHeight - 500) > 0.5
        }

    }
}
#endif
