#if os(iOS)
@testable import ExactKit
import UIKit
import XCTest

/// The iOS host's owner update and registered targets (LLP 1123 D2, D9): what it samples from
/// UIKit for a route, `ExactNativeTarget`'s names, and the kernel taking both through
/// `exact_host_covers` kinds 21 and 22.
@MainActor
final class OwnerUpdateIOSTests: XCTestCase {
    private var window: UIWindow?
    private var sessions: [ExactSession] = []
    private var sessionView: UIView?

    override func tearDown() {
        for session in sessions { session.destroy() }
        sessions = []
        sessionView = nil
        window?.isHidden = true
        window = nil
        super.tearDown()
    }

    func testARoutesUpdateIsWhatUIKitGivesIt() throws {
        let session = try fixture()
        let (route, view) = try firstRoute(session)
        let update = OwnerUpdateIOS.sample(controller: route, view: view, session: sessionView)
        let sessionTarget = update.relations.first { $0.relation == OwnerUpdateIOS.Relation.session }
        XCTAssertEqual(sessionTarget?.target.id, TargetIds.id(of: try XCTUnwrap(sessionView)))
        XCTAssertEqual(update.band, view.safeAreaInsets)
        // Entry 1 is the route's own view, and the navigation controller holding it is named
        let first = try XCTUnwrap(update.chain.first)
        XCTAssertEqual(first.target.insets, view.safeAreaInsets)
        XCTAssertEqual(first.target.id, TargetIds.id(of: view))
        let navigation = try XCTUnwrap(route.navigationController?.view)
        let named = update.chain.first { $0.kinds & OwnerUpdateIOS.Kind.navigation != 0 }
        XCTAssertEqual(named?.target.insets, navigation.safeAreaInsets)
        let window = try XCTUnwrap(view.window)
        let windowTarget = update.relations.first { $0.relation == OwnerUpdateIOS.Relation.window }
        XCTAssertEqual(windowTarget?.target.insets, window.safeAreaInsets)
        let margins = try XCTUnwrap(update.margins)
        XCTAssertEqual(margins.root, view.layoutMargins)
        XCTAssertEqual(margins.plain, UIView().layoutMargins)
        // The width UIKit keeps text to, not the route's guide, which its margins clamp: what a
        // hidden view 4000 wide with no margins gives under the window's root, as the probe read it
        XCTAssertEqual(view.readableContentGuide.layoutFrame.width, view.layoutMarginsGuide.layoutFrame.width)
        let probe = UIView(frame: CGRect(x: 0, y: 0, width: 4000, height: 10))
        probe.isHidden = true
        probe.insetsLayoutMarginsFromSafeArea = false
        probe.preservesSuperviewLayoutMargins = false
        probe.layoutMargins = .zero
        let root = try XCTUnwrap(view.window?.rootViewController?.view)
        root.addSubview(probe)
        probe.layoutIfNeeded()
        XCTAssertEqual(margins.readableWidth, probe.readableContentGuide.layoutFrame.width)
        XCTAssertGreaterThan(margins.readableWidth, view.bounds.width)
        XCTAssertLessThan(margins.readableWidth, 4000)
        probe.removeFromSuperview()
        let minimum = route.systemMinimumLayoutMargins
        XCTAssertEqual([margins.minimum.left, margins.minimum.right], [minimum.leading, minimum.trailing])
    }

    func testTheKernelTakesAnEncodedUpdateAndRefusesATruncatedOne() throws {
        let session = try fixture()
        let (route, view) = try firstRoute(session)
        let update = OwnerUpdateIOS.sample(controller: route, view: view, session: sessionView)
        // The margins alone, which move no content root before activation
        let margins = OwnerUpdateIOS(margins: update.margins).encode()
        XCTAssertNil(session.runtime.ownerReports(updates: [(route.node.id, margins)]).error)
        let truncated = margins.prefix(margins.count - 1)
        XCTAssertNotNil(session.runtime.ownerReports(updates: [(route.node.id, Data(truncated))]).error)
        // And the whole sample, its chain and relations with it, which the kernel decodes alike
        XCTAssertNil(session.runtime.ownerReports(updates: [(route.node.id, update.encode())]).error)
    }

    func testARegistrationRefusesAnInvalidOrTakenNameAndFreesItWhenDisposed() throws {
        let session = try fixture()
        let (map, other) = (UIView(), UIView())
        XCTAssertThrowsError(try ExactNativeTarget.register(name: "Map", view: map, in: session)) {
            XCTAssertEqual($0 as? ExactNativeTarget.Refusal, .invalidName("Map"))
        }
        let held = try ExactNativeTarget.register(name: "map", view: map, in: session)
        XCTAssertThrowsError(try ExactNativeTarget.register(name: "map", view: other, in: session)) {
            XCTAssertEqual($0 as? ExactNativeTarget.Refusal, .taken("map"))
        }
        held.dispose()
        var again: ExactNativeTarget? = try ExactNativeTarget.register(name: "map", view: other, in: session)
        XCTAssertNotNil(again)
        // Dropping a registration frees its name too
        again = nil
        XCTAssertNoThrow(try ExactNativeTarget.register(name: "map", view: map, in: session))
    }

