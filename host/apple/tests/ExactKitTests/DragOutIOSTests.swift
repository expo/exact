#if os(iOS)
import UIKit
import UniformTypeIdentifiers
import XCTest
@testable import ExactCore
@testable import ExactDragOut
@testable import ExactKit

/// LLP 1094.000 D4 on UIKit: a node carrying `draggable=true` is a
/// `UIDragInteraction`, enabled on every device, whose items and lift are
/// built from the node's admitted props. A row view the node pool lends keeps
/// its interaction only for a draggable node. UIKit, so a simulator runs it:
/// bun host/apple/build.mjs --test --ios --only DragOutIOSTests
final class DragOutIOSTests: XCTestCase {
    private var window: UIWindow!

    private func presenter(_ ops: [[String: Any]]) -> Presenter {
        let p = Presenter()
        window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 800))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        p.apply(wireBatch(ops))
        return p
    }
    /// A card (`id`), draggable with `props`, holding a text.
    private func card(_ id: Int, _ props: [String: Any]) -> [[String: Any]] {
        [["op": "create", "id": id, "kind": "view", "props": props],
         ["op": "create", "id": id + 1, "kind": "text", "props": ["text": "Card"]],
         ["op": "children", "id": id, "ids": [id + 1]],
         ["op": "frame", "id": id, "x": 20.0, "y": 20.0, "w": 200.0, "h": 120.0],
         ["op": "frame", "id": id + 1, "x": 8.0, "y": 8.0, "w": 100.0, "h": 20.0]]
    }
    private func page(_ props: [String: Any]) -> Presenter {
        presenter(card(10, props) + [["op": "create", "id": 1, "kind": "view"], ["op": "children", "id": 1, "ids": [10]],
                                    ["op": "roots", "ids": [1]], ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 400.0, "h": 800.0]])
    }
    private func drags(_ v: UIView) -> [UIDragInteraction] { v.interactions.compactMap { $0 as? UIDragInteraction } }
    private func sources(_ p: Presenter) throws -> DragSources { try XCTUnwrap(p.dragOut as? DragSources) }
    /// Whether a touch at `point` in `node` keeps its contact from the lift.
    private func keepsContact(_ node: NodeView, at point: CGPoint, in p: Presenter) -> Bool {
        DragSources.keepsContact(node, from: node.hitTest(point, with: nil), in: p)
    }

    /// One enabled interaction on a node that can lift: draggable, not
    /// disabled, and carrying something (D4, D9).
    func testADraggableNodeHasOneEnabledInteractionWhileItDrags() throws {
        if ExactEnv.agentPaints { throw XCTSkip("none while the agent paints (D4)") }
        let p = page(["draggable": "true", "dataTransferText": "Mount Fuji"])
        let view = try XCTUnwrap(p.views[10])
        XCTAssertEqual(drags(view).count, 1)
        XCTAssertEqual(drags(view).first?.isEnabled, true, "on every device, whatever enabledByDefault says")
        XCTAssertTrue(try drags(view).first?.delegate === sources(p))
        XCTAssertTrue(drags(try XCTUnwrap(p.views[11])).isEmpty, "its text drags with it, not on its own")
        if #available(iOS 27.0, *) { XCTAssertEqual(drags(view).first?.allowsPointerDragBeforeLiftDelay, true) }
        let added = try (view.gestureRecognizers ?? []).filter { try sources(p).owns($0, on: view) }.map(ObjectIdentifier.init)
        p.apply(wireBatch([["op": "props", "id": 10, "set": ["draggable": "false"]]]))
        XCTAssertTrue(drags(view).isEmpty, "draggable=false takes it off")
        XCTAssertTrue(view.interactions.isEmpty, "with the interactions UIKit added beside it")
        XCTAssertTrue((view.gestureRecognizers ?? []).allSatisfy { !added.contains(ObjectIdentifier($0)) }, "and its recognizers")
        p.apply(wireBatch([["op": "props", "id": 10, "set": ["draggable": "true"]]]))
        XCTAssertEqual(drags(view).count, 1, "and true puts one back")
        p.apply(wireBatch([["op": "props", "id": 10, "set": ["disabled": "true"]]]))
        XCTAssertTrue(drags(view).isEmpty, "a disabled node lifts nothing")
        p.apply(wireBatch([["op": "props", "id": 10, "set": ["disabled": "false", "dataTransferText": ""]]]))
        XCTAssertTrue(drags(view).isEmpty, "nor does one that carries nothing")
    }

    func testBesideASwipeAPointerWaitsForTheLift() throws {
        if ExactEnv.agentPaints { throw XCTSkip("none while the agent paints (D4)") }
        guard #available(iOS 27.0, *) else { throw XCTSkip("iOS 27's allowsPointerDragBeforeLiftDelay") }
        let p = presenter([["op": "create", "id": 10, "kind": "view", "handlers": ["swipeleft"], "props": ["draggable": "true", "dataTransferText": "a"]],
                           ["op": "roots", "ids": [10]], ["op": "frame", "id": 10, "x": 0.0, "y": 0.0, "w": 200.0, "h": 100.0]])
        XCTAssertEqual(drags(try XCTUnwrap(p.views[10])).first?.allowsPointerDragBeforeLiftDelay, false)
    }

    /// UIKit offers an element only the drags on it or in its subtree, and
    /// the card's is on the card, which VoiceOver does not visit: its text is
    /// given the card's, named as the card is (D9). The card itself keeps
    /// UIKit's own.
    func testAnElementInsideTheNodeIsGivenItsDrag() throws {
        if ExactEnv.agentPaints { throw XCTSkip("none while the agent paints (D4)") }
        let p = page(["draggable": "true", "dataTransferText": "Mount Fuji"])
        let card = try XCTUnwrap(p.views[10]), text = try XCTUnwrap(p.views[11])
        XCTAssertNil(card.accessibilityDragSourceDescriptors, "UIKit's own")
        let drag = try XCTUnwrap(text.accessibilityDragSourceDescriptors?.first)
        XCTAssertTrue(drag.view === card)
        XCTAssertEqual(drag.name, card.accessibleName)
        XCTAssertFalse(drag.name.isEmpty)
        p.apply(wireBatch([["op": "props", "id": 10, "set": ["disabled": "true"]]]))
        XCTAssertNil(text.accessibilityDragSourceDescriptors, "a disabled card lifts nothing, so gives none")
    }

    /// A checkbox, a progress bar and a select are UIKit-drawn elements of their own,
    /// which a rested finger on lifts the card around them (D8): each is
    /// given the card's drag, read when asked.
    func testACheckboxAProgressBarAndASelectAreGivenTheCardsDrag() throws {
        if ExactEnv.agentPaints { throw XCTSkip("none while the agent paints (D4)") }
        let p = page(["draggable": "true", "dataTransferText": "Mount Fuji"])
        p.apply(wireBatch([["op": "create", "id": 20, "kind": "control", "props": ["type": "checkbox", "accessibilityLabel": "Seen"]],
                           ["op": "create", "id": 21, "kind": "control", "props": ["type": "progress", "accessibilityRole": "progressbar", "value": "0.5"], "handlers": []],
                           ["op": "create", "id": 22, "kind": "control", "props": ["type": "select"], "handlers": ["input", "change"]],
                           ["op": "children", "id": 10, "ids": [11, 20, 21, 22]],
                           ["op": "frame", "id": 22, "x": 120.0, "y": 40.0, "w": 72.0, "h": 24.0],
                           ["op": "frame", "id": 20, "x": 8.0, "y": 40.0, "w": 24.0, "h": 24.0],
                           ["op": "frame", "id": 21, "x": 8.0, "y": 80.0, "w": 160.0, "h": 16.0]]))
        let card = try XCTUnwrap(p.views[10])
        let checkbox = try XCTUnwrap(p.controls.controls[20])
        XCTAssertTrue(checkbox.accessibilityDragSourceDescriptors?.first?.view === card, "the checkbox")
        let bar = try XCTUnwrap(p.controls.bars[21])
        XCTAssertTrue(bar.accessibilityDragSourceDescriptors?.first?.view === card, "the bar")
        XCTAssertTrue(try XCTUnwrap(p.controls.controls[22]).accessibilityDragSourceDescriptors?.first?.view === card, "the select")
        p.apply(wireBatch([["op": "props", "id": 10, "set": ["draggable": "false"]]]))
        XCTAssertNil(bar.accessibilityDragSourceDescriptors, "none once the card does not drag")
    }

    /// The lift reads what the touch lands on now, as the window routes it:
    /// the card's text lifts the card; a view put over it since the touch
    /// began, or a point off the card, lifts nothing.
    func testTheLiftReadsWhatTheTouchLandsOnNow() throws {
        if ExactEnv.agentPaints { throw XCTSkip("none while the agent paints (D4)") }
        let p = page(["draggable": "true", "dataTransferText": "Mount Fuji"])
        let card = try XCTUnwrap(p.views[10]), text = try XCTUnwrap(p.views[11])
        let interaction = try XCTUnwrap(drags(card).first)
        let onText = text.convert(CGPoint(x: text.bounds.midX, y: text.bounds.midY), to: window)
        XCTAssertEqual(try sources(p).dragInteraction(interaction, itemsForBeginning: PointDragSession(at: onText, in: window)).count, 1)
        let cover = UIView(frame: CGRect(x: onText.x - 5, y: onText.y - 5, width: 10, height: 10))
        window.addSubview(cover)
        XCTAssertTrue(try sources(p).dragInteraction(interaction, itemsForBeginning: PointDragSession(at: onText, in: window)).isEmpty, "covered")
        cover.removeFromSuperview()
        XCTAssertTrue(try sources(p).dragInteraction(interaction, itemsForBeginning: PointDragSession(at: CGPoint(x: 390, y: 700), in: window)).isEmpty, "off the card")
    }

    /// A view holding more than one drag offers each, in node order, named by
    /// its node's accessible name, else by what it carries, which UIKit's
    /// own lack; holding one, it keeps UIKit's own.
    func testAViewOffersEachDragInsideItByName() throws {
        if ExactEnv.agentPaints { throw XCTSkip("none while the agent paints (D4)") }
        let p = page(["accessibilityLabel": "Attachments"])
        p.apply(wireBatch([["op": "create", "id": 21, "kind": "view", "props": ["draggable": "true", "dataTransferFiles": "assets/b.pdf"]],
                           ["op": "create", "id": 20, "kind": "view", "props": ["draggable": "true", "dataTransferText": "Report", "accessibilityLabel": "Export"]],
                           ["op": "children", "id": 10, "ids": [11, 21, 20]],
                           ["op": "frame", "id": 20, "x": 8.0, "y": 40.0, "w": 36.0, "h": 36.0], ["op": "frame", "id": 21, "x": 48.0, "y": 40.0, "w": 36.0, "h": 36.0]]))
        let wrapper = try XCTUnwrap(p.views[10])
        let offered = { wrapper.accessibilityDragSourceDescriptors.map { $0.map { ($0.view as? NodeView)?.id } } }
        XCTAssertEqual(offered(), [20, 21], "in node order")
        XCTAssertEqual(wrapper.accessibilityDragSourceDescriptors?.map(\.name), ["Export", "b.pdf"])
        p.apply(wireBatch([["op": "props", "id": 21, "set": ["inert": "true"]]]))
        let inert = try XCTUnwrap(drags(try XCTUnwrap(p.views[21])).first, "kept: a modal makes a node inert and takes that back with no batch for it")
        let at = try XCTUnwrap(p.views[21]).convert(CGPoint(x: 18, y: 18), to: window)
        XCTAssertTrue(try sources(p).dragInteraction(inert, itemsForBeginning: PointDragSession(at: at, in: window)).isEmpty, "an inert node lifts nothing")
        XCTAssertEqual(offered(), [20], "the one that lifts, not UIKit's own, which would find the inert one's")
        p.apply(wireBatch([["op": "props", "id": 20, "set": ["draggable": "false"]]]))
        XCTAssertEqual(offered(), [], "none that lifts: explicitly none")
        p.apply(wireBatch([["op": "props", "id": 20, "set": ["draggable": "true"]], ["op": "props", "id": 21, "set": ["draggable": "false"]]]))
        XCTAssertNil(wrapper.accessibilityDragSourceDescriptors, "one left, which lifts: UIKit's own")
    }

    /// A custom invoker's button, which VoiceOver visits over the invoker, is
    /// given the drag of the card that holds the invoker.
    func testACustomInvokersButtonIsGivenTheCardsDrag() throws {
        if ExactEnv.agentPaints { throw XCTSkip("none while the agent paints (D4)") }
        let p = page(["draggable": "true", "dataTransferText": "Mount Fuji", "accessibilityLabel": "Fuji"])
        p.apply(wireBatch([["op": "create", "id": 20, "kind": "button", "props": ["popovertarget": "confirm"]],
                           ["op": "create", "id": 21, "kind": "view", "props": ["id": "confirm", "popover": "auto", "accessibilityRole": "alertdialog"]],
                           ["op": "children", "id": 10, "ids": [11, 20]], ["op": "children", "id": 1, "ids": [10, 21]],
                           ["op": "frame", "id": 20, "x": 8.0, "y": 60.0, "w": 120.0, "h": 44.0],
                           ["op": "frame", "id": 21, "x": 20.0, "y": 300.0, "w": 200.0, "h": 100.0]]))
        p.viewport.layoutIfNeeded()
        let overlay = try XCTUnwrap(p.views[20]?.subviews.compactMap { $0 as? InvokerButton }.first, "the invoker has its button")
        let drag = try XCTUnwrap(overlay.accessibilityDragSourceDescriptors?.first)
        XCTAssertTrue(drag.view === p.views[10])
        XCTAssertEqual(drag.name, "Fuji")
    }

    /// Selectable text hands a touch to its selection surface, whose
    /// selection then has the contact, as a text input's does (D8): a rested
    /// finger there lifts no card.
    func testSelectableTextKeepsTheContact() throws {
        if ExactEnv.agentPaints { throw XCTSkip("none while the agent paints (D4)") }
        let p = page(["draggable": "true", "dataTransferText": "Mount Fuji"])
        p.apply(wireBatch([["op": "create", "id": 20, "kind": "view", "style": ["user_select": "text"]],
                           ["op": "create", "id": 21, "kind": "text", "props": ["text": "First paragraph"], "style": ["font_size": 17.0]],
                           ["op": "create", "id": 22, "kind": "text", "props": ["text": "Second paragraph"], "style": ["font_size": 17.0]],
                           ["op": "children", "id": 20, "ids": [21, 22]], ["op": "children", "id": 10, "ids": [11, 20]],
                           ["op": "frame", "id": 20, "x": 8.0, "y": 40.0, "w": 180.0, "h": 60.0],
                           ["op": "frame", "id": 21, "x": 0.0, "y": 0.0, "w": 180.0, "h": 22.0],
                           ["op": "frame", "id": 22, "x": 0.0, "y": 30.0, "w": 180.0, "h": 22.0]]))
        let card = try XCTUnwrap(p.views[10]), first = try XCTUnwrap(p.views[21]), second = try XCTUnwrap(p.views[22])
        // Before any touch has installed the surface, as at the first one
        XCTAssertTrue(first.touchSelectsText && second.touchSelectsText, "the container's paragraphs would share a surface")
        XCTAssertTrue(keepsContact(card, at: first.convert(CGPoint(x: first.bounds.midX, y: first.bounds.midY), to: card), in: p))
        p.textSelection.touched(first)
        XCTAssertTrue(second.touchSelectsText, "and once it is installed")
        XCTAssertFalse(keepsContact(card, at: CGPoint(x: 150, y: 110), in: p), "the card's own face still lifts")
    }

    /// A pressable inside a card holds a small draggable in its corner: the
    /// pressable offers the card's drag around it beside the corner's own,
    /// and the corner keeps UIKit's own.
    func testAnElementOffersTheDragAroundItBesideItsOwn() throws {
        if ExactEnv.agentPaints { throw XCTSkip("none while the agent paints (D4)") }
        let p = page(["draggable": "true", "dataTransferText": "Mount Fuji"])
        p.apply(wireBatch([["op": "create", "id": 20, "kind": "view", "handlers": ["press"], "props": ["accessibilityLabel": "Open"]],
                           ["op": "create", "id": 21, "kind": "view", "props": ["draggable": "true", "dataTransferText": "Pin"]],
                           ["op": "children", "id": 20, "ids": [21]], ["op": "children", "id": 10, "ids": [11, 20]],
                           ["op": "frame", "id": 20, "x": 8.0, "y": 40.0, "w": 120.0, "h": 60.0],
                           ["op": "frame", "id": 21, "x": 0.0, "y": 0.0, "w": 16.0, "h": 16.0]]))
        let card = try XCTUnwrap(p.views[10]), pressable = try XCTUnwrap(p.views[20]), pin = try XCTUnwrap(p.views[21])
        let drags = try XCTUnwrap(pressable.accessibilityDragSourceDescriptors)
        XCTAssertEqual(drags.count, 2)
        XCTAssertTrue(drags.first?.view === card && drags.last?.view === pin, "the card's, then the corner's")
        XCTAssertNil(pin.accessibilityDragSourceDescriptors, "the corner keeps UIKit's own")
    }

    /// A paragraph with a link is no element itself: its words and its link
    /// are elements UIKit makes no view for, and each is given the card's drag.
    func testAParagraphsWordsAndLinksAreGivenItsDrag() throws {
        if ExactEnv.agentPaints { throw XCTSkip("none while the agent paints (D4)") }
        // A session's presenter, which shapes a paragraph's text
        let session = ExactApp.shared.makeSession(label: "drag-out-paragraph")
        defer { session.destroy() }
        let p = session.presenter
        p.viewport.frame = CGRect(x: 0, y: 0, width: 400, height: 800)
        p.apply(wireBatch(card(10, ["draggable": "true", "dataTransferText": "Mount Fuji"]) + [
            ["op": "create", "id": 13, "kind": "text", "style": ["font_size": 18]], ["op": "children", "id": 10, "ids": [11, 13]],
            ["op": "roots", "ids": [10]], ["op": "frame", "id": 13, "x": 8.0, "y": 40.0, "w": 180.0, "h": 40.0]]))
        p.apply(Batch.decode(Data(#"{"ops":[{"op":"paragraph","id":13,"runs":[{"id":14,"parent":13,"paint":true,"props":{"text":"Read more about the mountain","href":"/fuji"},"style":{"font_size":18}}]}]}"#.utf8)))
        let card = try XCTUnwrap(p.views[10]), text = try XCTUnwrap(p.views[13])
        let children = try XCTUnwrap(text.textAccessibilityChildren()).compactMap { $0 as? NSObject }
        XCTAssertEqual(children.count, 2, "its words and its link")
        let drags = children.compactMap { $0.accessibilityDragSourceDescriptors?.first }
        XCTAssertEqual(drags.count, 2)
        XCTAssertTrue(drags.allSatisfy { $0.view === card })
        p.apply(wireBatch([["op": "props", "id": 10, "set": ["disabled": "true"]]]))
        XCTAssertEqual(children.map { $0.accessibilityDragSourceDescriptors == nil }, [true, true], "read when asked: a disabled card gives none")
    }

    /// A native button is its node's element, a subview the node's
    /// interaction is not on: it is given the node's drag.
    func testANativeButtonIsGivenItsNodesDrag() throws {
        if ExactEnv.agentPaints { throw XCTSkip("none while the agent paints (D4)") }
        let p = Presenter()
        p.buttonFace = { _ in ButtonFace() }
        window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 800))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        p.apply(wireBatch([["op": "create", "id": 10, "kind": "control", "handlers": ["press"], "style": ["appearance": "auto"],
                            "props": ["type": "button", "accessibilityLabel": "Share", "draggable": "true", "dataTransferText": "a"]],
                           ["op": "roots", "ids": [10]], ["op": "frame", "id": 10, "x": 20.0, "y": 20.0, "w": 120.0, "h": 44.0]]))
        let node = try XCTUnwrap(p.views[10])
        let button = try XCTUnwrap(node.subviews.compactMap { $0 as? NativeButtonIOS }.first)
        XCTAssertTrue(button.isAccessibilityElement)
        let drag = try XCTUnwrap(button.accessibilityDragSourceDescriptors?.first)
        XCTAssertTrue(drag.view === node)
        XCTAssertEqual(drag.name, "Share")
    }

    func testTheItemsAreTheFilesThenTheURLAndTextOnTheFirst() throws {
        let p = page(["draggable": "true", "dataTransferFiles": "assets/apple/1016-thumb.jpg\napp:/data/notes.pdf",
                      "dataTransferUrl": "https://example.com/1016", "dataTransferText": "Mount Fuji"])
        let items = DragItems.items(of: try XCTUnwrap(p.views[10]), in: nil)
        XCTAssertEqual(items.count, 2, "one item a file")
        XCTAssertEqual(items.map { $0.itemProvider.suggestedName }, ["1016-thumb.jpg", "notes.pdf"])
        let first = items[0].itemProvider.registeredTypeIdentifiers
        XCTAssertEqual(first.first, UTType.jpeg.identifier, "the file first, its type its extension's")
        XCTAssertTrue(first.contains(UTType.url.identifier) && first.contains(UTType.utf8PlainText.identifier), "\(first)")
        XCTAssertLessThan(try XCTUnwrap(first.firstIndex(of: UTType.url.identifier)), try XCTUnwrap(first.firstIndex(of: UTType.utf8PlainText.identifier)))
        XCTAssertEqual(items[1].itemProvider.registeredTypeIdentifiers, [UTType.pdf.identifier])
        XCTAssertEqual((items[0].localObject as? DragItems.Lifted)?.id, 10)
    }

    func testWithNoFileOneItemCarriesTheURLAndText() throws {
        let p = page(["draggable": "true", "dataTransferUrl": "https://example.com/1016", "dataTransferText": ""])
        let items = DragItems.items(of: try XCTUnwrap(p.views[10]), in: nil)
        XCTAssertEqual(items.count, 1)
        XCTAssertTrue(items[0].itemProvider.hasItemConformingToTypeIdentifier(UTType.url.identifier))
        XCTAssertFalse(items[0].itemProvider.hasItemConformingToTypeIdentifier(UTType.plainText.identifier), "an empty text is none, as tree reads it")
    }

    /// A URL Foundation cannot read is left out of the drag, and the journal
    /// says so once, as the value arrives, without the URL, which may carry a
    /// token: on a card that carries only that URL, which lifts nothing, too.
    func testAURLFoundationCannotReadIsLeftOutAndJournaled() throws {
        if ExactEnv.agentPaints { throw XCTSkip("none while the agent paints (D4)") }
        XCTAssertNil(URL(string: "https://[broken"), "Foundation refuses it")
        let session = ExactApp.shared.makeSession(label: "drag-out-url")
        defer { session.destroy() }
        XCTAssertNil(session.boot(size: CGSize(width: 390, height: 844)).error)
        let p = session.presenter
        let journaled = { session.agent(#"{"op":"logs","since":0}"#).components(separatedBy: "drag: url not carried: Foundation cannot read it as a URL; percent-encode what a browser would").count - 1 }
        p.apply(wireBatch(card(9010, ["draggable": "true", "dataTransferUrl": "https://[broken"]) + [["op": "roots", "ids": [9010]]]))
        let card = try XCTUnwrap(p.views[9010])
        XCTAssertTrue(drags(card).isEmpty, "a card that carries only that lifts nothing")
        p.apply(wireBatch([["op": "props", "id": 9011, "set": ["text": "Card again"]]]))
        XCTAssertEqual(journaled(), 1, "once for the value, not at each batch")
        p.apply(wireBatch([["op": "props", "id": 9010, "set": ["dataTransferText": "Mount Fuji"]]]))
        let items = DragItems.items(of: card, in: session)
        XCTAssertEqual(items.count, 1, "the text still")
        XCTAssertFalse(items[0].itemProvider.hasItemConformingToTypeIdentifier(UTType.url.identifier))
        XCTAssertEqual(journaled(), 1, "the lift journals nothing more")
        XCTAssertFalse(session.agent(#"{"op":"logs","since":0}"#).contains("[broken"), "the URL itself stays out")
    }

    func testTheLiftIsTheNodeOverItsController() throws {
        let host = UIViewController()
        let p = page(["draggable": "true", "dataTransferText": "a"])
        host.view.frame = window.bounds
        window.rootViewController = host
        host.view.addSubview(p.viewport)
        let view = try XCTUnwrap(p.views[10])
        let preview = try XCTUnwrap(DragSources.preview(of: view))
        XCTAssertTrue(preview.view === view, "the node itself")
        XCTAssertTrue(preview.target.container === host.view, "over the controller's view, so a clipping strip does not cut it")
        XCTAssertEqual(preview.parameters.backgroundColor, .clear, "no fill of its own")
        view.layer.backgroundColor = UIColor.systemTeal.cgColor
        XCTAssertEqual(DragSources.preview(of: view)?.parameters.backgroundColor?.cgColor, UIColor.systemTeal.cgColor, "on its own fill, as the menu's preview")
        view.isHidden = true
        XCTAssertNil(DragSources.preview(of: view), "a hidden node lifts nothing")
    }

    func testAControlOfANodeInsideKeepsTheContact() throws {
        let p = presenter([["op": "create", "id": 10, "kind": "view", "props": ["draggable": "true", "dataTransferText": "a"]],
                           ["op": "create", "id": 12, "kind": "view", "props": ["testId": "inner"]], ["op": "children", "id": 10, "ids": [12]], ["op": "roots", "ids": [10]],
                           ["op": "frame", "id": 10, "x": 0.0, "y": 0.0, "w": 200.0, "h": 120.0], ["op": "frame", "id": 12, "x": 8.0, "y": 8.0, "w": 100.0, "h": 40.0]])
        let card = try XCTUnwrap(p.views[10])
        XCTAssertFalse(keepsContact(card, at: CGPoint(x: 150, y: 80), in: p), "the card's own face lifts")
        XCTAssertFalse(keepsContact(card, at: CGPoint(x: 20, y: 20), in: p), "and a box inside it")
        let inner = try XCTUnwrap(p.views[12])
        let field = UITextField(frame: inner.bounds)
        inner.addSubview(field)
        XCTAssertTrue(keepsContact(card, at: CGPoint(x: 20, y: 20), in: p), "a text input inside keeps its own drag")
        field.removeFromSuperview()
        let slider = UISlider(frame: inner.bounds)
        inner.addSubview(slider)
        XCTAssertTrue(keepsContact(card, at: CGPoint(x: 20, y: 20), in: p), "and a control that tracks a drag")
        slider.removeFromSuperview()
        let button = UIButton(frame: inner.bounds)
        inner.addSubview(button)
        XCTAssertFalse(keepsContact(card, at: CGPoint(x: 20, y: 20), in: p), "a child button is a press: a rested finger lifts the card (D8)")
        button.removeFromSuperview()
        let checkbox = UIControl(frame: inner.bounds)
        inner.addSubview(checkbox)
        XCTAssertFalse(keepsContact(card, at: CGPoint(x: 20, y: 20), in: p), "and so is any other control, a checkbox")
        checkbox.removeFromSuperview()
        let face = UIButton(frame: card.bounds)
        card.insertSubview(face, at: 0)
        XCTAssertFalse(keepsContact(card, at: CGPoint(x: 150, y: 80), in: p), "the node's own control is the node")
        face.removeFromSuperview()
        p.apply(wireBatch([["op": "props", "id": 12, "set": ["disabled": "true"]]]))
        XCTAssertTrue(keepsContact(card, at: CGPoint(x: 20, y: 20), in: p), "a disabled node inside keeps it, as on the Mac and Android")
        XCTAssertFalse(keepsContact(card, at: CGPoint(x: 150, y: 80), in: p), "and only where it is")
        p.apply(wireBatch([["op": "props", "id": 12, "set": ["disabled": "false", "draggable": "true"]]]))
        XCTAssertFalse(keepsContact(card, at: CGPoint(x: 20, y: 20), in: p), "a draggable node inside that carries nothing lifts the card")
        p.apply(wireBatch([["op": "props", "id": 12, "set": ["dataTransferText": "b"]]]))
        XCTAssertTrue(keepsContact(card, at: CGPoint(x: 20, y: 20), in: p), "one that carries something lifts itself")
        p.apply(wireBatch([["op": "props", "id": 10, "set": ["disabled": "true"]]]))
        XCTAssertTrue(keepsContact(card, at: CGPoint(x: 150, y: 80), in: p), "a disabled node does not lift")
    }

    /// A delivered update's resolver removes its files when released: the
    /// load handler holds it, so its file is there at the drop however long
    /// after the lift, and goes with the handler.
    func testADeliveredFileIsThereAtTheDropAndGoesWithTheHandler() throws {
        let bytes = Data("jpeg".utf8)
        var resolver: AssetResolver? = try AssetResolver(root: URL(fileURLWithPath: NSTemporaryDirectory()), verified: ["assets/a.jpg": bytes])
        var load: DragItems.Load? = DragItems.shipped("assets/a.jpg", resolver, DragFiles.Journal())
        weak let held = resolver
        resolver = nil
        XCTAssertNotNil(held, "the handler holds the resolver")
        var file: URL?
        _ = load?({ url, coordinated, error in file = url; XCTAssertFalse(coordinated); XCTAssertNil(error) })
        let url = try XCTUnwrap(file)
        XCTAssertEqual(try Data(contentsOf: url), bytes)
        load = nil
        XCTAssertNil(held)
        XCTAssertFalse(FileManager.default.fileExists(atPath: url.path), "its directory goes with the resolver")
    }

    func testAFileTheAppDoesNotShipCompletesWithTheError() throws {
        let resolver = try AssetResolver(root: URL(fileURLWithPath: NSTemporaryDirectory()), verified: [:])
        let done = expectation(description: "the load completes")
        _ = DragItems.shipped("assets/gone.jpg", resolver, DragFiles.Journal())({ url, _, error in
            XCTAssertNil(url)
            XCTAssertEqual((error as NSError?)?.localizedDescription, "drag: assets/gone.jpg could not be read: the app does not ship it")
            done.fulfill()
        })
        wait(for: [done], timeout: 5)
    }

    /// An `app:/` file is opened by the session's runtime when a destination
    /// asks: one it refuses (this session is not booted) completes with the
    /// runtime's reason, journaled, and makes no directory.
    func testAnAppFileTheRuntimeDoesNotOpenCompletesWithItsReasonAndMakesNothing() throws {
        let session = ExactApp.shared.makeSession(label: "drag-out-app-file")
        defer { session.destroy() }
        let made = { Set((try? FileManager.default.contentsOfDirectory(atPath: NSTemporaryDirectory())) ?? []).filter { $0.hasPrefix("exact-drag-") } }
        let before = made()
        let p = page(["draggable": "true", "dataTransferFiles": "app:/data/notes.pdf"])
        let item = try XCTUnwrap(DragItems.items(of: try XCTUnwrap(p.views[10]), in: session).first)
        let done = expectation(description: "the load completes")
        var failure: String?
        _ = item.itemProvider.loadFileRepresentation(forTypeIdentifier: UTType.pdf.identifier) { url, error in
            XCTAssertNil(url)
            // The provider's own error wraps the handler's.
            let error = error as NSError?
            failure = (error?.userInfo[NSUnderlyingErrorKey] as? NSError ?? error)?.localizedDescription
            done.fulfill()
        }
        wait(for: [done], timeout: 15)
        XCTAssertEqual(failure, "drag: app:/data/notes.pdf could not be read: not booted")
        XCTAssertEqual(made(), before, "no directory, while the provider is still held")
        withExtendedLifetime(item) {}
    }

    /// An `app:/` file is opened when a destination asks (here by a stand-in,
    /// of a file made after the lift) and copied off the main thread; its
    /// load answers the whole file with the descriptor already closed. A
    /// copy that fails leaves no folder behind.
    func testAnAppFileIsOpenedAndCopiedWhenTheDestinationAsks() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("drag-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        let scratch = DragItems.Scratch()
        var fd: Int32 = -1, offMain = true
        let load = { (flags: Int32) in
            DragItems.appFile("app:/data/notes.txt", scratch, DragFiles.Journal()) { _, _ in
                offMain = offMain && !Thread.isMainThread
                fd = open(folder.appendingPathComponent("notes.txt").path, flags)
                return fd >= 0 ? .success(fd) : .failure(DragFiles.posix())
            }
        }
        let reads = load(O_RDONLY), fails = load(O_WRONLY)
        try Data("the backup".utf8).write(to: folder.appendingPathComponent("notes.txt"))
        let done = expectation(description: "the file loads")
        var read: Data?, closed = false
        _ = reads({ url, _, error in
            XCTAssertNil(error)
            closed = fcntl(fd, F_GETFD) == -1 && errno == EBADF
            read = url.flatMap { try? Data(contentsOf: $0) }
            done.fulfill()
        })
        wait(for: [done], timeout: 5)
        XCTAssertEqual(read, Data("the backup".utf8))
        XCTAssertTrue(closed, "the descriptor closed before the answer")
        XCTAssertTrue(offMain, "the copy runs off the main thread")
        // A descriptor that cannot be read fails the copy
        let failed = expectation(description: "the load fails")
        var closedOnFailure = false
        _ = fails({ url, _, error in
            XCTAssertNil(url)
            XCTAssertNotNil(error)
            closedOnFailure = fcntl(fd, F_GETFD) == -1 && errno == EBADF
            failed.fulfill()
        })
        wait(for: [failed], timeout: 5)
        XCTAssertTrue(closedOnFailure, "the descriptor closed before the failure is answered")
        let left = (try? FileManager.default.contentsOfDirectory(atPath: scratch.directory.path)) ?? []
        XCTAssertEqual(left.count, 1, "the first load's folder alone: the failed copy's went with it")
    }

    /// A lift opens nothing: a node's `app:/` file is opened through its
    /// item's provider when a destination asks, here a file made after the
    /// lift.
    func testALiftOpensNothing() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("drag-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }
        let p = page(["draggable": "true", "dataTransferFiles": "app:/data/notes.txt"])
        var opens = 0
        let items = DragItems.items(of: try XCTUnwrap(p.views[10]), in: nil) { _, _ in
            opens += 1
            let fd = open(folder.appendingPathComponent("notes.txt").path, O_RDONLY)
            return fd >= 0 ? .success(fd) : .failure(DragFiles.posix())
        }
        XCTAssertEqual(opens, 0, "none at the lift")
        try Data("the backup".utf8).write(to: folder.appendingPathComponent("notes.txt"))
        let done = expectation(description: "the file loads")
        var read: Data?
        _ = try XCTUnwrap(items.first).itemProvider.loadFileRepresentation(forTypeIdentifier: UTType.plainText.identifier) { url, _ in
            read = url.flatMap { try? Data(contentsOf: $0) }
            done.fulfill()
        }
        wait(for: [done], timeout: 5)
        XCTAssertEqual(read, Data("the backup".utf8))
        XCTAssertEqual(opens, 1)
    }

    /// A destination that asks for an `app:/` file after its session was
    /// destroyed, though still held, is told it failed: the runtime that
    /// would open it has gone.
    func testAnAppFileAskedForAfterItsSessionEndsFails() throws {
        let session = ExactApp.shared.makeSession(label: "drag-out-ended")
        let p = page(["draggable": "true", "dataTransferFiles": "app:/data/notes.txt"])
        let item = try XCTUnwrap(DragItems.items(of: try XCTUnwrap(p.views[10]), in: session).first)
        session.destroy()
        let done = expectation(description: "the load completes")
        var failure: String?
        _ = item.itemProvider.loadFileRepresentation(forTypeIdentifier: UTType.plainText.identifier) { url, error in
            XCTAssertNil(url)
            let error = error as NSError?
            failure = (error?.userInfo[NSUnderlyingErrorKey] as? NSError ?? error)?.localizedDescription
            done.fulfill()
        }
        wait(for: [done], timeout: 15)
        XCTAssertEqual(failure, "drag: app:/data/notes.txt could not be read: the app's session has ended")
        withExtendedLifetime(session) {}
    }

    // MARK: The node pool

    private func collections(_ rows: [Int]) -> [String: Any] {
        ["op": "collections", "items": [["view": 1, "revision": 1, "scrollSequence": 0, "count": 50, "totalExtent": 2000,
            "rows": rows.map { ["view": $0, "root": $0, "epoch": 1] }, "correction": NSNull()]]]
    }
    private func row(_ base: Int, draggable: Bool) -> [[String: Any]] {
        card(base, draggable ? ["testId": "row-\(base)", "draggable": "true", "dataTransferText": "Row \(base)"] : ["testId": "row-\(base)"])
    }
    private func list() -> Presenter {
        presenter([collections([10]), ["op": "create", "id": 1, "kind": "list", "style": ["overflow_y": "scroll"]], ["op": "create", "id": 9, "kind": "view"]]
            + row(10, draggable: true)
            + [["op": "children", "id": 1, "ids": [10]], ["op": "children", "id": 9, "ids": [1]], ["op": "roots", "ids": [9]],
               ["op": "frame", "id": 9, "x": 0.0, "y": 0.0, "w": 400.0, "h": 800.0], ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 300.0, "h": 600.0],
               ["op": "content", "id": 1, "w": 300.0, "h": 2000.0]])
    }
    private func next(_ p: Presenter, _ from: Int, _ to: Int, draggable: Bool) {
        p.apply(wireBatch([collections([to]), ["op": "destroy", "id": from], ["op": "destroy", "id": from + 1]] + row(to, draggable: draggable)
            + [["op": "children", "id": 1, "ids": [to]]]))
    }

    /// The lift's hit test runs views' own code: a row view renewed there for
    /// another incarnation, as the node pool renews one it lends, keeping
    /// the id, the view and the interaction, lifts nothing.
    func testARowRenewedDuringTheLiftsHitTestLiftsNothing() throws {
        if ExactEnv.agentPaints { throw XCTSkip("none while the agent paints (D4)") }
        let p = list()
        let view = try XCTUnwrap(p.views[10]), interaction = try XCTUnwrap(drags(view).first)
        let renewing = Relending(frame: view.bounds)
        view.addSubview(renewing)
        let at = view.convert(CGPoint(x: view.bounds.midX, y: view.bounds.midY), to: window)
        XCTAssertEqual(try sources(p).dragInteraction(interaction, itemsForBeginning: PointDragSession(at: at, in: window)).count, 1, "the point lifts the row")
        let incarnation = view.incarnation
        renewing.onHit = { [weak view] in view?.incarnation = NodePool.issue() }
        XCTAssertTrue(try sources(p).dragInteraction(interaction, itemsForBeginning: PointDragSession(at: at, in: window)).isEmpty)
        XCTAssertNotEqual(view.incarnation, incarnation, "the hit test renewed it")
        XCTAssertTrue(p.views[10] === view && drags(view).first === interaction, "the same id, view and interaction")
    }

    /// The lift's hit test runs views' own code: one that takes the drag off
    /// the node and puts it back, with another payload, gives the node a new
    /// interaction, and the old one lifts nothing.
    func testAnInteractionReplacedDuringTheLiftsHitTestLiftsNothing() throws {
        if ExactEnv.agentPaints { throw XCTSkip("none while the agent paints (D4)") }
        let p = page(["draggable": "true", "dataTransferText": "Mount Fuji"])
        let card = try XCTUnwrap(p.views[10]), old = try XCTUnwrap(drags(card).first)
        let relending = Relending(frame: card.bounds)
        relending.onHit = { [weak p] in
            p?.apply(wireBatch([["op": "props", "id": 10, "set": ["draggable": "false"]]]))
            p?.apply(wireBatch([["op": "props", "id": 10, "set": ["draggable": "true", "dataTransferText": "Mount Hood"]]]))
        }
        card.addSubview(relending)
        let at = card.convert(CGPoint(x: card.bounds.midX, y: card.bounds.midY), to: window)
        XCTAssertTrue(try sources(p).dragInteraction(old, itemsForBeginning: PointDragSession(at: at, in: window)).isEmpty)
        XCTAssertFalse(drags(card).first === old, "a new interaction on the node")
        // The page taken out of its window there
        let q = page(["draggable": "true", "dataTransferText": "Mount Fuji"])
        let other = try XCTUnwrap(q.views[10]), interaction = try XCTUnwrap(drags(other).first)
        let detaching = Relending(frame: other.bounds)
        detaching.onHit = { [weak q] in q?.viewport.removeFromSuperview() }
        other.addSubview(detaching)
        let point = other.convert(CGPoint(x: other.bounds.midX, y: other.bounds.midY), to: window)
        XCTAssertTrue(try sources(q).dragInteraction(interaction, itemsForBeginning: PointDragSession(at: point, in: window)).isEmpty, "out of its window")
    }

    private final class Relending: UIView {
        var onHit: () -> Void = {}
        override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
            let run = onHit
            onHit = {}
            run()
            return super.hitTest(point, with: event)
        }
    }

    func testARowViewLentToADraggableRowKeepsItsInteraction() throws {
        if ExactEnv.agentPaints { throw XCTSkip("none while the agent paints (D4)") }
        let p = list()
        let view = try XCTUnwrap(p.views[10])
        let kept = try XCTUnwrap(drags(view).first)
        next(p, 10, 20, draggable: true)
        XCTAssertTrue(p.views[20] === view, "the same view, lent to the next row")
        XCTAssertEqual(p.pool.takes, 1, "the interaction does not keep the row out of the pool")
        XCTAssertEqual(drags(view).count, 1)
        XCTAssertTrue(drags(view).first === kept, "the same interaction, now the new node's")
        XCTAssertEqual((DragItems.items(of: view, in: nil).first?.localObject as? DragItems.Lifted)?.id, 20)
        next(p, 20, 30, draggable: false)
        XCTAssertTrue(p.views[30] === view)
        XCTAssertTrue(drags(view).isEmpty, "lent to a row that does not drag, it comes off")
        XCTAssertTrue(view.interactions.isEmpty, "with what UIKit added beside it")
    }

    func testACancelledDragReturnsOnlyToItsOwnNode() throws {
        if ExactEnv.agentPaints { throw XCTSkip("none while the agent paints (D4)") }
        let p = list()
        let view = try XCTUnwrap(p.views[10])
        let interaction = try XCTUnwrap(drags(view).first)
        let item = try XCTUnwrap(DragItems.items(of: view, in: nil).first)
        let fallback = UITargetedDragPreview(view: view)
        XCTAssertNotNil(try sources(p).dragInteraction(interaction, previewForCancelling: item, withDefault: fallback))
        view.incarnation = NodePool.issue() // as the pool's rebind does, to the same id
        XCTAssertNil(try sources(p).dragInteraction(interaction, previewForCancelling: item, withDefault: fallback), "another incarnation: the item fades")
        next(p, 10, 20, draggable: true)
        XCTAssertNil(try sources(p).dragInteraction(interaction, previewForCancelling: item, withDefault: fallback), "the view is another row's now")
    }
}

/// A drag session at one point of `window`, which a test hands the lift.
final class PointDragSession: NSObject, UIDragSession {
    let at: CGPoint
    weak var window: UIView?
    init(at: CGPoint, in window: UIView) { self.at = at; self.window = window }
    var localContext: Any?
    var items: [UIDragItem] { [] }
    func location(in view: UIView) -> CGPoint { window.map { view.convert(at, from: $0) } ?? at }
    var allowsMoveOperation: Bool { false }
    var isRestrictedToDraggingApplication: Bool { false }
    func hasItemsConforming(toTypeIdentifiers typeIdentifiers: [String]) -> Bool { false }
    func canLoadObjects(ofClass aClass: any NSItemProviderReading.Type) -> Bool { false }
}
#endif
