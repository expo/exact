#if os(iOS)
import UIKit
import XCTest
@testable import ExactDragOut
@testable import ExactKit

/// The shape UIKit lifts a context menu's node in (`ContextMenuHost.targeted`)
/// when the node has a clip path: what the path leaves of its box, as a chat
/// balloon's outline, its body and its tail (LLP 1021 §5.1).
final class ContextLiftShapeIOSTests: XCTestCase {
    private var window: UIWindow!
    private var session: ExactSession?
    private var asked: (UIContextMenuInteraction, UIContextMenuConfiguration)?

    override func tearDown() { session?.destroy(); session = nil; window?.isHidden = true; super.tearDown() }

    /// A balloon at (100, 100), 120 by 48, naming the menu: a blue body 40
    /// high, a blue tail in the 8 points below it, and its text. `clipped`
    /// gives the balloon the clip path of its body and its tail, or `clip`;
    /// `height` 40 leaves the tail past its box.
    private func balloon(clipped: Bool, height: Double = 48, clip: [[Any]]? = nil) throws -> Presenter {
        let session = ExactApp.shared.makeSession(label: "lift-shape")
        self.session = session
        let p = session.presenter
        window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 800))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        let blue: [Double] = [0, 136, 255, 255]
        let outline: [[Any]] = [["M", [0, 0]], ["L", [120, 0]], ["L", [120, 40]], ["L", [0, 40]], ["Z", []],
                                ["M", [104, 40]], ["L", [120, 40]], ["L", [120, 48]], ["Z", []]]
        let tail: [String: Any] = ["commands": [["M", [104, 40]], ["L", [120, 40]], ["L", [120, 48]], ["Z", []]]]
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view", "props": ["contextPopover": "message-menu"], "handlers": ["contextmenu"],
             "style": clipped ? ["clip_path": ["commands": clip ?? outline]] : [:]],
            ["op": "create", "id": 3, "kind": "view", "style": ["background_color": blue, "clip_path": tail]],
            ["op": "create", "id": 4, "kind": "view", "style": ["background_color": blue]],
            ["op": "create", "id": 5, "kind": "text", "props": ["text": "Hey there"], "style": ["font_size": 17.0]],
            ["op": "create", "id": 6, "kind": "view", "props": ["popover": "auto", "id": "message-menu"]],
            ["op": "create", "id": 7, "kind": "button", "handlers": ["press"]],
            ["op": "children", "id": 6, "ids": [7]],
            ["op": "children", "id": 2, "ids": [3, 4, 5]],
            ["op": "children", "id": 1, "ids": [2, 6]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 400, "h": 800],
            ["op": "frame", "id": 2, "x": 100, "y": 100, "w": 120, "h": height],
            ["op": "frame", "id": 3, "x": 0, "y": 0, "w": 120, "h": 48],
            ["op": "frame", "id": 4, "x": 0, "y": 0, "w": 120, "h": 40],
            ["op": "frame", "id": 5, "x": 14, "y": 8, "w": 92, "h": 24],
        ]))
        p.buttonFace = { _ in var face = ButtonFace(); face.title = "Copy"; return face }
        p.menus.context.sync()
        return p
    }

    private func lifted(_ p: Presenter) throws -> UITargetedPreview {
        let node = try XCTUnwrap(p.views[2])
        let interaction = try XCTUnwrap(node.interactions.compactMap { $0 as? UIContextMenuInteraction }.first)
        let config = try XCTUnwrap(p.menus.context.contextMenuInteraction(interaction, configurationForMenuAtLocation: CGPoint(x: 60, y: 20)))
        asked = (interaction, config)
        return try XCTUnwrap(p.menus.context.contextMenuInteraction(interaction, configuration: config, highlightPreviewForItemWithIdentifier: config.identifier))
    }

    /// The preview UIKit returns the balloon in, as the menu closes.
    private func dismissed(_ p: Presenter) throws -> UITargetedPreview {
        let (interaction, config) = try XCTUnwrap(asked)
        return try XCTUnwrap(p.menus.context.contextMenuInteraction(interaction, configuration: config, dismissalPreviewForItemWithIdentifier: config.identifier))
    }

    func testABalloonLiftsInItsClipPathBodyAndTail() throws {
        if ExactEnv.agentMode { throw XCTSkip("under the agent the popover is painted (D4)") }
        let p = try balloon(clipped: true)
        defer { p.menus.reset() }
        let preview = try lifted(p)
        let path = try XCTUnwrap(preview.parameters.visiblePath)
        XCTAssertTrue(path.contains(CGPoint(x: 60, y: 20)), "the body")
        XCTAssertTrue(path.contains(CGPoint(x: 118, y: 44)), "the tail")
        XCTAssertFalse(path.contains(CGPoint(x: 4, y: 44)), "beside the tail, under the body")
        XCTAssertEqual(preview.parameters.backgroundColor, .clear, "the screen, not UIKit's white, where it paints nothing")
        let back = try XCTUnwrap(dismissed(p).parameters.visiblePath)
        XCTAssertTrue(back.contains(CGPoint(x: 118, y: 44)) && !back.contains(CGPoint(x: 4, y: 44)), "it returns in the shape it lifted in")
    }

    func testAClipPathPastTheBoxIsCutToIt() throws {
        if ExactEnv.agentMode { throw XCTSkip("under the agent the popover is painted (D4)") }
        let p = try balloon(clipped: true, height: 40)
        defer { p.menus.reset() }
        let path = try XCTUnwrap(try lifted(p).parameters.visiblePath)
        XCTAssertTrue(path.contains(CGPoint(x: 60, y: 20)), "the body")
        XCTAssertFalse(path.contains(CGPoint(x: 118, y: 44)), "the tail, past the box, as the box's overflow")
    }

    /// While layout grows the box, it shows at its surface's size, not the
    /// size it grows to (LLP 1063), and lifts cut to what shows.
    func testABoxLayoutIsResizingLiftsCutToWhatShows() throws {
        if ExactEnv.agentMode { throw XCTSkip("under the agent the popover is painted (D4)") }
        let p = try balloon(clipped: true)
        defer { p.menus.reset() }
        p.apply(wireBatch([["op": "present", "id": 2, "property": "layout", "x": 0, "y": 0, "w": 0, "h": -8]]))
        let path = try XCTUnwrap(try lifted(p).parameters.visiblePath)
        XCTAssertTrue(path.contains(CGPoint(x: 60, y: 20)), "the body")
        XCTAssertFalse(path.contains(CGPoint(x: 118, y: 44)), "the tail, below the box as it shows")
    }

    func testAClipPathThatLeavesNothingOfTheBoxKeepsItsBoxsLift() throws {
        if ExactEnv.agentMode { throw XCTSkip("under the agent the popover is painted (D4)") }
        let below: [[Any]] = [["M", [0, 44]], ["L", [10, 44]], ["L", [10, 48]], ["Z", []]]
        let p = try balloon(clipped: true, height: 40, clip: below)
        defer { p.menus.reset() }
        XCTAssertNil(try lifted(p).parameters.visiblePath, "an empty shape would lift nothing")
    }

    /// A drag from the node lifts in the outline its menu does, so the menu's
    /// highlight becomes the drag with one preview (LLP 1094.000 D4).
    func testADragLiftsInTheMenusOutline() throws {
        if ExactEnv.agentMode { throw XCTSkip("under the agent the popover is painted (D4)") }
        let p = try balloon(clipped: true)
        defer { p.menus.reset() }
        let menu = try lifted(p).parameters
        let drag = try XCTUnwrap(DragSources.preview(of: try XCTUnwrap(p.views[2]))).parameters
        XCTAssertEqual(drag.visiblePath?.cgPath, menu.visiblePath?.cgPath)
        XCTAssertEqual(drag.backgroundColor, menu.backgroundColor)
    }

    func testANodeWithNoClipPathKeepsItsBoxsLift() throws {
        if ExactEnv.agentMode { throw XCTSkip("under the agent the popover is painted (D4)") }
        let p = try balloon(clipped: false)
        defer { p.menus.reset() }
        XCTAssertNil(try lifted(p).parameters.visiblePath, "a square box with no clip path lifts in its box")
    }
}
#endif