    func testTheKernelTakesARegistrationAndRefusesATruncatedOne() throws {
        let session = try fixture()
        let target = OwnerUpdateIOS.target(window)
        let record = ExactNativeTarget.encode(name: "map", target: target)
        XCTAssertNil(session.runtime.ownerReports(registrations: [record]).error)
        XCTAssertNil(session.runtime.ownerReports(registrations: [ExactNativeTarget.encode(name: "map", target: nil)]).error)
        XCTAssertNotNil(session.runtime.ownerReports(registrations: [Data(record.prefix(record.count - 1))]).error)
    }

    func testTheReadableWidthIsReadInTheOwnersWindowAndIsNoStray() throws {
        let session = try fixture()
        let (route, view) = try firstRoute(session)
        let width = OwnerUpdateIOS.sample(controller: route, view: view, session: nil).margins?.readableWidth
        // A session view in no window, where a view reads 0: the owner view hosts the read instead
        let elsewhere = UIView()
        XCTAssertEqual(OwnerUpdateIOS.sample(controller: route, view: view, session: elsewhere).margins?.readableWidth, width)
        XCTAssertNil(ReadableWidthIOS.view(in: elsewhere))
        // In the session view, the measuring view is the session's: the agreement walk counts no more
        // strays after it than before
        let strays = { () throws -> Int in
            let agreement = try XCTUnwrap(Agent(session: session).agreement(limit: 64)["agreement"] as? [String: Any])
            return (agreement["counts"] as? [String: Int])?["stray"] ?? 0
        }
        let before = try strays()
        _ = OwnerUpdateIOS.sample(controller: route, view: view, session: sessionView)
        XCTAssertNotNil(ReadableWidthIOS.view(in: try XCTUnwrap(sessionView)))
        XCTAssertEqual(try strays(), before)
    }

    func testAnOwnerOutOfAWindowHasNoBandMarginsOrSegments() {
        let controller = UIViewController()
        let update = OwnerUpdateIOS.sample(controller: controller, view: controller.view, session: nil)
        XCTAssertNil(update.band)
        XCTAssertNil(update.margins)
        XCTAssertNil(update.segments)
        XCTAssertEqual(update.chain.count, 1)
        XCTAssertNil(update.chain[0].target.insets)
    }

    func testTheKernelTakesAWindowRootsUpdateUnderANegativeContribution() throws {
        // The window's root under Exact's write past its insets reads below 0, its margins with it
        let session = try fixture()
        let (route, _) = try firstRoute(session)
        let root = try XCTUnwrap(window?.rootViewController)
        let ledger = ContributionsIOS.of(root)
        let identity = ContributionsIOS.Identity(node: route.node.id, operation: .additionalInsets)
        ledger.set(identity, UIEdgeInsets(top: -(root.view.safeAreaInsets.top + 40), left: 0, bottom: 0, right: 0), generation: 1)
        let update = OwnerUpdateIOS.sample(controller: root, view: root.view, session: nil)
        XCTAssertEqual(update.band?.top, -40)
        let batch = session.runtime.ownerReports(updates: [(route.node.id, update.encode())])
        XCTAssertNil(batch.error, "\(update.margins.map { "margins \($0)" } ?? "no margins")")
        ledger.set(identity, nil, generation: 2)
        ledger.apply()
    }

    func testATargetsIdentityIsItsViewsForLife() {
        let (a, b) = (UIView(), UIView())
        XCTAssertEqual(TargetIds.id(of: a), TargetIds.id(of: a))
        XCTAssertNotEqual(TargetIds.id(of: a), TargetIds.id(of: b))
        // A view made after another is freed, perhaps at its address, is a new target
        var ids: Set<UInt64> = []
        for _ in 0..<100 { autoreleasepool { ids.insert(TargetIds.id(of: UIView())) } }
        XCTAssertEqual(ids.count, 100)
    }

