// @exact/tips (LLP 1035.006 row 11.08): `showTip` presents TipKit's popover
// anchored to the element its `source` names, a bar item where a bar shows
// the element and its view otherwise (`Presenter.anchor(id:)`, the hatch
// table's `anchor`), and `invalidateTip` closes it, after which TipKit does
// not show it again. The package's Swift is compiled here as
// TipsImplementation; its context answers `anchor` as ExactExtensions.swift
// does, from the presenter.
//   bun host/apple/build.mjs --test [--ios] --only TipsIOSTests|TipsMacTests
import ExactPackage
import TipKit
import XCTest
@testable import ExactKit
@testable import TipsImplementation
#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// The package's context over `presenter`, its TipKit record in a fresh directory.
private func tipsContext(_ presenter: Presenter, view: @escaping () -> ExactPackagePlatformView?, log: @escaping (String) -> Void) -> ExactPackageContext {
    let data = FileManager.default.temporaryDirectory.appendingPathComponent("exact-tips-test-\(UUID().uuidString)", isDirectory: true)
    return ExactPackageContext(agent: true, session: "tips", data: data, changed: { _ in }, log: log, now: { 0 }, view: view, anchor: { id in
        guard let found = presenter.anchor(id: id) else { return nil }
        return found.bar ? (found.object as? ExactPackageBarItem).map(ExactAnchor.barItem) : (found.object as? ExactPackagePlatformView).map(ExactAnchor.view)
    })
}

private func spin(until done: () -> Bool, _ seconds: Double = 5) -> Bool {
    let deadline = Date().addingTimeInterval(seconds)
    while !done(), Date() < deadline { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.02)) }
    return done()
}

#if os(iOS)
final class TipsIOSTests: XCTestCase {
    func testATipShowsOverABarItemAndAButtonAndAnInvalidatedOneStaysGone() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "UIKit presents a popover only from a window a scene holds") }
        let session = ExactApp.shared.makeSession(label: "tips")
        defer { session.destroy() }
        let view = ExactView(session: session), host = UIViewController()
        let window = UIWindow(windowScene: scene)
        window.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        window.rootViewController = host
        window.makeKeyAndVisible()
        defer { window.isHidden = true }
        host.view.addSubview(view)
        XCTAssertNil(session.boot(plan: try fixturePlan("BAR_SHARE"), size: CGSize(width: 402, height: 874)).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        let p = session.presenter
        XCTAssertTrue(spin { p.navigation.primaryNavigation?.topViewController is RouteController }, "the route shows")
        let share = try XCTUnwrap(p.views.values.first { $0.props["id"] == "share" })
        let item = try XCTUnwrap(p.navigation.shownBarItem(share.id), "the header's Share button is a bar item")
        var said: [String] = []
        let tips = TipsModule(context: tipsContext(p, view: { view }, log: { said.append($0) }))
        func send(_ name: String, _ args: [String: Any]) { tips.later(name, args, preferred: nil) { XCTAssertNil($0["error"]) } }
        func shown() -> TipUIPopoverViewController? {
            host.presentedViewController.flatMap { $0.isBeingDismissed ? nil : $0 as? TipUIPopoverViewController }
        }

        send("showTip", ["id": "share-tip", "title": "Share a fixture", "message": "Send it to a friend.", "image": "square.and.arrow.up", "source": "share"])
        XCTAssertTrue(spin { shown() != nil }, "TipKit's popover shows: \(said)")
        XCTAssertTrue(shown()?.popoverPresentationController?.sourceItem === item, "anchored to the bar item")
        XCTAssertTrue(said.contains("showTip: share-tip shown over #share's bar item"), "\(said)")

        send("invalidateTip", ["id": "share-tip"])
        XCTAssertTrue(spin { host.presentedViewController == nil }, "invalidated, it closes")
        XCTAssertTrue(spin { OfferedTip(id: "share-tip").status == .invalidated(.actionPerformed) }, "TipKit keeps it invalidated: \(OfferedTip(id: "share-tip").status)")

        // A button in the page: its view, or its control when it is drawn as one.
        send("showTip", ["id": "toggle-tip", "title": "Toggle the bar", "source": "toggle"])
        XCTAssertTrue(spin { shown() != nil }, "\(said)")
        let anchor = try XCTUnwrap(p.anchor(id: "toggle"))
        XCTAssertFalse(anchor.bar)
        XCTAssertTrue(shown()?.popoverPresentationController?.sourceItem === anchor.object, "anchored to the button")
        send("invalidateTip", ["id": "toggle-tip"])
        XCTAssertTrue(spin { host.presentedViewController == nil })

        // Offered again, an invalidated tip does not show; no element of the id, nothing shows.
        send("showTip", ["id": "share-tip", "title": "Share a fixture", "source": "share"])
        send("showTip", ["id": "missing-tip", "title": "Nowhere", "source": "nowhere"])
        XCTAssertFalse(spin(until: { shown() != nil }, 1), "\(said)")
        XCTAssertTrue(said.contains("showTip: missing-tip not shown, no element #nowhere shows"), "\(said)")
        tips.destroy {}
    }
}
#endif

