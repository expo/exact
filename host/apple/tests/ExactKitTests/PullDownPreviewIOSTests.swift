#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// A custom invoker's pull-down grows out of the invoker and shrinks back
/// into it, as a stock button's menu does (#117): UIKit previews the
/// invoker node, clipped to its corners, not the transparent overlay that
/// holds the menu, and hides it while the menu shows. What UIKit draws in
/// its place leaves out the invoker's own layer fill, so a filled invoker
/// lends its fill to a sublayer until the menu has ended. UIKit, so a
/// simulator runs it: `bun host/apple/build.mjs --test --ios --only PullDownPreviewIOSTests`.
final class PullDownPreviewIOSTests: XCTestCase {
    private var window: UIWindow!

    private func presenter(glass: Bool = true) -> Presenter {
        let p = Presenter()
        window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        // A glass pill, or a page-drawn one the test fills.
        let props: [String: Any] = glass ? ["popovertarget": "m", "backgroundMaterial": "glass"] : ["popovertarget": "m"]
        // A screen (1): a glass pill (2) that opens its menu (3) of one row (4).
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "button", "props": props],
            ["op": "style", "id": 2, "style": ["border_radius": 22]],
            ["op": "create", "id": 3, "kind": "view", "props": ["id": "m", "popover": "auto", "role": "menu"]],
            ["op": "create", "id": 4, "kind": "button", "handlers": ["press"], "props": ["role": "menuitem", "popovertarget": "m", "popovertargetaction": "hide"]],
            ["op": "children", "id": 3, "ids": [4]],
            ["op": "children", "id": 1, "ids": [2, 3]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 400.0, "h": 400.0],
            ["op": "frame", "id": 2, "x": 100.0, "y": 60.0, "w": 120.0, "h": 44.0],
            ["op": "frame", "id": 3, "x": 100.0, "y": 108.0, "w": 200.0, "h": 44.0],
            ["op": "frame", "id": 4, "x": 0.0, "y": 0.0, "w": 200.0, "h": 44.0],
        ]))
        return p
    }

    func testThePullDownPreviewsItsInvoker() throws {
        let p = presenter()
        defer { p.menus.reset() }
        let invoker = try XCTUnwrap(p.views[2])
        let button = try XCTUnwrap(invoker.subviews.compactMap { $0 as? MenuButton }.first)
        let interaction = try XCTUnwrap(button.contextMenuInteraction)
        let configuration = UIContextMenuConfiguration(identifier: nil, previewProvider: nil) { _ in button.menu }
        let previews = [
            ("highlight", button.contextMenuInteraction(interaction, previewForHighlightingMenuWithConfiguration: configuration)),
            ("dismissal", button.contextMenuInteraction(interaction, previewForDismissingMenuWithConfiguration: configuration)),
        ]
        for (name, preview) in previews {
            let preview = try XCTUnwrap(preview, name)
            XCTAssertTrue(preview.view === invoker, "\(name): the invoker, not the overlay")
            let path = try XCTUnwrap(preview.parameters.visiblePath, name)
            XCTAssertEqual(path.bounds, invoker.bounds, name)
            XCTAssertFalse(path.contains(CGPoint(x: 1, y: 1)), "\(name): clipped to the pill's corners")
            XCTAssertTrue(path.contains(CGPoint(x: 60, y: 22)), name)
            XCTAssertEqual(preview.parameters.backgroundColor, .clear, "\(name): nothing drawn behind the glass")
            let center = invoker.convert(CGPoint(x: invoker.bounds.midX, y: invoker.bounds.midY), to: preview.target.container)
            XCTAssertEqual(preview.target.center, center, "\(name): where the invoker is")
            XCTAssertTrue(preview.target.container === invoker.superview, "\(name): over its parent, so it scrolls with it")
        }
    }

    /// An invoker a clipping box already cuts (half scrolled out of a strip)
    /// is previewed over the controller's view, so the stand-in is not cut.
    func testAPartlyClippedInvokersPreviewStandsAboveTheClip() throws {
        let p = presenter()
        defer { p.menus.reset() }
        let invoker = try XCTUnwrap(p.views[2])
        let screen = try XCTUnwrap(invoker.superview)
        let strip = UIView(frame: CGRect(x: 100, y: 60, width: 60, height: 44))
        strip.clipsToBounds = true
        screen.addSubview(strip)
        invoker.removeFromSuperview()
        invoker.frame = CGRect(x: 0, y: 0, width: 120, height: 44)
        strip.addSubview(invoker)
        XCTAssertTrue(LiftTarget.pullDownContainer(of: invoker) === LiftTarget.container(of: invoker), "cut by the strip: above it")
        strip.frame.size.width = 200
        XCTAssertTrue(LiftTarget.pullDownContainer(of: invoker) === strip, "whole inside the strip: its parent")
    }

    /// The same for an `overflow: hidden` box: it clips as its layer, or its
    /// clip box, masks to bounds.
    func testAnInvokerAnOverflowHiddenBoxCutsPreviewsAboveTheClip() throws {
        let p = presenter()
        defer { p.menus.reset() }
        let controller = UIViewController()
        window.rootViewController = controller
        controller.view.addSubview(p.viewport)
        p.apply(wireBatch([
            ["op": "create", "id": 5, "kind": "view"],
            ["op": "style", "id": 5, "style": ["overflow_x": "hidden", "overflow_y": "hidden"]],
            ["op": "children", "id": 5, "ids": [2]],
            ["op": "children", "id": 1, "ids": [5, 3]],
            ["op": "frame", "id": 5, "x": 100.0, "y": 60.0, "w": 60.0, "h": 44.0],
            ["op": "frame", "id": 2, "x": 0.0, "y": 0.0, "w": 120.0, "h": 44.0],
        ]))
        let invoker = try XCTUnwrap(p.views[2])
        XCTAssertTrue(LiftTarget.pullDownContainer(of: invoker) === controller.view, "cut by the box: above it")
        p.apply(wireBatch([["op": "frame", "id": 5, "x": 100.0, "y": 60.0, "w": 200.0, "h": 44.0]]))
        XCTAssertTrue(LiftTarget.pullDownContainer(of: invoker) === invoker.superview, "whole inside the box: its parent")
    }

    /// A page-drawn pill (`appearance: none`) filled 8 % white, its menu
    /// opened and then ended; `restyle` runs while the menu has it.
    private func lendAndEnd(_ restyle: (Presenter, NodeView) throws -> Void) throws -> (invoker: NodeView, lent: CAShapeLayer, during: CGColor?) {
        let p = presenter(glass: false)
        defer { p.menus.reset() }
        let invoker = try XCTUnwrap(p.views[2])
        p.apply(wireBatch([["op": "style", "id": 2, "style": ["appearance": "none", "border_radius": 22, "background_color": [255, 255, 255, 20]]]]))
        invoker.layer.displayIfNeeded()
        XCTAssertEqual(invoker.layer.backgroundColor?.alpha ?? 0, 20.0 / 255, accuracy: 0.01, "the authored fill is the layer's")
        let button = try XCTUnwrap(invoker.subviews.compactMap { $0 as? MenuButton }.first)
        let interaction = try XCTUnwrap(button.contextMenuInteraction)
        let configuration = UIContextMenuConfiguration(identifier: nil, previewProvider: nil) { _ in button.menu }
        let preview = try XCTUnwrap(button.contextMenuInteraction(interaction, previewForHighlightingMenuWithConfiguration: configuration))
        XCTAssertTrue(preview.view === invoker, "the invoker itself, which UIKit hides and morphs")
        XCTAssertNil(invoker.layer.backgroundColor, "not on the layer UIKit's stand-in leaves out")
        let lent = try XCTUnwrap(invoker.layer.sublayers?.first as? CAShapeLayer)
        XCTAssertEqual(lent.fillColor?.alpha ?? 0, 20.0 / 255, accuracy: 0.01)
        button.contextMenuInteraction(interaction, willDisplayMenuFor: configuration, animator: nil)
        try restyle(p, invoker)
        invoker.layer.setNeedsDisplay()
        invoker.layer.displayIfNeeded()
        XCTAssertNil(invoker.layer.backgroundColor, "a repaint while the menu has it paints the lent fill")
        let during = lent.fillColor
        button.contextMenuInteraction(interaction, willEndFor: configuration, animator: nil)
        XCTAssertFalse(invoker.layer.sublayers?.contains(lent) ?? false)
        return (invoker, lent, during)
    }

    func testAFilledInvokersLentFillFollowsAStyleChangeAndGoesBack() throws {
        let (invoker, _, during) = try lendAndEnd { p, _ in
            p.apply(wireBatch([["op": "style", "id": 2, "style": ["appearance": "none", "border_radius": 22, "background_color": [255, 0, 0, 255]]]]))
        }
        XCTAssertEqual(during?.components?.first ?? 0, 1, accuracy: 0.01, "the stand-in shows the new fill")
        XCTAssertEqual(invoker.layer.backgroundColor?.components?.first ?? 0, 1, accuracy: 0.01, "and the layer has it after the menu")
    }

    func testABackgroundClearedWhileTheMenuShowsStaysCleared() throws {
        let (invoker, _, during) = try lendAndEnd { p, _ in
            p.apply(wireBatch([["op": "style", "id": 2, "style": ["appearance": "none", "border_radius": 22]]]))
        }
        XCTAssertNil(during, "the stand-in shows no fill")
        XCTAssertNil(invoker.layer.backgroundColor, "the menu's end restores no background the page cleared")
    }

    func testABoxLayerAddedWhileTheFillIsLentPaintsAboveIt() throws {
        _ = try lendAndEnd { p, invoker in
            p.apply(wireBatch([["op": "style", "id": 2, "style": ["appearance": "none", "border_radius": 22, "background_color": [255, 255, 255, 20],
                                                                   "border_radius_top_left": 22, "border_radius_bottom_right": 4,
                                                                   "background_image": ["linear": 180, "stops": [0, 5, 82, 240, 128, 1, 44, 107, 237, 128]]]]]))
            invoker.layer.setNeedsDisplay()
            invoker.layer.displayIfNeeded()
            let layers = invoker.layer.sublayers ?? []
            let lent = try XCTUnwrap(invoker.extras?.lentFill, "the fill is still lent")
            let gradient = try XCTUnwrap(invoker.boxGradient, "an uneven-corner box's gradient is a layer")
            let below = try XCTUnwrap(layers.firstIndex(of: lent)), above = try XCTUnwrap(layers.firstIndex(of: gradient))
            XCTAssertLessThan(below, above, "the gradient paints over the lent fill, as over the layer's own")
        }
    }

    /// A filled invoker that extends (LLP 1123 D13) lends the fill its picture holds, as one does that extends after the
    /// menu opens: its extension ends while the menu has the fill and comes back after it.
    func testAnExtendedInvokerLendsItsFillAndExtendsAgainAfter() throws {
        guard #available(iOS 26, *) else { throw XCTSkip("UIBackgroundExtensionView needs iOS 26") }
        let p = presenter(glass: false)
        defer { p.menus.reset() }
        let invoker = try XCTUnwrap(p.views[2])
        p.apply(wireBatch([["op": "style", "id": 2, "style": ["appearance": "none", "border_radius": 22, "background_color": [255, 0, 0, 255]]]]))
        invoker.layer.displayIfNeeded()
        invoker.setExtension(UIEdgeInsets(top: 20, left: 0, bottom: 0, right: 0))
        XCTAssertNotNil(invoker.boxExtension)
        XCTAssertNil(invoker.layer.backgroundColor, "the picture holds the fill")
        let button = try XCTUnwrap(invoker.subviews.compactMap { $0 as? MenuButton }.first)
        let interaction = try XCTUnwrap(button.contextMenuInteraction)
        let configuration = UIContextMenuConfiguration(identifier: nil, previewProvider: nil) { _ in button.menu }
        _ = try XCTUnwrap(button.contextMenuInteraction(interaction, previewForHighlightingMenuWithConfiguration: configuration))
        let lent = try XCTUnwrap(invoker.extras?.lentFill, "the fill is lent")
        XCTAssertEqual(lent.fillColor?.components?.first ?? 0, 1, accuracy: 0.01, "the picture's fill")
        XCTAssertNil(invoker.boxExtension, "no extension while the menu has the fill")
        button.contextMenuInteraction(interaction, willDisplayMenuFor: configuration, animator: nil)
        button.contextMenuInteraction(interaction, willEndFor: configuration, animator: nil)
        XCTAssertNil(invoker.extras?.lentFill)
        XCTAssertNotNil(invoker.boxExtension, "the extension comes back after the menu")
    }

    func testAnInvokerOutOfTheWindowHasNoPreview() throws {
        let p = presenter()
        defer { p.menus.reset() }
        let invoker = try XCTUnwrap(p.views[2])
        let button = try XCTUnwrap(invoker.subviews.compactMap { $0 as? MenuButton }.first)
        let interaction = try XCTUnwrap(button.contextMenuInteraction)
        let configuration = UIContextMenuConfiguration(identifier: nil, previewProvider: nil) { _ in button.menu }
        invoker.removeFromSuperview()
        XCTAssertNil(button.contextMenuInteraction(interaction, previewForDismissingMenuWithConfiguration: configuration))
        XCTAssertNil(button.contextMenuInteraction(interaction, previewForHighlightingMenuWithConfiguration: configuration))
    }
}
#endif
