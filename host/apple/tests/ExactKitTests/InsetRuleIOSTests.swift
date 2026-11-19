#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

/// The inset rule (`kernel/src/kernel/context.rs`) on a device, over
/// `scripts/fixtures/inset-rule-cover.contract` (a `cover` root's full-screen
/// scroller) and `inset-rule-route.contract` (a route and a sheet): each case
/// of the safe-area design's 2.1 to 2.3. The host clears each band once, so a
/// reading of `env()` or `var(--exact-safe-area-inset-*)` adds only what the
/// author writes past it. Each case's geometry is logged as a `MEASURE` line,
/// appended to the file `EXACT_INSET_MEASURE` names when it is set, before it
/// is checked.
final class InsetRuleIOSTests: XCTestCase {
    private var window: UIWindow?
    private var sessions: [ExactSession] = []

    override func tearDown() {
        for session in sessions { session.destroy() }
        sessions = []
        window?.isHidden = true
        window = nil
        super.tearDown()
    }

    /// Ex. 8 to 11, measurement 1: the first row rests at the band's depth whatever reads it, and an author's 16
    /// adds to the inset.
    func testACoverScrollersPaddingAndContentReadWhatTheScrollerLeaves() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the scroller's inset follows the window's safe area") }
        for name in ["plain", "env", "var", "max", "ignore", "ignore-max", "child-env", "short"] {
            let (session, window) = try cover(scene, name)
            let sv = try XCTUnwrap(node(session, "scroller")?.scrollView)
            let band = window.safeAreaInsets.top, first = try top(session, "first")
            log("cover \(name) band=\(f(band)) adjusted=\(f(sv.adjustedContentInset.top)) inset=\(f(sv.contentInset.top)) offset=\(f(sv.contentOffset.y)) first=\(f(first))")
            let want: CGFloat = switch name {
            case "max": band + 16
            case "ignore-max": max(16, band)
            default: band
            }
            XCTAssertEqual(first, want, accuracy: 0.5, "\(name): the first row")
            XCTAssertEqual(sv.adjustedContentInset.top, name.hasPrefix("ignore") ? 0 : band, accuracy: 0.5, "\(name): the scroller's inset")
            XCTAssertEqual(sv.contentOffset.y, -sv.adjustedContentInset.top, accuracy: 0.5, "\(name): at rest")
        }
    }

    /// Ex. 13: beside a 50-point bar over the bottom band, `calc(50px + env(safe-area-inset-bottom))` on the
    /// scroller is 50, so the last row rests the bar's height above it.
    func testABarsPaddingOnTheScrollerAddsTheBarOnly() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the scroller's inset follows the window's safe area") }
        let (session, window) = try cover(scene, "bar")
        let sv = try XCTUnwrap(node(session, "scroller")?.scrollView)
        sv.setContentOffset(CGPoint(x: sv.contentOffset.x, y: sv.contentSize.height + sv.adjustedContentInset.bottom - sv.bounds.height), animated: false)
        settle(session)
        let last = try XCTUnwrap(node(session, "last")), bar = try XCTUnwrap(node(session, "bar"))
        let band = window.safeAreaInsets.bottom, above = window.bounds.maxY - onScreen(last).maxY
        log("cover bar band=\(f(band)) adjusted=\(f(sv.adjustedContentInset.bottom)) inset=\(f(sv.contentInset.bottom)) bar=\(f(bar.bounds.height)) last-above-bottom=\(f(above))")
        XCTAssertEqual(above, band + 50, accuracy: 0.5)
    }

    /// Measurement 4: a sticky box sticks at the band's inner side, `top: 0` and `top: env(safe-area-inset-top)`
    /// alike.
    func testAStickyBoxSticksBelowTheBand() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the scroller's inset follows the window's safe area") }
        for name in ["sticky", "sticky-env"] {
            let (session, window) = try cover(scene, name)
            let sv = try XCTUnwrap(node(session, "scroller")?.scrollView)
            sv.setContentOffset(CGPoint(x: sv.contentOffset.x, y: sv.contentOffset.y + 300), animated: false)
            settle(session)
            let band = window.safeAreaInsets.top, stuck = onScreen(try XCTUnwrap(node(session, "sticky"))).minY
            log("cover \(name) band=\(f(band)) adjusted=\(f(sv.adjustedContentInset.top)) stuck=\(f(stuck))")
            XCTAssertEqual(stuck, band, accuracy: 0.5, "\(name)")
        }
    }

    /// Measurement 3: in landscape UIKit insets a vertical scroller's axis only, so content that pads the side
    /// bands by `env()` keeps off them once, and no sideways range appears.
    func testAVerticalScrollerInLandscapeKeepsItsContentOffTheSidesOnce() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the scroller's inset follows the window's safe area") }
        let was = scene.interfaceOrientation
        defer { scene.requestGeometryUpdate(.iOS(interfaceOrientations: was.isLandscape ? .landscapeLeft : .portrait)) }
        scene.requestGeometryUpdate(.iOS(interfaceOrientations: .landscapeLeft))
        waitFor("the scene rotates") { scene.interfaceOrientation.isLandscape }
        for name in ["plain", "sides"] {
            let (session, window) = try cover(scene, name)
            let sv = try XCTUnwrap(node(session, "scroller")?.scrollView)
            let first = onScreen(try XCTUnwrap(node(session, "first"))), safe = window.safeAreaInsets
            let range = sv.contentSize.width + sv.adjustedContentInset.left + sv.adjustedContentInset.right - sv.bounds.width
            log("landscape \(name) safe=\(f(safe.left)),\(f(safe.right)) adjusted=\(f(sv.adjustedContentInset.left)),\(f(sv.adjustedContentInset.right)) inset=\(f(sv.contentInset.left)),\(f(sv.contentInset.right)) offset-x=\(f(sv.contentOffset.x)) first=\(f(first.minX))..\(f(first.maxX)) width=\(f(window.bounds.width)) sideways=\(f(range))")
            XCTAssertLessThanOrEqual(range, 0.5, "\(name): no sideways scroll range")
            guard name == "sides" else { continue }
            XCTAssertEqual(first.minX, safe.left, accuracy: 0.5, "the content's leading edge")
            XCTAssertEqual(first.maxX, window.bounds.width - safe.right, accuracy: 0.5, "its trailing edge")
        }
    }

    /// Ex. 1 to 4, 6 and 7, measurement 2: in a route `env()` reads what the route's root leaves. `root-env` and
    /// `root-max` here follow a press that takes the route's bar away.
    func testARoutesContentReadsWhatItsRootLeaves() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a route's safe area requires UIKit containment") }
        for name in ["list", "row-var", "row-env", "barless", "root-env", "root-max", "box-max", "ignore-max", "two"] {
            let (session, controller) = try route(scene, name)
            let band = controller.view.safeAreaInsets.top
            let sv = try XCTUnwrap(node(session, "primary")?.scrollView), first = try top(session, "first")
            let frame = onScreen(sv)
            var line = "route \(name) band=\(f(band)) adjusted=\(f(sv.adjustedContentInset.top)) offset=\(f(sv.contentOffset.y)) scroller-top=\(f(frame.minY)) first=\(f(first))"
            switch name {
            case "barless":
                let bar = try top(session, "top-bar-content")
                line += " top-bar=\(f(bar))"
                log(line)
                XCTAssertEqual(bar, band, accuracy: 0.5, "the bar's content starts below the status band once")
                XCTAssertEqual(first, band + 52, accuracy: 0.5, "the list follows the 52-point bar")
            case "root-env", "root-max":
                log(line)
                XCTAssertEqual(first, name == "root-env" ? band : band + 16, accuracy: 0.5, "\(name): the root's padding adds to its clearance")
            case "box-max", "ignore-max":
                let box = try top(session, "box-content")
                line += " box=\(f(box))"
                log(line)
                XCTAssertEqual(box, name == "box-max" ? band + 16 : max(16, band), accuracy: 0.5, "\(name): the box's content")
            case "two":
                let second = try XCTUnwrap(node(session, "second")?.scrollView), secondFirst = try top(session, "second-first")
                line += " second-adjusted=\(f(second.adjustedContentInset.top)) second-first=\(f(secondFirst))"
                log(line)
                XCTAssertEqual(secondFirst, band, accuracy: 0.5, "the second scroller's first row")
                XCTAssertEqual(first, band, accuracy: 0.5, "the primary scroller's first row")
            default:
                log(line)
                XCTAssertEqual(first, band, accuracy: 0.5, "\(name): the first row")
            }
        }
    }

    /// Ex. 4's route as `expose` writes it: a route that opens barless and pads its root by `env()` clears the
    /// status band once, and an author's 16 adds to it.
    func testABarlessRootPaddedByTheInsetClearsOnce() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a route's safe area requires UIKit containment") }
        for name in ["root-env", "root-max"] {
            let (session, controller) = try route(scene, name, key: "INSET_RULE_ROOT", opens: "root-env")
            let band = controller.view.safeAreaInsets.top
            let sv = try XCTUnwrap(node(session, "primary")?.scrollView), first = try top(session, "first")
            log("barless \(name) band=\(f(band)) adjusted=\(f(sv.adjustedContentInset.top)) offset=\(f(sv.contentOffset.y)) scroller-top=\(f(onScreen(sv).minY)) first=\(f(first))")
            XCTAssertEqual(first, name == "root-env" ? band : band + 16, accuracy: 0.5, "\(name): the root's padding adds to its clearance")
        }
    }

    /// Measurement 2's collapsed half and 5: as the title collapses, the primary scroller's inset follows the
    /// band, and a box that pads `env(safe-area-inset-top)` in a root that ignores the top keeps its content at
    /// the band at each step, above the scroller (`ignore-env`) or over it (`ignore-over`).
    func testAnIgnoringRootsReadingFollowsACollapsingTitle() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a route's safe area requires UIKit containment") }
        for name in ["list", "ignore-env", "ignore-over"] {
            let (session, controller) = try route(scene, name)
            let sv = try XCTUnwrap(node(session, "primary")?.scrollView)
            let rest = sv.contentOffset.y
            var steps: [String] = []
            for step in stride(from: 0, through: 200, by: 20) {
                sv.setContentOffset(CGPoint(x: sv.contentOffset.x, y: rest + CGFloat(step)), animated: false)
                settle(session)
                let band = controller.view.safeAreaInsets.top
                let reading = try name == "list" ? sv.adjustedContentInset.top : top(session, "box-content")
                steps.append("\(step):\(f(band))/\(f(reading))")
                XCTAssertEqual(reading, band, accuracy: 0.5, "\(name) at \(step)")
            }
            log("collapse \(name) band/reading by scroll \(steps.joined(separator: " "))")
        }
    }

    /// Ex. 14: a sheet that fits 300 points of content and pads its root's bottom by `env()` is the content and
    /// the band below it, once.
    func testAFittingSheetClearsItsBandOnce() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a sheet requires UIKit containment") }
        let (session, _) = try route(scene, "list")
        let p = session.presenter
        _ = Agent(session: session).tap(["id": Int(try XCTUnwrap(node(session, "open")).id)])
        waitFor("the sheet appears") {
            guard let sheet = self.node(session, "sheet"), let c = p.navigationHost.controllers[sheet.id] else { return false }
            return c.viewIfLoaded?.window != nil && c.view.safeAreaInsets.bottom > 0 && !p.navigation.inTransition
        }
        settle(session)
        // In the sheet's own points: the presentation may scale it on screen
        let sheet = try XCTUnwrap(p.navigationHost.controllers[try XCTUnwrap(node(session, "sheet")).id]?.view)
        let box = try XCTUnwrap(node(session, "sheet-content"))
        let content = box.convert(box.bounds, to: sheet), band = sheet.safeAreaInsets.bottom
        let height = sheet.bounds.height, below = height - content.maxY
        log("sheet band=\(f(band)) height=\(f(height)) content=\(f(content.height)) below-content=\(f(below)) scale=\(f(onScreen(sheet).height / height))")
        XCTAssertEqual(below, band, accuracy: 0.5, "the band below the content, once")
        XCTAssertEqual(height, 300 + band, accuracy: 0.5)
    }

    // MARK: - Fixtures

    /// The cover fixture in `name`'s arrangement, settled.
    private func cover(_ scene: UIWindowScene, _ name: String) throws -> (ExactSession, UIWindow) {
        let session = try boot(scene, "INSET_RULE_COVER")
        if name != "plain" { try pick(session, name) }
        return (session, try XCTUnwrap(window))
    }

    /// The route fixture in `name`'s arrangement, settled, and the route's controller; `key`'s plan opens in
    /// `opens`.
    private func route(_ scene: UIWindowScene, _ name: String, key: String = "INSET_RULE_ROUTE", opens: String = "list") throws -> (ExactSession, UIViewController) {
        let session = try boot(scene, key)
        let p = session.presenter
        waitFor("the route appears") {
            guard let route = self.node(session, "route"), let c = p.navigationHost.controllers[route.id] else { return false }
            return c.viewIfLoaded?.window != nil && c.view.safeAreaInsets != .zero && !p.navigation.inTransition
        }
        if name != opens { try pick(session, name) }
        let controller = try XCTUnwrap(p.navigationHost.controllers[try XCTUnwrap(node(session, "route")).id])
        return (session, controller)
    }

    /// The fixture in a window of the test host's scene, the screen's size; the previous one's window goes.
    private func boot(_ scene: UIWindowScene, _ key: String) throws -> ExactSession {
        for session in sessions { session.destroy() }
        sessions = []
        window?.isHidden = true
        let session = ExactApp.shared.makeSession(label: "inset-rule")
        sessions.append(session)
        let view = ExactView(session: session)
        let host = UIViewController()
        let window = UIWindow(windowScene: scene)
        window.frame = scene.coordinateSpace.bounds
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        self.window = window
        XCTAssertNil(session.boot(plan: try fixturePlan(key), size: window.bounds.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        settle(session)
        return session
    }

    private func pick(_ session: ExactSession, _ name: String) throws {
        _ = Agent(session: session).tap(["id": Int(try XCTUnwrap(node(session, "pick-\(name)")).id)])
        settle(session)
    }

    /// Waits until the native layout has landed and every node's frame holds for a turn.
    private func settle(_ session: ExactSession) {
        waitFor("the native layout settles") { !Agent(session: session).nativeInFlight() }
        var last = ""
        waitFor("the frames hold") {
            RunLoop.main.run(until: Date().addingTimeInterval(0.05))
            let now = session.presenter.views.values.filter { $0.window != nil }.map { "\($0.id):\(self.onScreen($0))" }.sorted().joined()
            defer { last = now }
            return now == last
        }
    }

    private func node(_ session: ExactSession, _ testId: String) -> NodeView? {
        session.presenter.views.values.first { $0.props["testId"] == testId && $0.window != nil }
    }

    private func top(_ session: ExactSession, _ testId: String) throws -> CGFloat {
        onScreen(try XCTUnwrap(node(session, testId), testId)).minY
    }

    private func onScreen(_ view: UIView) -> CGRect { view.convert(view.bounds, to: nil) }

    private func f(_ x: CGFloat) -> String { String(format: "%.1f", x) }

    private func log(_ line: String) {
        let device = ProcessInfo.processInfo.environment["SIMULATOR_DEVICE_NAME"] ?? UIDevice.current.model
        let text = "MEASURE [\(device)] \(line)\n"
        print(text, terminator: "")
        guard let path = ProcessInfo.processInfo.environment["EXACT_INSET_MEASURE"] else { return }
        if let handle = FileHandle(forWritingAtPath: path) {
            handle.seekToEndOfFile()
            handle.write(Data(text.utf8))
            handle.closeFile()
        } else {
            FileManager.default.createFile(atPath: path, contents: Data(text.utf8))
        }
    }
}
#endif