#if os(macOS)
final class TipsMacTests: XCTestCase {
    func testATipShowsOverAToolbarItemAndAViewAndAnInvalidatedOneStaysGone() throws {
        _ = NSApplication.shared
        let p = Presenter()
        let window = NSWindow(contentRect: NSRect(x: 100, y: 100, width: 700, height: 400), styleMask: [.titled, .closable, .resizable], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.viewport
        defer { window.close() }
        // A route (2) whose header (10) holds its heading (11) and an Add button (12, `#add`), then its page (15)
        // holding a Filter button (16, `#filter`).
        func view(_ id: Int, _ kind: String = "view", _ props: [String: String] = [:], press: Bool = false, at frame: [Double] = [0, 0, 60, 30]) -> [[String: Any]] {
            [["op": "create", "id": id, "kind": kind, "props": props, "handlers": press ? ["press"] : []],
             ["op": "frame", "id": id, "x": frame[0], "y": frame[1], "w": frame[2], "h": frame[3]]]
        }
        var ops = view(1, "view", ["navigationBack": "back", "navigationKey": "a"], at: [0, 0, 700, 400]) + view(2, "view", ["navigationKey": "a"], at: [0, 0, 700, 400])
        ops += view(10, "view", ["semanticTag": "header"]) + view(11, "text", ["text": "Library", "accessibilityRole": "heading", "accessibilityHeadingLevel": "1"])
        ops += view(12, "button", ["id": "add", "accessibilityLabel": "Add Book"], press: true) + view(120, "image", ["symbolName": "plus"])
        ops += view(15, "view", [:], at: [0, 0, 700, 400]) + view(16, "button", ["id": "filter", "accessibilityLabel": "Filter"], press: true, at: [40, 120, 80, 30])
        ops += [["op": "children", "id": 12, "ids": [120]], ["op": "children", "id": 10, "ids": [11, 12]], ["op": "children", "id": 15, "ids": [16]],
                ["op": "children", "id": 2, "ids": [10, 15]], ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]]]
        p.apply(wireBatch(ops))
        XCTAssertTrue(p.toolbar.attach(to: window))
        window.makeKeyAndOrderFront(nil)
        let item = try XCTUnwrap(p.toolbar.items[12], "the header's Add button is a toolbar item")
        XCTAssertTrue(p.anchor(id: "add")?.object === item, "the toolbar item is the button's anchor")
        var said: [String] = []
        let tips = TipsModule(context: tipsContext(p, view: { p.viewport }, log: { said.append($0) }))
        func send(_ name: String, _ args: [String: Any]) { tips.later(name, args, preferred: nil) { XCTAssertNil($0["error"]) } }
        func popover(_ id: String) -> NSPopover? { tips.offered[id]?.popover.flatMap { $0.isShown ? $0 : nil } }

        send("showTip", ["id": "add-tip", "title": "Add a book", "message": "Your library starts here.", "image": "plus.circle", "source": "add"])
        XCTAssertTrue(spin { popover("add-tip") != nil }, "TipKit's popover shows: \(said)")
        XCTAssertTrue(popover("add-tip") is TipNSPopover)
        XCTAssertTrue(said.contains("showTip: add-tip shown over #add's bar item"), "\(said)")
        let shownOver = popover("add-tip")
        send("invalidateTip", ["id": "add-tip"])
        XCTAssertTrue(spin { shownOver?.isShown == false }, "invalidated, it closes")
        XCTAssertTrue(spin { OfferedTip(id: "add-tip").status == .invalidated(.actionPerformed) }, "TipKit keeps it invalidated: \(OfferedTip(id: "add-tip").status)")

        // A button in the page: the popover's arrow points at its box.
        send("showTip", ["id": "filter-tip", "title": "Filter the list", "source": "filter"])
        XCTAssertTrue(spin { popover("filter-tip") != nil }, "\(said)")
        let filter = try XCTUnwrap(p.views[16])
        XCTAssertTrue(p.anchor(id: "filter")?.object === filter)
        let box = filter.window!.convertToScreen(filter.convert(filter.bounds, to: nil))
        let frame = try XCTUnwrap(popover("filter-tip")?.contentViewController?.view.window?.frame)
        XCTAssertTrue(frame.minX < box.maxX && frame.maxX > box.minX, "beside the button horizontally: \(frame) \(box)")
        XCTAssertLessThanOrEqual(frame.maxY, box.minY + 1, "below it: \(frame) \(box)")
        send("invalidateTip", ["id": "filter-tip"])

        // Offered again, an invalidated tip does not show; no element of the id, nothing shows.
        send("showTip", ["id": "add-tip", "title": "Add a book", "source": "add"])
        send("showTip", ["id": "missing-tip", "title": "Nowhere", "source": "nowhere"])
        XCTAssertFalse(spin(until: { popover("add-tip") != nil || popover("missing-tip") != nil }, 1), "\(said)")
        XCTAssertTrue(said.contains("showTip: missing-tip not shown, no element #nowhere shows"), "\(said)")
        tips.destroy {}
    }
}
#endif
