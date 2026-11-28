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
        // Each target's frame is its bounds in its window, the window's its own
        XCTAssertEqual(first.target.frame, view.convert(view.bounds, to: nil))
        // And the window map takes a point of the owner's block, from its origin, where UIKit puts it
        let map = try XCTUnwrap(update.windowMap)
        let point = CGPoint(x: 30, y: 50), origin = view.bounds.origin
        let mapped = point.applying(map), converted = view.convert(CGPoint(x: origin.x + 30, y: origin.y + 50), to: nil)
        XCTAssertEqual(mapped.x, converted.x, accuracy: 0.01)
        XCTAssertEqual(mapped.y, converted.y, accuracy: 0.01)
        let navigation = try XCTUnwrap(route.navigationController?.view)
        let named = update.chain.first { $0.kinds & OwnerUpdateIOS.Kind.navigation != 0 }
        XCTAssertEqual(named?.target.insets, navigation.safeAreaInsets)
        let window = try XCTUnwrap(view.window)
        let windowTarget = update.relations.first { $0.relation == OwnerUpdateIOS.Relation.window }
        XCTAssertEqual(windowTarget?.target.insets, window.safeAreaInsets)
        XCTAssertEqual(windowTarget?.target.frame, window.bounds)
        // Its reserved regions from iOS 27.1, each keeping its identity from one sample to the next
        if #available(iOS 27.1, *) {
            let regions = try XCTUnwrap(update.regions)
            let again = OwnerUpdateIOS.sample(controller: route, view: view, session: sessionView).regions
            XCTAssertEqual(again?.map(\.id), regions.map(\.id))
            // The divisions among them are the view's division regions, from its bounds' origin
            let origin = view.bounds.origin
            let divisions = (ReservedRegions.divisions(of: view) ?? []).map { $0.frame.offsetBy(dx: -origin.x, dy: -origin.y) }
            XCTAssertEqual(regions.filter { $0.kind == 0 }.map(\.frame), divisions)
        } else {
            XCTAssertNil(update.regions)
        }
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

    /// A session made with stage 5's owner layout lays out under it (LLP 1123, "stage 5, the switch"): an edge cover
    /// on its route's root, an owner by its block, pads nothing, where in a session made without it the cover moves
    /// the root's children down, as on main.
    func testASessionMadeWithTheOwnerLayoutLaysOutUnderIt() throws {
        var moved: [[UInt32]] = []
        for on in [false, true] {
            let session = try fixture(ownerLayout: on)
            XCTAssertEqual(session.ownerLayout, on)
            let (route, _) = try firstRoute(session)
            let children = Set(route.node.subviews.compactMap { ($0 as? NodeView)?.id })
            let batch = session.runtime.covers([(route.node.id, .edges(.init(top: 50, right: 0, bottom: 0, left: 0)))])
            XCTAssertNil(batch.error)
            moved.append(batch.ops.filter { $0.op == .frame && children.contains($0.id) }.map(\.id))
            session.destroy()
            sessions.removeAll { $0 === session }
            window?.isHidden = true
        }
        XCTAssertFalse(moved[0].isEmpty, "the cover moves the root's children")
        XCTAssertEqual(moved[1], [], "the owner's root takes no padding from its cover")
    }

    /// The session's update is the session view's in the document's space (LLP 1123 D2): its band is the safe area
    /// where the document takes the whole view, as under the fixture's native container; its chain starts at the
    /// controller holding the session view, the window's root a relation; the keyboard and the regions are where a
    /// person sees them, the page scrolled or not, their margins scaled with the viewport; it reports no window map; and
    /// the kernel takes it.
    func testTheSessionsUpdateIsTheSessionViewsInTheDocument() throws {
        let session = try fixture()
        let view = try XCTUnwrap(sessionView), p = session.presenter
        let edges = { (i: UIEdgeInsets) in OwnerUpdateWire.Edges(top: i.top, right: i.right, bottom: i.bottom, left: i.left) }
        // A safe area with no two edges alike, whatever the window gives
        let host = try XCTUnwrap(window?.rootViewController)
        host.additionalSafeAreaInsets = UIEdgeInsets(top: 40, left: 12, bottom: 30, right: 8)
        host.view.layoutIfNeeded()
        view.layoutIfNeeded()
        XCTAssertEqual(p.viewport.convert(p.viewport.bounds.inset(by: p.pageMargins), to: view), view.bounds, "the document takes the whole view")
        let update = p.sessionUpdate(of: view)
        XCTAssertGreaterThanOrEqual(view.safeAreaInsets.top, 40)
        XCTAssertEqual(update.band, edges(view.safeAreaInsets))
        XCTAssertEqual(update.chain.first?.target.id, TargetIds.id(of: host.view), "the hosting controller's view")
        XCTAssertEqual(update.relations.first { $0.relation == OwnerUpdateIOS.Relation.session }?.target.id, TargetIds.id(of: view))
        // The page names no `interactive-widget`: UIKit's own behavior, sent as `resizes-content`
        XCTAssertEqual(p.keyboardPolicy, "resizes-content")
        XCTAssertEqual(update.keyboardPolicy, .resizesContent)
        XCTAssertNotNil(update.configuration)
        XCTAssertNil(session.runtime.ownerReports(session: update.encode()).error)
        XCTAssertNil(update.windowMap)
        // The conversion the keyboard and the regions take puts the viewport's visible origin at the document's, the
        // page scrolled or not
        for offset in [CGFloat(0), 30] {
            p.viewport.contentOffset.y = offset
            XCTAssertEqual(p.viewport.bounds.minY, offset, "the page scrolled")
            let seen = p.documentRect(CGRect(origin: p.viewport.convert(p.viewport.bounds.origin, to: nil), size: CGSize(width: 10, height: 10)), from: nil)
            XCTAssertEqual(seen.origin.x, -p.pageMargins.left, accuracy: 0.01)
            XCTAssertEqual(seen.origin.y, -p.pageMargins.top, accuracy: 0.01)
        }
        p.viewport.contentOffset.y = 0
        // A region's frame and margins scale with the viewport: under the agent's explicit size at half scale, twice
        // the session view's points, its four margins unalike
        let region = ReservedRegions.Region(id: 1, kind: 1, frame: CGRect(x: 40, y: 100, width: 60, height: 20),
                                            margins: UIEdgeInsets(top: 2, left: 4, bottom: 6, right: 8), active: true)
        XCTAssertEqual(p.documentRegion(region, of: view).margins, .init(top: 2, right: 8, bottom: 6, left: 4))
        p.viewport.transform = CGAffineTransform(scaleX: 0.5, y: 0.5)
        let scaled = p.documentRegion(region, of: view)
        p.viewport.transform = .identity
        XCTAssertEqual(scaled.frame.size, CGSize(width: 120, height: 40))
        XCTAssertEqual(scaled.margins, .init(top: 4, right: 16, bottom: 12, left: 8))
        // A controller between the window's root and the session view holds it: its view begins the chain
        let child = UIViewController()
        host.addChild(child)
        host.view.addSubview(child.view)
        child.didMove(toParent: host)
        child.view.addSubview(view)
        let held = p.sessionUpdate(of: view)
        XCTAssertEqual(held.chain.first?.target.id, TargetIds.id(of: child.view))
        XCTAssertEqual(held.chain.dropFirst().first?.target.id, TargetIds.id(of: host.view))
        XCTAssertEqual(held.relations.first { $0.relation == OwnerUpdateIOS.Relation.root }?.target.id, TargetIds.id(of: host.view))
        host.view.addSubview(view)
        // A view out of a window with no controller: no keyboard, configuration or regions, no chain, and the session
        // view still a relation
        let alone = p.sessionUpdate(of: UIView())
        XCTAssertEqual(alone.keyboard, .unreported)
        XCTAssertNil(alone.configuration)
        XCTAssertNil(alone.regions)
        XCTAssertTrue(alone.chain.isEmpty)
        XCTAssertEqual(alone.relations.map(\.relation), [OwnerUpdateIOS.Relation.session])
    }

    /// The band is how far the unsafe area reaches into the document: the safe area where the document takes the whole
    /// view, none where it is the safe area, and what a scaled document's own space holds of it.
    func testTheSessionsBandIsTheUnsafeAreaInTheDocument() {
        let whole = CGRect(x: 0, y: 0, width: 400, height: 800), safe = whole.inset(by: UIEdgeInsets(top: 40, left: 12, bottom: 30, right: 8))
        XCTAssertEqual(Presenter.band(document: whole, safe: safe), .init(top: 40, right: 8, bottom: 30, left: 12))
        XCTAssertEqual(Presenter.band(document: safe, safe: safe), .zero)
        // The agent's explicit size at half scale: the same safe area holds twice the document's points
        let scaled = CGRect(x: 0, y: 0, width: 800, height: 1600), safeScaled = scaled.inset(by: UIEdgeInsets(top: 80, left: 24, bottom: 60, right: 16))
        XCTAssertEqual(Presenter.band(document: scaled, safe: safeScaled), .init(top: 80, right: 16, bottom: 60, left: 24))
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

    /// The session's update encodes as the kernel does (`a_session_update_encodes_to_the_hosts_golden_bytes` pins the
    /// kernel's bytes for the same update, every field present and no two edges alike), rides the reports' records as
    /// kind 29 after owners' updates and registrations, and is taken, refused truncated or followed by a byte.
    func testTheSessionsUpdateEncodesAsTheKernelsAndRidesTheReports() throws {
        let target = { (id: UInt64, generation: UInt64, insets: OwnerUpdateWire.Edges?) in
            OwnerUpdateWire.Target(id: id, generation: generation, insets: insets, frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        }
        var update = SessionUpdateWire()
        update.band = .init(top: 59, right: 1, bottom: 34, left: 2)
        update.chain = [(target(11, 2, .init(top: 59, right: 0, bottom: 34, left: 0)), 4)] // `Kind::Navigation`
        update.relations = [(3, target(12, 1, nil))] // `Relation::Window`
        update.keyboard = .shown(frame: CGRect(x: 0, y: 516, width: 402, height: 358), docked: true)
        update.keyboardPolicy = .resizesVisual
        update.windowMap = CGAffineTransform(a: 1, b: 0, c: 0, d: 1, tx: 0, ty: 20)
        update.configuration = 7
        update.regions = [.init(id: 5, kind: 1, frame: CGRect(x: 10, y: 20, width: 30, height: 40), margins: .init(top: 1, right: 2, bottom: 3, left: 4), active: true)]
        let bytes = update.encode()
        XCTAssertEqual(bytes.map { String(format: "%02x", $0) }.joined(), "00006c420000803f0000084200000040010000000b0000000000000002000000000000000100006c420000000000000842000000000100000000000000000000c94300805a440401000000030c000000000000000100000000000000000100000000000000000000c94300805a440200000000000001440000c9430000b3430102010000803f00000000000000000000803f000000000000a0410107000000000000000101000000050000000000000001000020410000a0410000f041000020420000803f00000040000040400000804001")
        // Its record follows the owners' updates and the registrations, each framed by its view, kind and length
        let owner = Data([1, 2, 3]), registration = Data([4, 5])
        let records = Runtime.reportRecords(updates: [(7, owner)], registrations: [registration], session: bytes)
        let words = { (data: Data) in (0..<data.count / 4).map { i in data.withUnsafeBytes { UInt32(littleEndian: $0.loadUnaligned(fromByteOffset: i * 4, as: UInt32.self)) } } }
        XCTAssertEqual(words(records.prefix(12)), [7, 21, 3])
        XCTAssertEqual(records.subdata(in: 12..<15), owner)
        XCTAssertEqual(words(records.subdata(in: 15..<27)), [0, 22, 2])
        XCTAssertEqual(records.subdata(in: 27..<29), registration)
        XCTAssertEqual(words(records.subdata(in: 29..<41)), [0, 29, UInt32(bytes.count)])
        XCTAssertEqual(records.suffix(from: 41), bytes)
        let session = try fixture()
        XCTAssertNil(session.runtime.ownerReports(session: bytes).error)
        XCTAssertNotNil(session.runtime.ownerReports(session: bytes.prefix(bytes.count - 1)).error, "a truncated update")
        XCTAssertNotNil(session.runtime.ownerReports(session: bytes + Data([0])).error, "an update followed by a byte")
        XCTAssertNil(session.runtime.ownerReports(session: SessionUpdateWire().encode()).error, "no optional field")
        // And beside an owner's update and a registration, in one call the kernel takes whole
        let (route, view) = try firstRoute(session)
        let margins = OwnerUpdateIOS(margins: OwnerUpdateIOS.sample(controller: route, view: view, session: sessionView).margins).encode()
        let map = ExactNativeTarget.encode(name: "map", target: target(21, 1, .init(top: 30, right: 0, bottom: 0, left: 0)))
        XCTAssertNil(session.runtime.ownerReports(updates: [(route.node.id, margins)], registrations: [map], session: bytes).error)
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
        let target = OwnerUpdateIOS.wire(OwnerUpdateIOS.target(window))
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
        XCTAssertNil(update.windowMap)
        XCTAssertNil(update.configuration)
        XCTAssertNil(update.regions)
        XCTAssertEqual(update.keyboard, .unreported)
    }

    /// D8 and D11: UIKit moves the root's margins one for one with Exact's contribution, which the update takes off, as
    /// it does the band's.
    func testTheRootsMarginsAreTheirsLessExactsContribution() throws {
        let session = try fixture()
        let (route, _) = try firstRoute(session)
        let root = try XCTUnwrap(window?.rootViewController)
        let before = try XCTUnwrap(OwnerUpdateIOS.sample(controller: root, view: root.view, session: nil).margins).root
        let given = root.view.layoutMargins
        let ledger = ContributionsIOS.of(root)
        let identity = ContributionsIOS.Identity(node: route.node.id, operation: .additionalInsets)
        ledger.set(identity, UIEdgeInsets(top: 0, left: 20, bottom: 10, right: 0), generation: 1)
        let update = OwnerUpdateIOS.sample(controller: root, view: root.view, session: nil)
        XCTAssertEqual(root.view.layoutMargins.left, given.left + 20, accuracy: 0.5, "UIKit moves the margin with the contribution")
        XCTAssertEqual(root.view.layoutMargins.bottom, given.bottom + 10, accuracy: 0.5)
        let after = try XCTUnwrap(update.margins).root
        XCTAssertEqual(after.left, before.left, accuracy: 0.5, "the update takes it off")
        XCTAssertEqual(after.bottom, before.bottom, accuracy: 0.5)
        XCTAssertEqual(after.top, before.top, accuracy: 0.5)
        ledger.set(identity, nil, generation: 2)
        ledger.apply()
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

    /// A scrolled owner's window map starts at its bounds' origin, where its block's space does, so the
    /// block's top left maps to the view's top left in its window whatever it scrolled to.
    func testAScrolledOwnersWindowMapStartsAtItsBoundsOrigin() throws {
        let scene = hostedWindowScene()
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        let controller = UIViewController(), scroll = UIScrollView(frame: CGRect(x: 10, y: 80, width: 300, height: 400))
        scroll.contentInsetAdjustmentBehavior = .never
        scroll.contentSize = CGSize(width: 300, height: 2000)
        controller.view.addSubview(scroll)
        window.rootViewController = controller
        window.makeKeyAndVisible()
        self.window = window
        scroll.contentOffset = CGPoint(x: 0, y: 100)
        let map = try XCTUnwrap(OwnerUpdateIOS.sample(controller: controller, view: scroll, session: nil).windowMap)
        let top = CGPoint.zero.applying(map), frame = scroll.convert(scroll.bounds, to: nil)
        XCTAssertEqual(top.x, frame.minX, accuracy: 0.01)
        XCTAssertEqual(top.y, frame.minY, accuracy: 0.01)
    }

    /// A route's configuration stays while nothing of it changes and moves when its navigation bar
    /// hides, coming back when the bar does (D9, "Moving edges").
    func testARoutesConfigurationMovesWithItsBars() throws {
        let session = try fixture()
        let (route, view) = try firstRoute(session)
        let navigation = try XCTUnwrap(route.navigationController)
        let sample = { OwnerUpdateIOS.sample(controller: route, view: view, session: self.sessionView).configuration }
        let first = try XCTUnwrap(sample())
        XCTAssertEqual(sample(), first)
        navigation.setNavigationBarHidden(!navigation.isNavigationBarHidden, animated: false)
        XCTAssertNotEqual(sample(), first)
        navigation.setNavigationBarHidden(!navigation.isNavigationBarHidden, animated: false)
        XCTAssertEqual(sample(), first)
    }

    /// A tab's configuration moves when its tab bar hides through `setTabBarHidden`, which on iPadOS's
    /// floating tab bar leaves `tabBar.isHidden` as it was (D9, "Moving edges").
    func testATabsConfigurationMovesWithItsTabBar() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a configuration is a window's") }
        guard #available(iOS 18, *) else { throw XCTSkip("setTabBarHidden is iOS 18's") }
        let window = UIWindow(windowScene: scene)
        window.frame = scene.coordinateSpace.bounds
        let tab = UIViewController(), tabs = UITabBarController()
        tab.tabBarItem = UITabBarItem(title: "One", image: nil, tag: 0)
        tabs.viewControllers = [tab, UIViewController()]
        window.rootViewController = tabs
        window.makeKeyAndVisible()
        self.window = window
        tabs.view.layoutIfNeeded()
        let sample = { OwnerUpdateIOS.sample(controller: tab, view: tab.view, session: nil).configuration }
        let first = try XCTUnwrap(sample())
        tabs.setTabBarHidden(true, animated: false)
        XCTAssertNotEqual(sample(), first, "hidden, tabBar.isHidden \(tabs.tabBar.isHidden)")
        tabs.setTabBarHidden(false, animated: false)
        XCTAssertEqual(sample(), first)
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
        update.keyboard = .shown(.init(frame: CGRect(x: 0, y: 538, width: 402, height: 336), docked: true))
        update.barRegions = [nil, nil, .init(distance: 24, sides: (16, 8.5)), nil]
        update.chrome = .init(verticalBarEdge: 2, sidebarAvailable: false)
        update.windowMap = CGAffineTransform(a: 0.9, b: 0.1, c: 0.2, d: 0.8, tx: 5, ty: 62)
        update.configuration = 7
        update.regions = [.init(id: 11, kind: 0, frame: CGRect(x: 175, y: 0, width: 40, height: 844), margins: UIEdgeInsets(top: 0, left: 10, bottom: 0, right: 10), active: false)]
        var w = [UInt8]()
        func f(_ values: Float...) { for v in values { w += withUnsafeBytes(of: v.bitPattern.littleEndian, Array.init) } }
        func u32(_ v: UInt32) { w += withUnsafeBytes(of: v.littleEndian, Array.init) }
        func u64(_ v: UInt64) { w += withUnsafeBytes(of: v.littleEndian, Array.init) }
        w.append(1); f(1, 2, 3, 4)
        u32(0)
        u32(1); u64(5); u64(6); w.append(0); w.append(0); w.append(1 << 2)
        u32(1); w.append(3); u64(7); u64(1); w.append(1); f(8, 0, 0, 0); w.append(1); f(0, 0, 402, 874)
        w.append(1); f(9, 10, 11, 12); f(8, 8, 8, 8); w.append(1); f(0, 16, 0, 16); w.append(1); f(672)
        w.append(2); f(0, 538, 402, 336); w.append(1)
        w.append(0)
        w += [0, 0, 1]; f(24, 16, 8.5); w.append(0)
        w += [1, 1, 2, 1, 0, 0, 0]
        w.append(1); f(0.9, 0.1, 0.2, 0.8, 5, 62)
        w.append(1); u64(7)
        w.append(1); u32(1); u64(11); w.append(0); f(175, 0, 40, 844); f(0, 10, 0, 10); w.append(0)
        XCTAssertEqual([UInt8](update.encode()), w)
        // The keyboard's state, after an empty update's band, lists and margins: 0 unreported, 1 dismissed
        XCTAssertEqual([UInt8](OwnerUpdateIOS().encode())[14], 0)
        XCTAssertEqual([UInt8](OwnerUpdateIOS(keyboard: .dismissed).encode())[14], 1)
        // A registration: the name's length and bytes, then the target after a presence byte
        let registration = ExactNativeTarget.encode(name: "map", target: .init(id: 9, generation: 2, insets: .init(top: 30, right: 0, bottom: 0, left: 0), frame: nil))
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
        let keyboard = try XCTUnwrap(OwnerUpdateIOS.sample(controller: route, view: view, session: sessionView).keyboard.shown)
        XCTAssertTrue(keyboard.docked)
        XCTAssertEqual(keyboard.frame.minY, view.convert(keys, from: window).minY - origin.y, accuracy: 0.5, "the route holds no editor and is told the keyboard")
        XCTAssertEqual(keyboard.frame.height, keys.height, accuracy: 0.5)
        field.resignFirstResponder()
        until("the update says the keyboard is dismissed") { OwnerUpdateIOS.sample(controller: route, view: view, session: sessionView).keyboard == .dismissed }
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
        let keyboard = try XCTUnwrap(OwnerUpdateIOS.sample(controller: under, view: under.view, session: nil).keyboard.shown)
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
    private func fixture(ownerLayout: Bool = false) throws -> ExactSession {
        let env = ProcessInfo.processInfo.environment
        let path = try XCTUnwrap(env["EXACT_FIXTURE_PLAN"], "build.mjs --test --ios compiles EXACT_FIXTURE_PLAN")
        let plan = try Data(contentsOf: URL(fileURLWithPath: path))
        let session = ExactApp.shared.makeSession(label: "owner-update", ownerLayout: ownerLayout)
        sessions.append(session)
        let view = ExactView(session: session)
        sessionView = view
        let host = UIViewController()
        let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene
        // A scene's window fills its scene, as an app's does, so the keyboard's guides end where the screen does
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        let size = window.bounds.size
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
