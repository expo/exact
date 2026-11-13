#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// LLP 1053.000.000.001 on UIKit: a glass a person's input adds to a group
/// grows out of the nearest glass within the group's spacing, and one it
/// removes is drawn back into that glass as it dematerializes (#121).
/// UIKit, so a simulator runs it: bun host/apple/build.mjs --test --ios
final class GlassMorphIOSTests: XCTestCase {
    private var window: UIWindow!

    override func setUpWithError() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("Liquid Glass is iOS 26") }
        if UIAccessibility.isReduceMotionEnabled { throw XCTSkip("Reduce Motion draws glass at once") }
    }

    private func presenter(_ ops: [[String: Any]]) -> Presenter {
        let p = Presenter()
        window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        p.apply(wireBatch(ops))
        return p
    }
    private func box(_ id: Int, _ props: [String: String] = [:], x: Double = 0, w: Double = 40) -> [[String: Any]] {
        [["op": "create", "id": id, "kind": "view", "props": props, "handlers": [String](),
          "style": ["border_radius": 20.0, "text_color": [0, 0, 0, 255]]],
         ["op": "frame", "id": id, "x": x, "y": 0.0, "w": w, "h": 40.0]]
    }
    private func glass(_ id: Int, x: Double, w: Double = 40) -> [[String: Any]] { box(id, ["backgroundMaterial": "glass"], x: x, w: w) }
    /// A group of spacing 12 holding a pill (2), 0 to 120.
    private func group() -> Presenter {
        presenter(box(1, ["glassGroup": "12"], w: 400) + glass(2, x: 0, w: 120)
            + [["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]]])
    }
    /// A batch a person's input caused, marked as `ExactSession.applyInput` marks it.
    private func pressed(_ ops: [[String: Any]]) -> Batch {
        var batch = wireBatch(ops)
        batch.input = CACurrentMediaTime() + 1
        return batch
    }
    private func ghosts(_ p: Presenter) throws -> [GlassGhost] {
        try XCTUnwrap(p.views[1]?.glassGroupView).contentView.subviews.compactMap { $0 as? GlassGhost }
    }

    /// Within the spacing it starts as a bead on the pill's edge and grows to
    /// its own box; beyond it, it materializes in place.
    func testAJoiningGlassGrowsOutOfTheGlassItMergesWith() throws {
        let p = group()
        p.apply(pressed(glass(3, x: 128) + glass(4, x: 300) + [["op": "children", "id": 1, "ids": [2, 3, 4]]]))
        let near = try XCTUnwrap(p.views[3]?.materialView), far = try XCTUnwrap(p.views[4]?.materialView)
        XCTAssertEqual(near.frame.size, CGSize(width: 40, height: 40), "it rests at its own box")
        // UIKit's animations are additive: each starts at its model value plus `fromValue`.
        let grows = try XCTUnwrap(near.layer.animation(forKey: "position") as? CABasicAnimation, "8 points from the pill, it grows out of it")
        XCTAssertTrue(grows.isAdditive)
        let shift = try XCTUnwrap((grows.fromValue as? NSValue)?.cgPointValue)
        let start = CGPoint(x: near.layer.position.x + shift.x, y: near.layer.position.y + shift.y)
        let inGroup = try XCTUnwrap(near.superview).convert(start, to: try XCTUnwrap(p.views[1]?.glassGroupView).contentView)
        XCTAssertEqual(inGroup.x, 120, accuracy: 0.5, "from the pill's edge")
        XCTAssertEqual(inGroup.y, 20, accuracy: 0.5, "level with it")
        let grown = try XCTUnwrap((near.layer.animation(forKey: "bounds.size") as? CABasicAnimation)?.fromValue as? NSValue).cgSizeValue
        XCTAssertEqual(40 + grown.width, 20, accuracy: 0.5, "a bead half the smaller side")
        XCTAssertNotNil(near.contentView.layer.animation(forKey: "opacity"), "its content fades in")
        XCTAssertNil(far.layer.animation(forKey: "position"), "180 points away it does not move")
        XCTAssertNotNil(far.contentView.layer.animation(forKey: "opacity"), "and materializes in place")
        XCTAssertNotNil(far.effect)
    }

    /// Two glass added by one input both grow out of the glass that was
    /// there before it, not out of each other.
    func testGlassAddedTogetherGrowOutOfTheGlassThatWasThere() throws {
        let p = group()
        p.apply(pressed(glass(3, x: 128) + glass(4, x: 176) + [["op": "children", "id": 1, "ids": [2, 3, 4]]]))
        XCTAssertNotNil(p.views[3]?.materialView?.layer.animation(forKey: "position"), "8 points from the pill")
        XCTAssertNil(p.views[4]?.materialView?.layer.animation(forKey: "position"), "56 points from the pill, 8 from a glass added with it")
    }

    /// A glass the input removes is drawn into the pill as a paint-only copy
    /// that dematerializes; a removal no input made leaves at once.
    func testALeavingGlassIsDrawnIntoTheGlassItMergedWith() throws {
        let p = group()
        p.apply(wireBatch(glass(3, x: 128) + glass(4, x: 300) + glass(5, x: 172) + [["op": "children", "id": 1, "ids": [2, 3, 4, 5]]]))
        p.apply(wireBatch([["op": "destroy", "id": 5], ["op": "children", "id": 1, "ids": [2, 3, 4]]]))
        XCTAssertTrue(try ghosts(p).isEmpty, "no input removed it: gone at once")
        // The host sends a receipt's destroys before its children (`commit_tree`).
        p.apply(pressed([["op": "destroy", "id": 3], ["op": "destroy", "id": 4], ["op": "children", "id": 1, "ids": [2]]]))
        XCTAssertNil(p.views[3]); XCTAssertNil(p.views[4])
        let left = try ghosts(p)
        XCTAssertEqual(left.count, 2, "each leaves a paint-only copy")
        let near = try XCTUnwrap(left.first { $0.layer.animation(forKey: "transform") != nil }, "the one beside the pill moves")
        XCTAssertEqual(near.frame.midX, 120, accuracy: 0.5, "into the pill's edge")
        XCTAssertEqual(near.frame.width, 20, accuracy: 0.5, "as a bead")
        XCTAssertNil(near.effect, "dematerializing as it goes")
        let far = try XCTUnwrap(left.first { $0 !== near })
        XCTAssertEqual(far.frame.minX, 300, accuracy: 0.5, "one with no glass within the spacing dematerializes in place")
        XCTAssertNil(far.effect)
        for ghost in left {
            XCTAssertFalse(ghost.isUserInteractionEnabled)
            XCTAssertTrue(ghost.accessibilityElementsHidden)
            XCTAssertNil(ghost.hitTest(CGPoint(x: 5, y: 5), with: nil))
        }
    }

    /// A glass its author hid (`visibility: hidden`, its effect set to nil)
    /// draws nothing as it leaves, and is no native glass button.
    func testAHiddenGlassLeavesNothing() throws {
        let p = group()
        var hidden = glass(5, x: 128)
        hidden[0]["style"] = ["border_radius": 20.0, "text_color": [0, 0, 0, 255], "visibility": "hidden"]
        p.apply(wireBatch(hidden + [["op": "children", "id": 1, "ids": [2, 5]]]))
        XCTAssertNil(p.views[5]?.materialView?.effect, "hidden, its glass has no effect")
        p.apply(pressed([["op": "destroy", "id": 5], ["op": "children", "id": 1, "ids": [2]]]))
        XCTAssertTrue(try ghosts(p).isEmpty, "nothing is drawn into the pill")
    }

    /// What a leaving glass shows on it goes with it, a decorative child the
    /// presenter mounted as a bare layer (a flat leaf) included.
    func testALeavingGlassTakesItsFlatChildrenWithIt() throws {
        let p = group()
        let dot: [[String: Any]] = [
            ["op": "create", "id": 6, "kind": "view",
             "style": ["background_color": [255, 0, 0, 255], "text_color": [0, 0, 0, 255], "written": ["background_color"]]],
            ["op": "frame", "id": 6, "x": 10.0, "y": 10.0, "w": 20.0, "h": 20.0]]
        p.apply(wireBatch(glass(5, x: 128) + dot + [["op": "children", "id": 5, "ids": [6]], ["op": "children", "id": 1, "ids": [2, 5]]]))
        XCTAssertTrue(p.flats.isFlat(6), "the dot is a bare layer in the glass's content")
        p.apply(pressed([["op": "destroy", "id": 5], ["op": "destroy", "id": 6], ["op": "children", "id": 1, "ids": [2]]]))
        let ghost = try XCTUnwrap(try ghosts(p).first)
        let picture = try XCTUnwrap(ghost.contentView.subviews.first as? UIImageView)
        let cg = try XCTUnwrap(picture.image?.cgImage)
        let scale = CGFloat(cg.width) / picture.bounds.width
        let data = try XCTUnwrap(cg.dataProvider?.data), bytes = try XCTUnwrap(CFDataGetBytePtr(data))
        func pixel(_ x: CGFloat, _ y: CGFloat) -> [UInt8] {
            let at = Int(y * scale) * cg.bytesPerRow + Int(x * scale) * cg.bitsPerPixel / 8
            return (0..<4).map { bytes[at + $0] }
        }
        let inside = pixel(20, 20), outside = pixel(35, 35)
        XCTAssertGreaterThan(inside[3], 200, "the dot is in the picture: \(inside)")
        XCTAssertEqual(Set([inside[0], inside[2]]).contains(255), true, "red, in either channel order: \(inside)")
        XCTAssertLessThan(outside[3], 20, "and nothing of the glass around it: \(outside)")
    }

    /// The copy shows a descendant as it is drawn mid-animation, not at the
    /// value its animation ends at: here the dot is held transparent by a
    /// running animation over its opaque model.
    func testALeavingGlassIsCopiedAsItIsShownMidAnimation() throws {
        // Only a window the scene draws commits its animations, which a presentation then shows.
        guard let scene = UIApplication.shared.connectedScenes.compactMap({ $0 as? UIWindowScene }).first else {
            throw missing("scene", "a running animation needs a window the scene draws")
        }
        let p = group()
        window.windowScene = scene
        window.makeKeyAndVisible()
        defer { window.isHidden = true }
        let dot: [[String: Any]] = [
            ["op": "create", "id": 6, "kind": "view",
             "style": ["background_color": [255, 0, 0, 255], "text_color": [0, 0, 0, 255], "written": ["background_color"]]],
            ["op": "frame", "id": 6, "x": 10.0, "y": 10.0, "w": 20.0, "h": 20.0]]
        p.apply(wireBatch(glass(5, x: 128) + dot + [["op": "children", "id": 5, "ids": [6]], ["op": "children", "id": 1, "ids": [2, 5]]]))
        let content = try XCTUnwrap(p.views[5]?.materialView?.contentView)
        let hold = CABasicAnimation(keyPath: "opacity")
        (hold.fromValue, hold.toValue, hold.duration) = (0, 0, 60)
        for layer in content.layer.sublayers ?? [] { layer.add(hold, forKey: "hold") }
        CATransaction.flush()
        RunLoop.main.run(until: Date().addingTimeInterval(0.1))
        XCTAssertTrue((content.layer.sublayers ?? []).contains { $0.presentation()?.opacity == 0 && $0.opacity == 1 }, "the dot is shown transparent over its opaque model")
        p.apply(pressed([["op": "destroy", "id": 5], ["op": "destroy", "id": 6], ["op": "children", "id": 1, "ids": [2]]]))
        let picture = try XCTUnwrap(try ghosts(p).first?.contentView.subviews.first as? UIImageView)
        let cg = try XCTUnwrap(picture.image?.cgImage)
        let scale = CGFloat(cg.width) / picture.bounds.width
        let bytes = try XCTUnwrap(CFDataGetBytePtr(try XCTUnwrap(cg.dataProvider?.data)))
        let alpha = bytes[Int(20 * scale) * cg.bytesPerRow + Int(20 * scale) * cg.bitsPerPixel / 8 + 3]
        XCTAssertLessThan(alpha, 20, "drawn as shown, transparent, not at its opaque model")
    }
}
#endif