    func testASplitsColumnIsBothNavigationAndColumnAndAPresentedChainEndsAtIt() throws {
        let scene = hostedWindowScene()
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        window.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        // Regular width, so UIKit shows both columns rather than collapsing them
        window.traitOverrides.horizontalSizeClass = .regular
        let split = UISplitViewController(style: .doubleColumn), inner = UIViewController()
        split.setViewController(UIViewController(), for: .primary)
        split.setViewController(inner, for: .secondary)
        window.rootViewController = split
        window.makeKeyAndVisible()
        self.window = window
        window.layoutIfNeeded()
        let update = OwnerUpdateIOS.sample(controller: inner, view: inner.view, session: nil)
        let kind = OwnerUpdateIOS.Kind.self
        XCTAssertEqual(update.chain.map(\.kinds), [0, kind.navigation | kind.column, kind.split])
        XCTAssertEqual(update.relations.first { $0.relation == OwnerUpdateIOS.Relation.root }?.target.id, TargetIds.id(of: split.view))
        // A controller presented from it: a chain of one, and the presenter a relation
        let presented = UIViewController()
        split.present(presented, animated: false)
        let deadline = Date().addingTimeInterval(5)
        while presented.view.window == nil, Date() < deadline { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.02)) }
        let sheet = OwnerUpdateIOS.sample(controller: presented, view: presented.view, session: nil)
        XCTAssertEqual(sheet.chain.count, 1)
        let presenter = try XCTUnwrap(presented.presentingViewController?.view)
        XCTAssertEqual(sheet.relations.first { $0.relation == OwnerUpdateIOS.Relation.presenter }?.target.id, TargetIds.id(of: presenter))
        presented.dismiss(animated: false)
    }

    func testTheEncodingIsTheKernelsByteForByte() {
        // Written out field by field apart from the encoder, as the kernel's wire test writes it
        var update = OwnerUpdateIOS()
        update.band = UIEdgeInsets(top: 1, left: 4, bottom: 3, right: 2)
        update.chain = [.init(target: .init(id: 5, generation: 6, insets: nil), kinds: OwnerUpdateIOS.Kind.navigation)]
        update.relations = [.init(relation: OwnerUpdateIOS.Relation.window, target: .init(id: 7, generation: 1, insets: UIEdgeInsets(top: 8, left: 0, bottom: 0, right: 0)))]
        update.margins = .init(root: UIEdgeInsets(top: 9, left: 12, bottom: 11, right: 10), plain: UIEdgeInsets(top: 8, left: 8, bottom: 8, right: 8),
                               minimum: UIEdgeInsets(top: 0, left: 16, bottom: 0, right: 16), readableWidth: 672)
        var w = [UInt8]()
        func f(_ values: Float...) { for v in values { w += withUnsafeBytes(of: v.bitPattern.littleEndian, Array.init) } }
        func u32(_ v: UInt32) { w += withUnsafeBytes(of: v.littleEndian, Array.init) }
        func u64(_ v: UInt64) { w += withUnsafeBytes(of: v.littleEndian, Array.init) }
        w.append(1); f(1, 2, 3, 4)
        u32(0)
        u32(1); u64(5); u64(6); w.append(0); w.append(1 << 2)
        u32(1); w.append(3); u64(7); u64(1); w.append(1); f(8, 0, 0, 0)
        w.append(1); f(9, 10, 11, 12); f(8, 8, 8, 8); f(0, 16, 0, 16); f(672)
        w.append(0)
        w.append(0)
        XCTAssertEqual([UInt8](update.encode()), w)
        // A registration: the name's length and bytes, then the target after a presence byte
        let registration = ExactNativeTarget.encode(name: "map", target: .init(id: 9, generation: 2, insets: UIEdgeInsets(top: 30, left: 0, bottom: 0, right: 0)))
        w = [3] + Array("map".utf8) + [1]
        u64(9); u64(2); w.append(1); f(30, 0, 0, 0)
        XCTAssertEqual([UInt8](registration), w)
    }

    /// The native fixture booted in a window of the test host's scene, 402 by 874.
    private func fixture() throws -> ExactSession {
        let env = ProcessInfo.processInfo.environment
        let path = try XCTUnwrap(env["EXACT_FIXTURE_PLAN"], "build.mjs --test --ios compiles EXACT_FIXTURE_PLAN")
        let plan = try Data(contentsOf: URL(fileURLWithPath: path))
        let size = CGSize(width: 402, height: 874)
        let session = ExactApp.shared.makeSession(label: "owner-update")
        sessions.append(session)
        let view = ExactView(session: session)
        sessionView = view
        let host = UIViewController()
        let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: CGRect(origin: .zero, size: size))
        window.frame = CGRect(origin: .zero, size: size)
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        self.window = window
        XCTAssertNil(session.boot(plan: plan, size: size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        let deadline = Date().addingTimeInterval(5)
        while session.presenter.navigation.primaryOwner == nil, Date() < deadline {
            RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.02))
        }
        return session
    }

    /// The first route shown in a window, as its controller and its view.
    private func firstRoute(_ session: ExactSession) throws -> (any RouteSurface & UIViewController, UIView) {
        let routes = session.presenter.navigation.routeSurfaces.values
        let shown = routes.compactMap { $0 as? any RouteSurface & UIViewController }
            .first { $0.surface.window != nil }
        let route = try XCTUnwrap(shown, "the fixture shows a route")
        return (route, route.surface)
    }
}
#endif
