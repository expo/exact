#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactSurfaces

/// Whether the fake surface places its first child, moved right by `controlsShift`.
private var placesFirst = false
private let controlsShift: Float = 100

/// A UIKit control inside a canvas child its surface places (LLP 1014 D5)
/// is where the surface draws it to assistive technology, as Exact's own
/// node views are (#69): a control that is an element maps its own frame,
/// and one whose parts are UIKit's elements stands one in for each part.
/// A context menu lifted from inside such a child rises from where it is
/// drawn. UIKit, so a simulator runs it:
///   bun host/apple/build.mjs --test --ios --only CanvasPlacedControlsIOSTests
final class CanvasPlacedControlsIOSTests: XCTestCase {
    override class func setUp() { super.setUp(); ExactSurfaces.install() } // LLP 1047.001 D4
    override func setUp() { placesFirst = false }

    private struct Deck {
        let session: ExactSession, host: CanvasesHost, canvas: NodeView, overlay: PlainView, window: UIWindow
        func frame() { _ = host.readback(view: canvas) }
    }

    private func deck(_ label: String) throws -> Deck {
        let s = ExactApp.shared.makeSession(label: label)
        let m = GpuModule(create: { _, _, _, _, _ in 7 }, bind: { _, _, _ in 0 },
            render: { _, _, _, _, _ in 0 }, dirty: { _ in 0 }, destroy: { _ in },
            texture: { _, _, _, _, _ in 0 }, textureMetal: nil, sync: nil,
            childrenMode: { _ in 3 }, readback: { _, _, _, _, _, _, _ in 0 },
            child: { _, _, _, _, _, _, _, _, _, _, _, _ in 0 }, childrenCount: { _, _ in 0 },
            placement: { _, index, out, _ in
                guard placesFirst, index == 0 else { return 0 }
                let h: [Float] = [1, 0, controlsShift, 0, 1, 0, 0, 0, 1, 0]
                for (i, v) in h.enumerated() { out?[i] = v }
                return 1
            },
            shader: nil, validateShader: nil, clearShaders: nil, errorLen: { 0 }, errorPtr: { nil },
            wantsInput: nil, input: nil, messages: nil, published: nil, agent: nil, outPtr: nil)
        let host = s.surfaceHost
        host.modules[""] = m; host.attempted = [""]
        m.canvases.add(host)
        let p = s.presenter
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.isHidden = false
        let canvas = NodeView(id: 100, kind: "canvas", presenter: p)
        canvas.frame = CGRect(x: 0, y: 0, width: 400, height: 400)
        p.viewport.addSubview(canvas); p.views[100] = canvas
        let overlay = try XCTUnwrap(canvas.overlay)
        try XCTUnwrap(canvas.canvasMetal).frame = canvas.bounds
        overlay.frame = canvas.bounds
        let e = CanvasesHost.Entry(view: canvas, name: "deck", values: []); e.id = 7; e.module = m; e.each = true
        host.entries[100] = e
        return Deck(session: s, host: host, canvas: canvas, overlay: overlay, window: window)
    }

    private func node(_ id: UInt32, _ p: Presenter, _ frame: CGRect, in parent: UIView) -> NodeView {
        let n = NodeView(id: id, kind: "view", presenter: p)
        n.frame = frame
        parent.addSubview(n); p.views[id] = n
        return n
    }

    /// The leaves VoiceOver visits in `window`, as UIKit's traversal finds them.
    private func voiceOver(_ window: UIWindow) throws -> [NSObject] {
        let cls = try XCTUnwrap(NSClassFromString("UIAccessibilityElementTraversalOptions") as? NSObject.Type)
        let options = cls.perform(NSSelectorFromString("defaultVoiceOverOptions"))?.takeUnretainedValue()
        return try XCTUnwrap(window.perform(NSSelectorFromString("_accessibilityLeafDescendantsWithOptions:"), with: options)?.takeUnretainedValue() as? [NSObject])
    }

