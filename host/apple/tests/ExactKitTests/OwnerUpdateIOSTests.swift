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
        // Each target's coordinate space is its bounds in its window, the window's its own
        XCTAssertEqual(first.target.frame, view.convert(view.bounds, to: nil))
        let navigation = try XCTUnwrap(route.navigationController?.view)
        let named = update.chain.first { $0.kinds & OwnerUpdateIOS.Kind.navigation != 0 }
        XCTAssertEqual(named?.target.insets, navigation.safeAreaInsets)
        let window = try XCTUnwrap(view.window)
        let windowTarget = update.relations.first { $0.relation == OwnerUpdateIOS.Relation.window }
        XCTAssertEqual(windowTarget?.target.insets, window.safeAreaInsets)
        XCTAssertEqual(windowTarget?.target.frame, window.bounds)
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
        // Each edge's bar region, its distance from that edge and its sides in order, as UIKit gives it
        if #available(iOS 27.1, *) {
            for (i, edge) in [UIRectEdge.top, .right, .bottom, .left].enumerated() {
                let u = view.edgeInsets(for: .bar(onEdge: edge, extent: 50)), region = try XCTUnwrap(update.barRegions[i])
                let (distance, sides) = edge == .top ? (u.top, (u.left, u.right)) : edge == .right ? (u.right, (u.top, u.bottom))
                    : edge == .bottom ? (u.bottom, (u.left, u.right)) : (u.left, (u.top, u.bottom))
                XCTAssertEqual(region, .init(distance: distance, sides: sides), "edge \(i)")
            }
        } else {
            XCTAssertTrue(update.barRegions.allSatisfy { $0 == nil }, "no bar region before iOS 27.1")
        }
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
        XCTAssertTrue(update.barRegions.allSatisfy { $0 == nil })
        XCTAssertEqual(update.chain.count, 1)
        XCTAssertNil(update.chain[0].target.insets)
        XCTAssertNil(update.chain[0].target.frame)
        XCTAssertNil(update.chrome)
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

    /// A controller in a tab controller reads that controller's content guide as `tab-content` (D9): the
    /// guide's edges' distances from the tab controller's view, which clear its tab bar, under the guide's
    /// identity and with that view's frame in its window.
    func testATabControllersContentGuideIsTheTabContentTarget() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("UIKit has the tab content guide from iOS 26") }
        let scene = hostedWindowScene()
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        window.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        let tabs = UITabBarController(), inner = UIViewController()
        tabs.viewControllers = [UINavigationController(rootViewController: inner)]
        window.rootViewController = tabs
        window.makeKeyAndVisible()
        self.window = window
        window.layoutIfNeeded()
        let update = OwnerUpdateIOS.sample(controller: inner, view: inner.view, session: nil)
        let content = try XCTUnwrap(update.relations.first { $0.relation == OwnerUpdateIOS.Relation.tabContent }?.target)
        let guide = tabs.contentLayoutGuide
        XCTAssertEqual(content.id, TargetIds.id(of: guide))
        XCTAssertEqual(content.frame, window.convert(tabs.view.bounds, from: tabs.view))
        let insets = try XCTUnwrap(content.insets)
        let (frame, bounds) = (guide.layoutFrame, tabs.view.bounds)
        XCTAssertEqual(insets, UIEdgeInsets(top: frame.minY, left: frame.minX, bottom: bounds.maxY - frame.maxY, right: bounds.maxX - frame.maxX))
        // It clears the tab bar, above the tab view's own safe bottom
        XCTAssertGreaterThan(insets.bottom, tabs.view.safeAreaInsets.bottom, "guide \(frame), tab bar \(tabs.tabBar.frame)")
        // And the chrome says whether the tab controller's sidebar is available
        if #available(iOS 27.0, *) { XCTAssertEqual(update.chrome?.sidebarAvailable, tabs.sidebar.isAvailable) }
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
        XCTAssertNil(update.relations.first { $0.relation == OwnerUpdateIOS.Relation.tabContent }, "no tab controller contains it")
        // Its chrome: no sidebar outside a tab controller, and the vertical bar's edge from iOS 27.1,
        // unspecified where the device places none
        XCTAssertNil(update.chrome?.sidebarAvailable)
        if #available(iOS 27.1, *) {
            XCTAssertEqual(update.chrome?.verticalBarEdge == 0, !inner.view.traitCollection.placesVerticalBar)
        } else {
            XCTAssertNil(try XCTUnwrap(update.chrome).verticalBarEdge)
        }
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
        update.relations = [.init(relation: OwnerUpdateIOS.Relation.window, target: .init(id: 7, generation: 1, insets: UIEdgeInsets(top: 8, left: 0, bottom: 0, right: 0),
                                                                                       frame: CGRect(x: 0, y: 0, width: 402, height: 874)))]
        update.margins = .init(root: UIEdgeInsets(top: 9, left: 12, bottom: 11, right: 10), plain: UIEdgeInsets(top: 8, left: 8, bottom: 8, right: 8),
                               minimum: UIEdgeInsets(top: 0, left: 16, bottom: 0, right: 16), readableWidth: 672)
        update.keyboard = .init(frame: CGRect(x: 0, y: 538, width: 402, height: 336), docked: true)
        update.barRegions = [nil, nil, .init(distance: 24, sides: (16, 8.5)), nil]
        update.chrome = .init(verticalBarEdge: 2, sidebarAvailable: false)
        var w = [UInt8]()
        func f(_ values: Float...) { for v in values { w += withUnsafeBytes(of: v.bitPattern.littleEndian, Array.init) } }
        func u32(_ v: UInt32) { w += withUnsafeBytes(of: v.littleEndian, Array.init) }
        func u64(_ v: UInt64) { w += withUnsafeBytes(of: v.littleEndian, Array.init) }
        w.append(1); f(1, 2, 3, 4)
        u32(0)
        u32(1); u64(5); u64(6); w.append(0); w.append(0); w.append(1 << 2)
        u32(1); w.append(3); u64(7); u64(1); w.append(1); f(8, 0, 0, 0); w.append(1); f(0, 0, 402, 874)
        w.append(1); f(9, 10, 11, 12); f(8, 8, 8, 8); f(0, 16, 0, 16); f(672)
        w.append(1); f(0, 538, 402, 336); w.append(1)
        w.append(0)
        w += [0, 0, 1]; f(24, 16, 8.5); w.append(0)
        w += [1, 1, 2, 1, 0]
        XCTAssertEqual([UInt8](update.encode()), w)
        // A registration: the name's length and bytes, then the target after a presence byte
        let registration = ExactNativeTarget.encode(name: "map", target: .init(id: 9, generation: 2, insets: UIEdgeInsets(top: 30, left: 0, bottom: 0, right: 0)))
        w = [3] + Array("map".utf8) + [1]
        u64(9); u64(2); w.append(1); f(30, 0, 0, 0); w.append(0)
        XCTAssertEqual([UInt8](registration), w)
    }

    /// An owner's keyboard is its window's, as UIKit reports it, converted into the owner's view, whether or not the
    /// owner holds the editor; the session's view installs the window's sampler with its first keyboard editor, and
    /// the keyboard goes from the update once it hides (LLP 1123 D7).
    func testAnOwnersKeyboardIsItsWindowsInItsView() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the software keyboard shows only for a scene window's editor") }
        let session = try fixture()
        let (route, view) = try firstRoute(session)
        let window = try XCTUnwrap(view.window), host = try XCTUnwrap(window.rootViewController)
        let field = UITextField(frame: CGRect(x: 20, y: 80, width: 180, height: 40))
        host.view.addSubview(field)
        session.presenter.keyboardPrompt = field
        var shown: CGRect?
        let observer = NotificationCenter.default.addObserver(forName: UIResponder.keyboardDidShowNotification, object: nil, queue: .main) { note in
            shown = (note.userInfo?[UIResponder.keyboardFrameEndUserInfoKey] as? NSValue)?.cgRectValue
        }
        defer { NotificationCenter.default.removeObserver(observer) }
        XCTAssertTrue(field.becomeFirstResponder())
        until("the keyboard shows and is sampled") { shown != nil && KeyboardSampleIOS.existing(in: window)?.sample != nil }
        let keys = window.convert(try XCTUnwrap(shown), from: window.screen.coordinateSpace)
        let origin = view.bounds.origin
        let keyboard = try XCTUnwrap(OwnerUpdateIOS.sample(controller: route, view: view, session: sessionView).keyboard)
        XCTAssertTrue(keyboard.docked)
        XCTAssertEqual(keyboard.frame.minY, view.convert(keys, from: window).minY - origin.y, accuracy: 0.5, "the route holds no editor and is told the keyboard")
        XCTAssertEqual(keyboard.frame.height, keys.height, accuracy: 0.5)
        field.resignFirstResponder()
        until("the keyboard goes from the update") { OwnerUpdateIOS.sample(controller: route, view: view, session: sessionView).keyboard == nil }
    }

    /// A sheet scaled behind the sheet over it reads the window's keyboard converted into its own view, where its own
    /// guide would end short of the keyboard (LLP 1123 D7, measured on the iPhone 18 Pro).
    func testAScaledSheetsKeyboardIsTheWindowsInItsView() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the software keyboard shows only for a scene window's editor") }
        let window = UIWindow(windowScene: scene)
        window.frame = scene.coordinateSpace.bounds
        let root = UIViewController(), under = UIViewController(), over = UIViewController()
        window.rootViewController = root
        window.makeKeyAndVisible()
        self.window = window
        KeyboardSampleIOS.install(in: window)
        root.present(under, animated: false)
        under.present(over, animated: false)
        until("the lower sheet is scaled behind the upper") { under.view.window != nil && over.view.window != nil && under.view.convert(under.view.bounds, to: window).width < window.bounds.width - 1 }
        let field = UITextField(frame: CGRect(x: 20, y: 80, width: 180, height: 40))
        over.view.addSubview(field)
        var shown: CGRect?
        let observer = NotificationCenter.default.addObserver(forName: UIResponder.keyboardDidShowNotification, object: nil, queue: .main) { note in
            shown = (note.userInfo?[UIResponder.keyboardFrameEndUserInfoKey] as? NSValue)?.cgRectValue
        }
        defer {
            NotificationCenter.default.removeObserver(observer)
            field.resignFirstResponder()
            root.dismiss(animated: false)
        }
        XCTAssertTrue(field.becomeFirstResponder())
        until("the keyboard shows and is sampled") { shown != nil && KeyboardSampleIOS.existing(in: window)?.sample != nil }
        let keys = window.convert(try XCTUnwrap(shown), from: window.screen.coordinateSpace)
        let keyboard = try XCTUnwrap(OwnerUpdateIOS.sample(controller: under, view: under.view, session: nil).keyboard)
        let expected = under.view.convert(keys, from: window).offsetBy(dx: -under.view.bounds.minX, dy: -under.view.bounds.minY)
        XCTAssertEqual(keyboard.frame.minY, expected.minY, accuracy: 0.5)
        XCTAssertEqual(keyboard.frame.height, expected.height, accuracy: 0.5)
    }

    private func until(_ what: String, _ seconds: Double = 5, _ done: () -> Bool) {
        let deadline = Date().addingTimeInterval(seconds)
        while !done(), Date() < deadline { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.02)) }
        XCTAssertTrue(done(), what)
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
