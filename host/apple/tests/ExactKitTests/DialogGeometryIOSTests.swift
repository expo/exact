#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

final class DialogGeometryIOSTests: XCTestCase {
    private var session: ExactSession?
    private var window: UIWindow?
    override func tearDown() {
        session?.destroy(); session = nil
        window?.isHidden = true; window = nil
        super.tearDown()
    }
    private func until(_ message: String, _ predicate: () -> Bool) {
        let end = Date(timeIntervalSinceNow: 5)
        while !predicate(), Date() < end { RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.02)) }
        XCTAssertTrue(predicate(), message)
    }
    private func slots(_ s: ExactSession) -> [String: Any] {
        let json = s.runtime.agent("{\"op\":\"state\"}")
        return ((try? JSONSerialization.jsonObject(with: Data(json.utf8))) as? [String: Any])?["slots"] as? [String: Any] ?? [:]
    }

    func testPointReadsIncludeThePresentedRoutesNativeBackdrop() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a route backdrop needs a scene's window") }
        let s = ExactApp.shared.makeSession(label: "route-backdrop")
        session = s
        let view = ExactView(session: s), host = UIViewController(), w = UIWindow(windowScene: scene)
        w.frame = w.screen.bounds; w.rootViewController = host; w.makeKeyAndVisible(); host.view.addSubview(view)
        window = w
        XCTAssertNil(s.boot(plan: try fixturePlan("ROUTE_BACKDROP"), size: w.bounds.size).error)
        view.frame = host.view.bounds; view.layoutIfNeeded()
        if s.firstDrawMs == nil { s.firstDrawMs = ExactEnv.wall() }
        let p = s.presenter, agent = Agent(session: s)
        defer { host.dismiss(animated: false) }
        let opener = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "open-sheet" })
        p.press(opener.id)
        until("the sheet settles") { p.modals.owner?.view.window != nil && !agent.nativeInFlight() }
        let owner = try XCTUnwrap(p.modals.owner)
        let container = try XCTUnwrap(owner.presentationController?.containerView)
        let outside = CGPoint(x: container.bounds.midX, y: container.bounds.minY + 5)
        XCTAssertFalse(owner.view.convert(owner.view.bounds, to: container).contains(outside))
        let delivered = try XCTUnwrap(w.hitTest(container.convert(outside, to: w), with: nil))
        XCTAssertTrue(delivered === container || delivered.isDescendant(of: container))
        XCTAssertFalse(delivered === owner.view || delivered.isDescendant(of: owner.view))
        func read(_ point: CGPoint, expected: String, file: StaticString = #filePath, line: UInt = #line) throws {
            let route = try XCTUnwrap(p.navigation.modalNavigation?.topViewController as? RouteController)
            let at = p.viewportPoints(container.convert(CGRect(origin: point, size: .zero), to: p.viewport)).origin
            let sample = PointerSample(x: 0, y: 0, buttons: 0, pressure: 0, type: "mouse", id: 1, clientX: at.x, clientY: at.y)
            s.apply(s.runtime.pointer(route.node.id, .move, sample, now: s.now()))
            let state = slots(s)
            XCTAssertEqual(state["hit"] as? String, expected, file: file, line: line)
            XCTAssertEqual(state["hits"] as? String, expected == "none" ? "" : expected, file: file, line: line)
        }
        try read(outside, expected: "sheet")
        let navigation = try XCTUnwrap(p.navigation.modalNavigation)
        let bar = navigation.navigationBar
        let chrome = bar.convert(CGPoint(x: bar.bounds.midX, y: bar.bounds.midY), to: container)
        let chromeHit = try XCTUnwrap(w.hitTest(container.convert(chrome, to: w), with: nil))
        XCTAssertTrue(chromeHit === bar || chromeHit.isDescendant(of: bar))
        let excluded = try XCTUnwrap(p.openTopLayer.backdrop?.2)
        let chromePoint = p.viewportPoints(container.convert(CGRect(origin: chrome, size: .zero), to: p.viewport)).origin
        XCTAssertTrue(excluded.contains(chromePoint))
        try read(chrome, expected: "none")
        func readDuringTransition(expected: String, bothLive: Bool) throws {
            until("both native route views are shown during the transition") {
                guard let transition = navigation.transitionCoordinator else { return false }
                return transition.viewController(forKey: .from)?.view.window != nil
                    && transition.viewController(forKey: .to)?.view.window != nil
                    && (!bothLive || p.openTopLayer.frames.count == 2)
            }
            let shown = p.openTopLayer
            let boundary = try XCTUnwrap(shown.modal), backdrop = try XCTUnwrap(shown.backdrop)
            if bothLive { XCTAssertNotEqual(shown.frames[boundary].0, backdrop.0, "the front route differs from the modal boundary") }
            let delivered = try XCTUnwrap(w.hitTest(container.convert(outside, to: w), with: nil))
            XCTAssertFalse(delivered === owner.view || delivered.isDescendant(of: owner.view))
            try read(outside, expected: expected)
        }
        let push = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "open-detail" })
        p.press(push.id)
        try readDuringTransition(expected: "detail", bothLive: true)
        until("the pushed route settles") { p.navigation.modalNavigation?.viewControllers.count == 2 && !agent.nativeInFlight() }
        try read(outside, expected: "detail")
        XCTAssertNotNil(navigation.popViewController(animated: true))
        try readDuringTransition(expected: "sheet", bothLive: false)
        until("the popped route settles") { navigation.viewControllers.count == 1 && !agent.nativeInFlight() }
        try read(outside, expected: "sheet")
        container.isUserInteractionEnabled = false
        XCTAssertNil(container.hitTest(outside, with: nil))
        try read(outside, expected: "none")
        container.isUserInteractionEnabled = true
        let beyond = CGPoint(x: container.bounds.maxX + 1, y: outside.y)
        XCTAssertNil(container.hitTest(beyond, with: nil))
        try read(beyond, expected: "none")
    }

    func testStructuralReadsFollowTheSheetsPresentationBetweenEndpoints() throws {
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else { throw missing("scene", "native sheet motion requires a scene") }
        let s = ExactApp.shared.makeSession(label: "dialog-animation-geometry")
        session = s
        let view = ExactView(session: s), host = UIViewController(), w = UIWindow(windowScene: scene)
        w.frame = w.screen.bounds; w.rootViewController = host; w.makeKeyAndVisible(); host.view.addSubview(view)
        window = w
        XCTAssertNil(s.boot(plan: try fixturePlan("DIALOG_SHEET_READS"), size: w.bounds.size).error)
        view.frame = host.view.bounds; view.layoutIfNeeded()
        if s.firstDrawMs == nil { s.firstDrawMs = ExactEnv.wall() }
        let p = s.presenter
        defer { p.menus.reset(); host.dismiss(animated: false) }
        p.dialogCommand("showModal", "form")
        until("the sheet appeared") { host.presentedViewController is DialogSheetController && !p.menus.inTransition }
        let sheet = try XCTUnwrap(host.presentedViewController as? DialogSheetController)
        let content = try XCTUnwrap(p.views.values.first { $0.props["id"] == "content" })
        let dialog = try XCTUnwrap(p.views.values.first { $0.props["id"] == "form" })
        until("native geometry rests") { !FrameClock.shared.wants(p.nativeGeometry) }
        let start = content.convert(content.bounds, to: p.viewport)
        var finished = false
        UIView.animate(withDuration: 0.8, delay: 0, options: [.curveLinear]) {
            sheet.view.transform = CGAffineTransform(translationX: 0, y: 160)
            sheet.view.setNeedsLayout(); sheet.view.layoutIfNeeded()
        } completion: { _ in finished = true }
        until("the sheet is between endpoints") {
            guard let layer = content.layer.presentation(), let port = p.viewport.layer.presentation() else { return false }
            let delta = layer.convert(layer.bounds, to: port).minY - start.minY
            return delta > 30 && delta < 120
        }
        let layer = try XCTUnwrap(content.layer.presentation()), port = try XCTUnwrap(p.viewport.layer.presentation())
        let visible = p.viewportPoints(layer.convert(layer.bounds, to: port))
        let destination = p.viewportPoints(content.convert(content.bounds, to: p.viewport))
        XCTAssertGreaterThan(abs(visible.minY - destination.minY), 30)
        let point = CGPoint(x: visible.midX, y: visible.midY)
        let sample = PointerSample(x: 0, y: 0, buttons: 0, pressure: 0, type: "mouse", id: 1, clientX: Double(point.x), clientY: Double(point.y))
        s.apply(s.runtime.pointer(dialog.id, .move, sample, now: s.now()))
        let state = slots(s), frame = (state["rect"] as? String ?? "").split(separator: ",").compactMap { Double($0) }
        XCTAssertEqual(frame.count, 4)
        if frame.count == 4 {
            for (index, pair) in zip(frame, [visible.minX, visible.minY, visible.width, visible.height]).enumerated() {
                XCTAssertEqual(pair.0, pair.1, accuracy: 2, "component \(index); read \(frame), visible \(visible), destination \(destination)")
            }
        }
        XCTAssertEqual(state["hit"] as? String, "content")
        XCTAssertTrue(FrameClock.shared.wants(p.nativeGeometry), "native movement owns geometry frames")
        until("the native animation ends") { finished }
        until("native geometry releases its frames") { !FrameClock.shared.wants(p.nativeGeometry) }
    }

    func testSheetResizeClipsPointReadsToItsVisibleHeight() throws {
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else { throw missing("scene", "native sheet resizing requires a scene") }
        let s = ExactApp.shared.makeSession(label: "dialog-resize-geometry")
        session = s
        let view = ExactView(session: s), host = UIViewController(), w = UIWindow(windowScene: scene)
        w.frame = w.screen.bounds; w.rootViewController = host; w.makeKeyAndVisible(); host.view.addSubview(view)
        window = w
        XCTAssertNil(s.boot(plan: try fixturePlan("DIALOG_SHEET_READS"), size: w.bounds.size).error)
        view.frame = host.view.bounds; view.layoutIfNeeded()
        if s.firstDrawMs == nil { s.firstDrawMs = ExactEnv.wall() }
        let p = s.presenter
        defer { p.menus.reset(); host.dismiss(animated: false) }
        p.dialogCommand("showModal", "form")
        until("the sheet appeared") { host.presentedViewController is DialogSheetController && !p.menus.inTransition }
        let sheet = try XCTUnwrap(host.presentedViewController as? DialogSheetController)
        let dialog = try XCTUnwrap(p.views.values.first { $0.props["id"] == "form" })
        sheet.view.frame.size.height = 200
        sheet.view.layoutIfNeeded()
        var finished = false
        UIView.animate(withDuration: 0.8, delay: 0, options: [.curveLinear]) {
            sheet.view.frame.size.height = 400
            sheet.view.layoutIfNeeded()
        } completion: { _ in finished = true }
        until("the sheet is between heights") {
            guard let layer = sheet.view.layer.presentation() else { return false }
            return layer.bounds.height > 240 && layer.bounds.height < 330
        }
        let card = try XCTUnwrap(sheet.view.layer.presentation()), scroll = try XCTUnwrap(sheet.scroller.layer.presentation())
        let port = try XCTUnwrap(p.viewport.layer.presentation())
        let visible = p.viewportPoints(card.convert(card.bounds, to: port).intersection(scroll.convert(scroll.bounds, to: port)))
        let clip = try XCTUnwrap(p.openTopLayer.frames.first { $0.0 == dialog.id }?.2)
        XCTAssertEqual(clip.height, visible.height, accuracy: 3, "the published clip follows native presentation bounds")
        let point = CGPoint(x: visible.midX, y: visible.maxY + 20)
        let presented = try XCTUnwrap(sheet.presentationController?.presentedView)
        let at = p.viewport.convert(point, to: presented)
        XCTAssertTrue(presented.bounds.contains(at), "this point is sheet chrome beyond the visible content clip")
        let delivered = try XCTUnwrap(w.hitTest(p.viewport.convert(point, to: w), with: nil))
        XCTAssertTrue(delivered === presented || delivered.isDescendant(of: presented))
        let sample = PointerSample(x: 0, y: 0, buttons: 0, pressure: 0, type: "mouse", id: 1, clientX: Double(point.x), clientY: Double(point.y))
        s.apply(s.runtime.pointer(dialog.id, .move, sample, now: s.now()))
        XCTAssertEqual(slots(s)["hit"] as? String, "none", "sheet chrome is outside both the content clip and the backdrop")
        XCTAssertEqual(slots(s)["hits"] as? String, "")
        until("the native resize ends") { finished }
    }

    func testScrollingTheSheetDeliversIntersectionWithoutAnotherInput() throws {
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else { throw missing("scene", "native sheet scrolling requires a scene") }
        let s = ExactApp.shared.makeSession(label: "dialog-geometry")
        session = s
        let view = ExactView(session: s), host = UIViewController(), w = UIWindow(windowScene: scene)
        w.frame = w.screen.bounds; w.rootViewController = host; w.makeKeyAndVisible(); host.view.addSubview(view)
        window = w
        XCTAssertNil(s.boot(plan: try fixturePlan("DIALOG_SHEET_READS"), size: w.bounds.size).error)
        view.frame = host.view.bounds; view.layoutIfNeeded()
        if s.firstDrawMs == nil { s.firstDrawMs = ExactEnv.wall() }
        let p = s.presenter
        defer { p.menus.reset(); host.dismiss(animated: false) }
        p.dialogCommand("showModal", "form")
        until("the sheet appeared") { host.presentedViewController is DialogSheetController && !p.menus.inTransition }
        let sheet = try XCTUnwrap(host.presentedViewController as? DialogSheetController)
        until("initial observer delivered") { self.slots(s)["observations"] as? Double ?? 0 > 0 }
        XCTAssertEqual(slots(s)["seen"] as? Bool, false)
        // Reading slots does not sample UIKit geometry or send a runner input.
        sheet.scroller.setContentOffset(CGPoint(x: 0, y: 1300), animated: false)
        until("scrolling alone enters the target") { self.slots(s)["seen"] as? Bool == true }
        sheet.scroller.setContentOffset(.zero, animated: false)
        until("scrolling alone leaves the target") { self.slots(s)["seen"] as? Bool == false }
    }
}
#endif