    func testPlacedSegmentsKeepTheirAuthoredTabsInAccessibilityReads() throws {
        guard Presenter.axRuntimeLoaded else { throw missing("accessibility", "UIKit's segments need its accessibility runtime") }
        let d = try deck("canvas-placed-tabs")
        defer { d.session.destroy() }
        let p = d.session.presenter
        _ = node(101, p, CGRect(x: 0, y: 0, width: 300, height: 300), in: d.overlay)
        p.apply(wireBatch([
            ["op": "create", "id": 30, "kind": "view", "props": ["accessibilityRole": "tablist"]],
            ["op": "create", "id": 31, "kind": "button", "handlers": ["press"], "props": ["accessibilityRole": "tab", "testId": "one"]],
            ["op": "create", "id": 32, "kind": "button", "handlers": ["press"], "props": ["accessibilityRole": "tab", "testId": "two"]],
            ["op": "create", "id": 33, "kind": "text", "props": ["text": "One"]],
            ["op": "create", "id": 34, "kind": "text", "props": ["text": "Two"]],
            ["op": "children", "id": 31, "ids": [33]], ["op": "children", "id": 32, "ids": [34]],
            ["op": "children", "id": 30, "ids": [31, 32]], ["op": "children", "id": 101, "ids": [30]],
            ["op": "frame", "id": 30, "x": 10, "y": 10, "w": 200, "h": 40],
        ]))
        try XCTUnwrap(p.segments.control(of: 30)).layoutIfNeeded()
        placesFirst = true
        d.frame()
        let parts = try voiceOver(d.window).compactMap { $0 as? PlacedElement }
        XCTAssertEqual(Set(parts.compactMap(\.accessibilityLabel)), ["One", "Two"])
        func projected(_ scope: Set<UInt32>? = nil) -> [[String: Any]] {
            (p.axElements(roots: [d.canvas], scope: scope)["elements"] as? [[String: Any]] ?? [])
                .filter { ($0["name"] as? String).map(["One", "Two"].contains) == true }
        }
        XCTAssertEqual(Set(projected().compactMap { $0["id"] as? UInt32 }), [31, 32])
        for (id, testId, name) in [(UInt32(31), "one", "One"), (UInt32(32), "two", "Two")] {
            let scoped = projected([id])
            XCTAssertEqual(scoped.count, 1)
            XCTAssertEqual(scoped.first?["testId"] as? String, testId)
            XCTAssertEqual(scoped.first?["name"] as? String, name)
            XCTAssertEqual(scoped.first?["via"] as? String, "owner")
        }
        for prop in ["accessibilityElementsHidden", "inert"] {
            p.apply(wireBatch([["op": "props", "id": 30, "set": [prop: "true"]]]))
            XCTAssertTrue(projected().isEmpty)
            p.apply(wireBatch([["op": "props", "id": 30, "clear": [prop]]]))
            XCTAssertEqual(projected().count, 2)
        }
    }

    func testAControlInsideAPlacedChildIsWhereTheSurfaceDrawsIt() throws {
        guard Presenter.axRuntimeLoaded else { throw missing("accessibility", "UIKit's VoiceOver traversal needs its accessibility runtime") }
        let d = try deck("canvas-placed-controls")
        defer { d.session.destroy() }
        let p = d.session.presenter
        let card = node(101, p, CGRect(x: 0, y: 0, width: 300, height: 300), in: d.overlay)
        let toggle = ExactSwitch(), check = ExactCheckbox(frame: CGRect(x: 10, y: 60, width: 30, height: 30))
        toggle.frame.origin = CGPoint(x: 10, y: 10)
        let stepper = FieldStepper(), segments = LabelledSegmentedControl(items: ["One", "Two"])
        stepper.sizeToFit(); segments.sizeToFit()
        stepper.frame.origin = CGPoint(x: 10, y: 110)
        segments.frame.origin = CGPoint(x: 10, y: 160)
        let field = TextField(frame: CGRect(x: 10, y: 210, width: 200, height: 30))
        field.placeholder = "Name"
        for control in [toggle, check, stepper, segments, field] as [UIView] { card.addSubview(control); control.layoutIfNeeded() }
        d.frame()
        XCTAssertNil(card.placement)
        let unplaced = try voiceOver(d.window).map(\.accessibilityFrame)
        XCTAssertEqual(toggle.accessibilityFrame.minX, 10, accuracy: 0.5, "left in place, UIKit's own frame")

        placesFirst = true
        d.frame()
        XCTAssertNotNil(card.placement)
        let shift = CGFloat(controlsShift)
        XCTAssertEqual(toggle.accessibilityFrame.minX, 10 + shift, accuracy: 0.5, "a switch is where its child is placed")
        XCTAssertEqual(check.accessibilityFrame.minX, 10 + shift, accuracy: 0.5, "and a checkbox")
        XCTAssertEqual(field.accessibilityFrame.minX, 10 + shift, accuracy: 0.5, "and a field")
        let leaves = try voiceOver(d.window)
        XCTAssertEqual(leaves.count, unplaced.count, "VoiceOver reaches the same elements: \(leaves)")
        for (leaf, before) in zip(leaves, unplaced) {
            XCTAssertEqual(leaf.accessibilityFrame.minX, before.minX + shift, accuracy: 0.5, "\(leaf) at its placed frame")
            XCTAssertEqual(leaf.accessibilityFrame.minY, before.minY, accuracy: 0.5)
            // VoiceOver's double tap lands where the leaf is drawn, which the canvas maps back to it.
            XCTAssertTrue(leaf.accessibilityFrame.contains(leaf.accessibilityActivationPoint), "\(leaf) is activated where it is drawn")
        }
        let parts = leaves.compactMap { $0 as? PlacedElement }
        XCTAssertEqual(parts.count, 4, "a stepper's two buttons and the two segments stand in: \(leaves)")
        XCTAssertEqual(Set(parts.compactMap(\.accessibilityLabel)).isSuperset(of: ["One", "Two"]), true)

        placesFirst = false
        d.frame()
        XCTAssertNil(card.placement)
        XCTAssertEqual(try voiceOver(d.window).map(\.accessibilityFrame), unplaced, "left in place again, UIKit's own frames and elements")
        XCTAssertNil(try voiceOver(d.window).first { $0 is PlacedElement })
    }

