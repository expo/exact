#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactSurfaces

/// The indices the fake surface places (`gpu_placement` 1), moved right by
/// `placedShift`; the rest it leaves at their kernel frames (0).
private var placedIndices: Set<UInt32> = []
/// The indices the fake surface hides (`gpu_placement` 2).
private var hiddenIndices: Set<UInt32> = []
private let placedShift: Float = 100
private var capturedChildPixels: [[UInt8]]?

/// A canvas child its surface places (LLP 1014 D5) is drawn by the surface,
/// so UIKit must not draw it at its own box; that is done without alpha 0,
/// below which UIKit stops applying its accessibility rules: the snapshot
/// XCUITest reads lists a leaf UIKit was told is no element (#69), and
/// VoiceOver goes no deeper than a placed child that is not an element.
/// UIKit, so a simulator runs it:
///   bun host/apple/build.mjs --test --ios --only CanvasPlacedDrawingIOSTests
final class CanvasPlacedDrawingIOSTests: XCTestCase {
    override class func setUp() { super.setUp(); ExactSurfaces.install() } // LLP 1047.001 D4
    override func setUp() { placedIndices = []; hiddenIndices = []; capturedChildPixels = nil }

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
            child: { _, _, _, _, _, _, _, _, w, h, bytes, _ in
                if capturedChildPixels != nil, let bytes, w > 0, h > 0 {
                    let at = Int((h / 2 * w + w / 2) * 4)
                    capturedChildPixels?.append(Array(UnsafeBufferPointer(start: bytes + at, count: 4)))
                }
                return 0
            }, childrenCount: { _, _ in 0 },
            placement: { _, index, out, _ in
                if hiddenIndices.contains(index) { return 2 }
                guard placedIndices.contains(index) else { return 0 }
                let h: [Float] = [1, 0, placedShift, 0, 1, 0, 0, 0, 1, 0]
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

    /// A plain view filling `view` with `colour`: a node's own background is
    /// its style's, which these nodes have none of.
    private func paint(_ colour: UIColor, into view: UIView, at index: Int? = nil) {
        let fill = UIView(frame: view.bounds); fill.backgroundColor = colour
        if let index { view.insertSubview(fill, at: index) } else { view.addSubview(fill) }
    }

    /// The window as Core Animation composites it, one point a pixel.
    private func picture(_ window: UIWindow) throws -> (CGPoint) -> [UInt8] {
        let format = UIGraphicsImageRendererFormat(); format.scale = 1; format.opaque = true
        let image = UIGraphicsImageRenderer(bounds: window.bounds, format: format).image { window.layer.render(in: $0.cgContext) }
        return try pixels(try XCTUnwrap(image.cgImage))
    }

    /// An image's pixels, copied, one at a point (its scale 1).
    private func pixels(_ cg: CGImage) throws -> (CGPoint) -> [UInt8] {
        let data = try XCTUnwrap(cg.dataProvider?.data)
        let bytes = [UInt8](UnsafeBufferPointer(start: try XCTUnwrap(CFDataGetBytePtr(data)), count: CFDataGetLength(data)))
        let row = cg.bytesPerRow, step = cg.bitsPerPixel / 8
        // Compared only with each other: the channel order does not matter.
        return { p in let at = Int(p.y) * row + Int(p.x) * step; return [bytes[at], bytes[at + 1], bytes[at + 2]] }
    }

    /// UIKit's own accessibility, as its clients read it: the leaves
    /// VoiceOver visits, and the objects the snapshot XCUITest reads holds.
    private func uikit(_ window: UIWindow) throws -> (voiceOver: [NSObject], snapshot: [NSObject]) {
        let cls = try XCTUnwrap(NSClassFromString("UIAccessibilityElementTraversalOptions") as? NSObject.Type)
        let options = cls.perform(NSSelectorFromString("defaultVoiceOverOptions"))?.takeUnretainedValue()
        let leaves = try XCTUnwrap(window.perform(NSSelectorFromString("_accessibilityLeafDescendantsWithOptions:"), with: options)?.takeUnretainedValue() as? [NSObject])
        // The snapshot names each element by its address: matched to the window's views.
        var known: [String: NSObject] = [:]
        var pending: [UIView] = [window]
        while let v = pending.popLast() { known[String(format: "%lx", unsafeBitCast(v, to: Int.self))] = v; pending += v.subviews }
        let sel = NSSelectorFromString("_accessibilityUserTestingSnapshotDescendantsWithAttributes:maxDepth:maxChildren:maxArrayCount:honorsModalViews:")
        typealias Snapshot = @convention(c) (AnyObject, Selector, AnyObject?, Int, Int, Int, Bool) -> AnyObject?
        let call = unsafeBitCast(try XCTUnwrap(window.method(for: sel)), to: Snapshot.self)
        var found: [NSObject] = [], nodes: [Any] = [call(window, sel, nil, 80, 5000, 5000, true) as Any]
        while let n = nodes.popLast() as? [String: Any] {
            if let ref = n["UIAccessibilitySnapshotKeyElement"].map({ "\($0)" }), let hash = ref.components(separatedBy: "hash:0x").last?.prefix(while: \.isHexDigit),
               let object = known[String(hash)] { found.append(object) }
            nodes += n["UIAccessibilitySnapshotKeyChildren"] as? [Any] ?? []
        }
        return (leaves, found)
    }

    /// A placed child's descendants are exposed at their placed frames, and
    /// what UIKit was told is no element, or hides its elements, is not.
    func testAPlacedChildsDescendantsAreExposedWhereItIsPlacedAndHiddenOnesAreNot() throws {
        guard Presenter.axRuntimeLoaded else { throw missing("accessibility", "UIKit's snapshot and VoiceOver traversal need its accessibility runtime") }
        let d = try deck("canvas-placed-hierarchy")
        defer { d.session.destroy() }
        let p = d.session.presenter
        placedIndices = [0]
        let row = node(101, p, CGRect(x: 0, y: 40, width: 300, height: 40), in: d.overlay)
        let words = node(102, p, CGRect(x: 0, y: 0, width: 120, height: 40), in: row)
        words.isAccessibilityElement = true; words.accessibilityLabel = "SHOWN-INSIDE"
        // A symbol's leaf, as `updateSymbol` makes it: never an element.
        let symbol = node(103, p, CGRect(x: 130, y: 10, width: 20, height: 20), in: row)
        let leaf = UIImageView(image: UIImage(systemName: "lock"))
        leaf.frame = symbol.bounds; leaf.isAccessibilityElement = false; leaf.accessibilityElementsHidden = true
        symbol.addSubview(leaf)
        let hidden = node(104, p, CGRect(x: 160, y: 0, width: 120, height: 40), in: row)
        hidden.accessibilityElementsHidden = true
        let nested = node(105, p, hidden.bounds, in: hidden)
        nested.isAccessibilityElement = true; nested.accessibilityLabel = "NESTED-HIDDEN"
        row.alpha = 0 // the author's `opacity: 0`, or a fade reaching it
        d.frame()
        XCTAssertNotNil(row.placement)
        row.alpha = 0 // and again while placed
        XCTAssertEqual(row.alpha, 1, "a placed child stays at alpha 1, below which UIKit's rules would stop")

        let (voiceOver, snapshot) = try uikit(d.window)
        XCTAssertTrue(voiceOver.contains(words), "VoiceOver reaches the placed row's words: \(voiceOver.map { $0.accessibilityLabel ?? "?" })")
        XCTAssertEqual(words.accessibilityFrame.minX, CGFloat(placedShift), accuracy: 0.5, "at their placed frame")
        XCTAssertFalse(voiceOver.contains(nested), "and not what a view hiding its elements holds")
        XCTAssertTrue(snapshot.contains(words), "XCUITest's snapshot lists the words")
        XCTAssertFalse(snapshot.contains(leaf), "and not the symbol's leaf, which is no element and hidden")
        XCTAssertFalse(snapshot.contains(nested), "nor what a view hiding its elements holds")
    }

    /// A child the surface leaves in place at its author's `opacity: 0` is
    /// left out of the canvas's accessibility list, as UIKit's own walk
    /// leaves out a transparent view: listed, UIKit would apply none of its
    /// rules below it (what #69 is about). So is one that was placed and is
    /// left in place again at that opacity, and one made transparent while
    /// unplaced, from its next frame.
    func testATransparentChildLeftInPlaceIsNotListedNorItsHiddenParts() throws {
        guard Presenter.axRuntimeLoaded else { throw missing("accessibility", "UIKit's snapshot needs its accessibility runtime") }
        let d = try deck("canvas-transparent-unplaced")
        defer { d.session.destroy() }
        let p = d.session.presenter
        let child = node(101, p, CGRect(x: 0, y: 40, width: 300, height: 40), in: d.overlay)
        let hidden = node(102, p, CGRect(x: 0, y: 0, width: 120, height: 40), in: child)
        hidden.accessibilityElementsHidden = true
        let nested = node(103, p, hidden.bounds, in: hidden)
        nested.isAccessibilityElement = true; nested.accessibilityLabel = "NESTED-HIDDEN"
        let symbol = node(104, p, CGRect(x: 130, y: 10, width: 20, height: 20), in: child)
        let leaf = UIImageView(image: UIImage(systemName: "lock"))
        leaf.frame = symbol.bounds; leaf.isAccessibilityElement = false; leaf.accessibilityElementsHidden = true
        symbol.addSubview(leaf)
        let other = node(105, p, CGRect(x: 0, y: 200, width: 300, height: 40), in: d.overlay)
        other.isAccessibilityElement = true; other.accessibilityLabel = "OTHER"
        func listed() -> [NodeView] { (d.overlay.accessibilityElements ?? []).compactMap { $0 as? NodeView } }
        func exposed() throws -> [NSObject] { try uikit(d.window).snapshot }

        child.alpha = 0 // the author's `opacity: 0`, left in place (outcome 0)
        d.frame()
        XCTAssertEqual(listed(), [other], "a transparent child is not listed")
        var snapshot = try exposed()
        XCTAssertFalse(snapshot.contains(leaf) || snapshot.contains(nested), "nor are its hidden parts exposed")

        placedIndices = [0]
        d.frame()
        XCTAssertEqual(listed(), [child, other], "placed, it is listed, at alpha 1")
        placedIndices = [] // 1 → 0, at the author's opacity again
        d.frame()
        XCTAssertEqual(child.alpha, 0)
        XCTAssertEqual(listed(), [other], "left in place again, transparent: not listed")
        snapshot = try exposed()
        XCTAssertFalse(snapshot.contains(leaf) || snapshot.contains(nested), "nor its hidden parts")

        child.alpha = 1
        d.frame()
        XCTAssertEqual(listed(), [child, other])
        d.canvas.needsCapture = false
        child.alpha = 0 // made transparent while unplaced
        XCTAssertTrue(d.canvas.needsCapture, "the canvas captures again, which brings the frame that lists its children again")
        d.frame()
        XCTAssertEqual(listed(), [other])
    }

    /// UIKit draws nothing of a placed child (the surface does) wherever its
    /// box goes, and draws an unplaced one as it is, its shadow included,
    /// over or under the placed one's box and shadow.
    func testUIKitDrawsNothingOfAPlacedChildAndAnUnplacedOneAsItIs() throws {
        let d = try deck("canvas-placed-pixels")
        defer { d.session.destroy() }
        let p = d.session.presenter
        paint(.green, into: d.canvas, at: 0)
        let placed = node(101, p, CGRect(x: 40, y: 40, width: 100, height: 100), in: d.overlay)
        paint(.red, into: placed)
        placed.alpha = 0.5 // the author's opacity, back when it is no longer placed
        placed.layer.shadowColor = UIColor.black.cgColor; placed.layer.shadowOpacity = 1
        placed.layer.shadowRadius = 0; placed.layer.shadowOffset = CGSize(width: 30, height: 30)
        // A HUD over the placed child's box and its shadow, the surface leaves in place.
        let hud = node(102, p, CGRect(x: 110, y: 110, width: 60, height: 60), in: d.overlay)
        paint(.blue, into: hud)
        hud.layer.shadowColor = UIColor.black.cgColor; hud.layer.shadowOpacity = 1
        hud.layer.shadowRadius = 0; hud.layer.shadowOffset = CGSize(width: 20, height: 20)
        placedIndices = [0]
        d.frame()

        XCTAssertNotNil(placed.placement)
        XCTAssertEqual(placed.alpha, 1, "a placed child is drawn by its surface, at alpha 1 here")
        var at = try picture(d.window)
        let green = at(CGPoint(x: 10, y: 10)), blue = at(CGPoint(x: 140, y: 140))
        XCTAssertNotEqual(blue, green, "the HUD draws")
        XCTAssertEqual(at(CGPoint(x: 90, y: 90)), green, "nothing of the placed child where its box is")
        XCTAssertEqual(at(CGPoint(x: 150, y: 70)), green, "nor of its shadow")
        XCTAssertEqual(at(CGPoint(x: 115, y: 115)), blue, "the HUD over the placed child's box keeps its pixels")
        XCTAssertEqual(at(CGPoint(x: 160, y: 160)), blue, "and over its shadow")
        let shadow = at(CGPoint(x: 185, y: 185))
        XCTAssertNotEqual(shadow, green, "with its own shadow beyond its box")
        XCTAssertNotEqual(shadow, blue)

        // Its box moves, scales and turns before the surface's next frame: still nothing of it.
        placed.frame = CGRect(x: 200, y: 20, width: 100, height: 60)
        placed.transform = CGAffineTransform(rotationAngle: .pi / 2).scaledBy(x: 1.5, y: 1.5)
        at = try picture(d.window)
        for x in stride(from: 5, to: 400, by: 10) {
            for y in stride(from: 5, to: 400, by: 10) where !CGRect(x: 105, y: 105, width: 90, height: 90).contains(CGPoint(x: x, y: y)) {
                XCTAssertEqual(at(CGPoint(x: x, y: y)), green, "nothing drawn at \(x), \(y)")
            }
        }
        placed.transform = .identity
        placedIndices = []
        d.frame()
        XCTAssertNil(placed.placement)
        XCTAssertEqual(placed.alpha, 0.5, accuracy: 0.001, "unplaced, it is at the author's opacity again")
        XCTAssertNotEqual(try picture(d.window)(CGPoint(x: 250, y: 50)), green, "and UIKit draws it")
    }

    /// A placed child with a CSS filter: its picture (`BoxFilter`), which
    /// stands beside it, shows only through the surface; the box keeps the
    /// mask that hides it behind that picture as it is drawn again; unplaced,
    /// the picture shows. (The filtered pixels themselves: a Mac test reads
    /// them, `BoxFilterTests`; this simulator's chain draws none.)
    func testAPlacedChildWithAFilterKeepsItsOwnMaskAndItsPictureShowsOnlyThroughItsSurface() throws {
        let sub: [Double] = [-10, -10, 20, 20]
        let program: [Double] = sub + [1] + [0, -1, -3] + sub + [0, 2, 2] // one Gaussian blur (`Filter::encode`)
        guard SvgFilterGPU.runs(program.map(Float.init)) else { throw missing("metal", "no GPU filter path on this host") }
        let d = try deck("canvas-placed-filter")
        defer { d.session.destroy() }
        let p = d.session.presenter
        let placed = node(101, p, CGRect(x: 40, y: 40, width: 100, height: 100), in: d.overlay)
        paint(.red, into: placed)
        placed.applyStyle(["filter": .object(["p": .array(program.map { .number($0) })])])
        placed.renderFilter()
        let f = try XCTUnwrap(placed.boxFilter)
        XCTAssertFalse(f.casts)
        XCTAssertTrue(placed.layer.mask === f.hide, "unplaced, the box hides behind its picture")
        placedIndices = [0]
        d.frame()
        XCTAssertNotNil(placed.placement)
        XCTAssertTrue(f.picture.isHidden, "placed, its picture shows only through the surface")
        placed.renderFilter() // drawn again while placed (a change under it)
        XCTAssertTrue(f.picture.isHidden, "still")
        XCTAssertTrue(placed.layer.boxMask === f.hide, "the box's own mask is still the filter's, not the one that hides it on screen")
        XCTAssertFalse(placed.layer.mask === f.hide, "which is the one Core Animation has")
        placedIndices = []
        d.frame()
        XCTAssertNil(placed.placement)
        XCTAssertFalse(f.picture.isHidden, "unplaced, its picture shows")
        XCTAssertTrue(placed.layer.mask === f.hide, "and the box hides behind it again")
    }

    /// A filtered child's picture stands beside it, so the box's mask does
    /// not cover it: it follows the child's placement once both of the
    /// placement's fields are set, with no frame or batch after. Placed, then
    /// hidden by the surface (1 → 2): still hidden. Then left in place
    /// (2 → 0): shown.
    func testAFilteredChildsPictureFollowsItsPlacementHiddenAndBack() throws {
        let sub: [Double] = [-10, -10, 20, 20]
        let program: [Double] = sub + [1] + [0, -1, -3] + sub + [0, 2, 2]
        guard SvgFilterGPU.runs(program.map(Float.init)) else { throw missing("metal", "no GPU filter path on this host") }
        let d = try deck("canvas-placed-filter-hidden")
        defer { d.session.destroy() }
        let p = d.session.presenter
        let child = node(101, p, CGRect(x: 40, y: 40, width: 100, height: 100), in: d.overlay)
        paint(.red, into: child)
        child.applyStyle(["filter": .object(["p": .array(program.map { .number($0) })])])
        child.renderFilter()
        let picture = try XCTUnwrap(child.boxFilter?.picture)
        placedIndices = [0]
        d.frame()
        XCTAssertTrue(picture.isHidden, "placed: only its surface shows it")
        placedIndices = []; hiddenIndices = [0]
        d.frame()
        XCTAssertTrue(child.placementHidden && child.isHidden)
        XCTAssertTrue(picture.isHidden, "hidden by its surface: hidden")
        hiddenIndices = []
        d.frame()
        XCTAssertFalse(child.isHidden)
        XCTAssertFalse(picture.isHidden, "left in place: shown")
    }

    /// A filtered child's capture, the surface's texture of it, is its
    /// filter's picture, on the GPU and on the CPU (`EXACT_CAPTURE=cpu`),
    /// placed or not. The pixels here are a stand-in for the filter's own,
    /// which this simulator's chain does not draw in a test: what is checked
    /// is that the picture is what is captured.
    func testAFilteredChildIsCapturedAsItsPictureOnTheGPUAndTheCPU() throws {
        let sub: [Double] = [-10, -10, 20, 20]
        let program: [Double] = sub + [1] + [0, -1, -3] + sub + [0, 2, 2]
        guard SvgFilterGPU.runs(program.map(Float.init)) else { throw missing("metal", "no GPU filter path on this host") }
        let d = try deck("canvas-placed-filter-capture")
        defer { d.session.destroy() }
        let p = d.session.presenter
        let child = node(101, p, CGRect(x: 40, y: 40, width: 100, height: 100), in: d.overlay)
        child.applyStyle(["filter": .object(["p": .array(program.map { .number($0) })])])
        child.renderFilter()
        let picture = try XCTUnwrap(child.boxFilter?.picture), shown = try XCTUnwrap(picture.sublayers?.first)
        let red = try XCTUnwrap(UIGraphicsImageRenderer(size: CGSize(width: 10, height: 10)).image { c in UIColor.red.setFill(); c.fill(CGRect(x: 0, y: 0, width: 10, height: 10)) }.cgImage)
        shown.frame = child.bounds; shown.contents = red
        for placed in [false, true] {
            placedIndices = placed ? [0] : []
            d.frame()
            let cpu = try XCTUnwrap(Bitmap.blank(width: 100, height: 100))
            Capture.draw(child, scale: 1, into: cpu)
            let centre = try pixels(try XCTUnwrap(cpu.image?.cgImage))(CGPoint(x: 50, y: 50))
            XCTAssertNotEqual(centre, [0, 0, 0], "the CPU capture draws the picture, placed \(placed)")
            let shadow = try XCTUnwrap(Shadow.shared, "no Metal capture here: the GPU's capture is not compared")
            let gpu = try XCTUnwrap(Bitmap.blank(width: 100, height: 100))
            XCTAssertTrue(shadow.render(child, scale: 1, into: gpu), "the GPU's capture rendered")
            XCTAssertEqual(try pixels(try XCTUnwrap(gpu.image?.cgImage))(CGPoint(x: 50, y: 50)), centre, "and draws the same, placed \(placed)")
        }
        XCTAssertTrue(picture.isHidden, "placed, its picture still shows only through the surface")
    }

    func testLayerFiltersReachCanvasTexturesAfterStyleBatches() throws {
        let d = try deck("canvas-opacity-style-batch")
        defer { d.session.destroy() }
        try XCTUnwrap(d.host.entries[100]).through = true
        let p = d.session.presenter
        let child = node(101, p, CGRect(x: 40, y: 40, width: 100, height: 100), in: d.overlay)
        let nested = node(102, p, child.bounds, in: child)
        paint(.red, into: nested)
        placedIndices = [0]
        p.apply(wireBatch([["op": "present", "id": 101, "property": "opacity", "x": 0.8]]))
        d.frame()
        capturedChildPixels = []
        func opacity(_ factor: Double) -> [String: Any] {
            ["p": [0, 0, 0, 0, 1, 9, -1, -3, -1e6, -1e6, 2e6, 2e6, 0, 0, 0, 0, 1, 2, 0, factor]]
        }
        func restyle(_ id: Int, _ filter: [String: Any]?, alpha: Double) throws {
            capturedChildPixels = []
            p.apply(wireBatch([["op": "style", "id": id, "style": filter.map { ["filter": $0] } ?? [:]]]))
            waitForMainQueue()
            let pixel = try XCTUnwrap(capturedChildPixels?.last, "the style batch uploads a texture")
            let cpu = try XCTUnwrap(Bitmap.blank(width: 100, height: 100))
            Capture.draw(child, scale: 1, into: cpu)
            let bytes = try XCTUnwrap(cpu.bytes).assumingMemoryBound(to: UInt8.self)
            let cpuAlpha = bytes[(50 * 100 + 50) * 4 + 3]
            let state = "node \(id), filter \(String(describing: p.views[UInt32(id)]?.boxFilter?.opacity)), captured \(pixel), CPU alpha \(cpuAlpha)"
            XCTAssertEqual(Double(cpuAlpha), alpha * 255, accuracy: 1, state)
            XCTAssertEqual(Double(pixel[3]), alpha * 255, accuracy: 1, state)
            XCTAssertEqual(pixel[0], pixel[3], "the red child remains visible")
            XCTAssertFalse(d.canvas.needsCapture, "the capture settles")
        }
        try restyle(101, opacity(0.5), alpha: 0.5)
        try restyle(101, opacity(0.25), alpha: 0.25)
        try restyle(101, nil, alpha: 1)
        p.apply(wireBatch([["op": "present", "id": 102, "property": "opacity", "x": 0.8]]))
        try restyle(102, opacity(0.5), alpha: 0.4)
        try restyle(102, opacity(0.25), alpha: 0.2)
        try restyle(102, nil, alpha: 0.8)
        p.apply(wireBatch([["op": "present", "id": 102, "property": "opacity", "x": 1.0]]))
        let sub: [Double] = [-4, -4, 8, 8]
        let shadow: [Double] = sub + [1, 6, -1, -3] + sub + [0, 2, 2, 0, 0, 0, 0, 0, 1]
        try restyle(101, ["p": shadow], alpha: 1)
        XCTAssertTrue(child.boxFilter?.casts == true)
    }

    func testUnchangedLayerFilterClipsDoNotRecaptureCanvasTextures() throws {
        let d = try deck("canvas-layer-filter-clips")
        defer { d.session.destroy() }
        try XCTUnwrap(d.host.entries[100]).through = true
        let p = d.session.presenter
        let child = node(101, p, CGRect(x: 40, y: 40, width: 100, height: 100), in: d.overlay)
        paint(.red, into: child)
        placedIndices = [0]
        let opacity: [Double] = [0, 0, 0, 0, 1, 9, -1, -3, -1e6, -1e6, 2e6, 2e6, 0, 0, 0, 0, 1, 2, 0, 0.5]
        let sub: [Double] = [-4, -4, 8, 8]
        let shadow: [Double] = sub + [1, 6, -1, -3] + sub + [0, 2, 2, 0, 0, 0, 0, 0, 1]
        let clips: [[String: Any]] = [
            ["clip_path": ["commands": [["M", [0, 0]], ["L", [100, 0]], ["L", [0, 100]], ["Z", []]]]],
            ["mask_image": ["linear": 90, "stops": [0, 0, 0, 0, 0, 1, 0, 0, 0, 255]]],
            ["overflow_x": "hidden", "overflow_y": "hidden", "corner_shape": [0, 0, 0, 0], "border_radius": 20],
        ]
        for program in [opacity, shadow] {
            for clip in clips {
                if program == shadow, clip["corner_shape"] != nil { continue }
                var style = clip
                style["filter"] = ["p": program]
                p.apply(wireBatch([["op": "style", "id": 101, "style": style]]))
                d.frame()
                waitForMainQueue()
                let mask = try XCTUnwrap(child.ownMask)
                XCTAssertNil(child.filterPicture)
                capturedChildPixels = []
                for _ in 0..<3 { p.apply(wireBatch([])) }
                waitForMainQueue()
                XCTAssertTrue(child.ownMask === mask, "an unchanged clip reuses its mask")
                XCTAssertTrue(capturedChildPixels?.isEmpty == true, "unrelated batches upload no texture")
                XCTAssertFalse(d.canvas.needsCapture)
                p.apply(wireBatch([["op": "frame", "id": 101, "x": 40, "y": 40, "w": 90, "h": 90]]))
                waitForMainQueue()
                XCTAssertFalse(child.ownMask === mask, "a size change rebuilds the mask")
                XCTAssertFalse(capturedChildPixels?.isEmpty == true, "a changed clip uploads its texture")
                p.apply(wireBatch([["op": "frame", "id": 101, "x": 40, "y": 40, "w": 100, "h": 100]]))
                waitForMainQueue()
            }
        }
    }

    /// A CPU capture ignores root opacity without changing the child's
    /// transparency or asking its canvas to capture again. An author's
    /// change still asks, whether placed or left in place.
    func testTheCPUCaptureOfATransparentChildSettles() throws {
        let d = try deck("canvas-transparent-capture")
        defer { d.session.destroy() }
        let p = d.session.presenter
        let child = node(101, p, CGRect(x: 0, y: 0, width: 100, height: 100), in: d.overlay)
        paint(.red, into: child)
        child.alpha = 0
        for placed in [false, true] {
            placedIndices = placed ? [0] : []
            d.frame()
            d.canvas.needsCapture = false
            let bitmap = try XCTUnwrap(Bitmap.blank(width: 100, height: 100))
            for _ in 0..<3 { Capture.draw(child, scale: 1, into: bitmap) }
            XCTAssertFalse(d.canvas.needsCapture, "the capture asks for no capture, placed \(placed)")
            RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.05)) // what was scheduled runs
            XCTAssertFalse(d.canvas.needsCapture, "and none is pending, placed \(placed)")
        }
        placedIndices = []
        d.frame()
        d.canvas.needsCapture = false
        child.alpha = 1
        XCTAssertTrue(d.canvas.needsCapture, "the author's change asks, which lists the canvas's children again")
    }

    /// A filtered child is captured as its filter's picture, so new pixels in
    /// that picture, drawn at once or landed from the filter's chain later,
    /// ask its canvas to capture again. (The chain's pixels themselves: this
    /// simulator's draws none in a test, so what is checked is the asking.)
    func testAFilteredChildsNewPixelsAskItsCanvasToCapture() throws {
        let sub: [Double] = [-10, -10, 20, 20]
        let program: [Double] = sub + [1] + [0, -1, -3] + sub + [0, 2, 2]
        guard SvgFilterGPU.runs(program.map(Float.init)) else { throw missing("metal", "no GPU filter path on this host") }
        let d = try deck("canvas-filter-published")
        defer { d.session.destroy() }
        let p = d.session.presenter
        let child = node(101, p, CGRect(x: 40, y: 40, width: 100, height: 100), in: d.overlay)
        let fill = UIView(frame: child.bounds); fill.backgroundColor = .red
        child.addSubview(fill)
        placedIndices = [0]
        d.frame()
        d.canvas.needsCapture = false
        // Outside a batch, its first picture is drawn at once.
        child.applyStyle(["filter": .object(["p": .array(program.map { .number($0) })])])
        XCTAssertNotNil(child.filterPicture)
        XCTAssertTrue(d.canvas.needsCapture, "its first pixels ask")
        d.canvas.needsCapture = false
        fill.backgroundColor = .blue
        child.renderFilter() // drawn off the main thread, landing later
        let end = Date(timeIntervalSinceNow: 20)
        repeat { RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.01)) } while p.boxFilters.drawing && Date() < end
        XCTAssertFalse(p.boxFilters.drawing)
        XCTAssertTrue(d.canvas.needsCapture, "and so do the ones that land")
    }

    /// A context menu's lift copies a run's ink at the opacity its layers
    /// show it at (`ContextMenuHost.shownOpacity`, which `words` assigns): a
    /// placed child's is 0, since the surface draws it elsewhere; unplaced,
    /// its own.
    func testAPlacedChildShowsNoOpacityToALiftsCopies() throws {
        let d = try deck("canvas-placed-lift")
        defer { d.session.destroy() }
        let p = d.session.presenter
        let child = node(101, p, CGRect(x: 0, y: 0, width: 100, height: 100), in: d.overlay)
        let words = node(102, p, CGRect(x: 0, y: 0, width: 100, height: 20), in: child)
        child.alpha = 0.5
        let chain = { ContextMenuHost.shownOpacity(of: words.layer, in: d.overlay.layer) }
        XCTAssertEqual(chain(), 0.5, accuracy: 0.001)
        placedIndices = [0]
        d.frame()
        XCTAssertEqual(chain(), 0, "a lift shows nothing of a placed child's run")
        placedIndices = []
        d.frame()
        XCTAssertEqual(chain(), 0.5, accuracy: 0.001, "unplaced, its own opacity again")
    }

    /// A scroller inside a placed card keeps its control-bar band: the card's
    /// own mask is none, whatever hides it on screen (`boxMask`). It guards
    /// that reading, not #69.
    func testAPlacedChildsEmptyMaskIsNoClipToItsControlBars() throws {
        let d = try deck("canvas-placed-bars")
        defer { d.session.destroy() }
        let p = d.session.presenter
        let card = node(101, p, CGRect(x: 0, y: 0, width: 200, height: 200), in: d.overlay)
        let list = node(102, p, CGRect(x: 0, y: 0, width: 200, height: 120), in: card)
        placedIndices = [0]
        d.frame()
        XCTAssertNotNil(card.placement)
        XCTAssertNotNil(ControlBars(p).bars(of: list, in: card, top: true), "the placed card clips nothing")
    }

    /// A control inside a placed child is where the surface draws it, not at
    /// its own frame, where focus would look: no focus item until focus is
    /// mapped through the placement. One in an unplaced child still is. It
    /// guards the focus search's skip, not #69. tvOS's own search
    /// (`NodeView.focusItems(in:)`) is compiled for tvOS only and not run here.
    func testFocusOffersNoControlInsideAPlacedChild() throws {
        let d = try deck("canvas-placed-focus")
        defer { d.session.destroy() }
        let p = d.session.presenter
        let placed = node(101, p, CGRect(x: 0, y: 0, width: 200, height: 100), in: d.overlay)
        let unplaced = node(102, p, CGRect(x: 0, y: 200, width: 200, height: 100), in: d.overlay)
        let inside = UISwitch(), beside = UISwitch()
        placed.addSubview(inside); unplaced.addSubview(beside)
        placedIndices = [0]
        d.frame()
        let items = FocusSearch.items(p.viewport, in: p.viewport.bounds).map { $0 as AnyObject }
        XCTAssertFalse(items.contains { $0 === inside }, "the placed child's switch is no focus item")
        XCTAssertTrue(items.contains { $0 === beside }, "the unplaced child's is")
    }
}
#endif