    /// An empty date picker's own elements (`EmptyDateElement`) stand in for
    /// UIKit's at the frame the surface draws them at, and VoiceOver's tap
    /// lands there too, which the canvas maps back to the picker.
    func testAnEmptyDatePickerInsideAPlacedChildIsActivatedWhereItIsDrawn() throws {
        guard Presenter.axRuntimeLoaded else { throw missing("accessibility", "UIKit's VoiceOver traversal needs its accessibility runtime") }
        let d = try deck("canvas-placed-empty-date")
        defer { d.session.destroy() }
        let p = d.session.presenter
        let card = node(101, p, CGRect(x: 0, y: 0, width: 300, height: 300), in: d.overlay)
        let picker = BoundDatePicker()
        picker.preferredDatePickerStyle = .compact
        picker.datePickerMode = .date
        picker.frame = CGRect(x: 10, y: 10, width: 200, height: 40)
        card.addSubview(picker)
        picker.empty = true
        picker.layoutIfNeeded()
        d.frame()
        let before = try voiceOver(d.window).compactMap { $0 as? EmptyDateElement }
        XCTAssertFalse(before.isEmpty, "the empty picker's own elements")
        let unplaced = before.map(\.accessibilityActivationPoint)
        placesFirst = true
        d.frame()
        let after = try voiceOver(d.window).compactMap { $0 as? EmptyDateElement }
        XCTAssertEqual(after.count, before.count)
        for (element, was) in zip(after, unplaced) {
            XCTAssertEqual(element.accessibilityActivationPoint.x, was.x + CGFloat(controlsShift), accuracy: 0.5, "tapped where it is drawn")
            XCTAssertEqual(element.accessibilityActivationPoint.y, was.y, accuracy: 0.5)
            XCTAssertTrue(element.accessibilityFrame.contains(element.accessibilityActivationPoint))
        }
    }

    /// A long press inside a placed child lifts a picture of the screen
    /// where the surface draws the box, over its controller's view, from
    /// the drawn box's centre.
    func testALiftInsideAPlacedChildRisesFromWhereItIsDrawn() throws {
        guard let scene = UIApplication.shared.connectedScenes.compactMap({ $0 as? UIWindowScene }).first else {
            throw missing("scene", "the screen's picture needs a window the scene draws")
        }
        let d = try deck("canvas-placed-lift-frame")
        defer { d.session.destroy(); d.window.isHidden = true }
        let p = d.session.presenter
        let controller = UIViewController()
        d.window.windowScene = scene
        d.window.rootViewController = controller
        controller.view.frame = d.window.bounds
        controller.view.addSubview(p.viewport)
        let card = node(101, p, CGRect(x: 0, y: 50, width: 200, height: 100), in: d.overlay)
        let row = node(102, p, CGRect(x: 20, y: 10, width: 100, height: 40), in: card)
        placesFirst = true
        d.frame()
        d.window.makeKeyAndVisible()
        d.window.layoutIfNeeded()
        CATransaction.flush()
        let preview = try XCTUnwrap(ContextMenuHost.placedPreview(of: row, in: row))
        XCTAssertTrue(preview.target.container === controller.view)
        // The placement maps the card's own points to the canvas: its kernel origin is not on the way.
        XCTAssertEqual(preview.target.center.x, 20 + 50 + CGFloat(controlsShift), accuracy: 0.5, "the drawn box's centre")
        XCTAssertEqual(preview.target.center.y, 10 + 20, accuracy: 0.5)
        XCTAssertNotEqual(preview.target.center, row.convert(CGPoint(x: 50, y: 20), to: controller.view), "not its kernel frame's")
        XCTAssertEqual(preview.view.bounds.size, row.bounds.size)
    }
}
#endif
