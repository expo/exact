#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

/// LLP 1021 §5.1 on UIKit: a node naming a popover with `contextPopover` is
/// a UIContextMenuInteraction. The node lifts as its own targeted preview,
/// the popover's `contextPreview` row is the preview controller's view
/// (sized by its layout), its menu rows the UIMenu, and tapping the preview
/// presses that row: `.pop` when the press navigates, pushed without the
/// stack's animation, `.dismiss` otherwise. The native fixture's `peek` row.
/// UIKit, so a simulator runs it: bun host/apple/build.mjs --test --ios
final class ContextMenuIOSTests: XCTestCase {
    private var window: UIWindow?
    private var session: ExactSession?

    override func tearDown() {
        session?.destroy()
        window?.isHidden = true
        window = nil
        session = nil
        super.tearDown()
    }

    /// UIKit's commit animator, as the delegate sees it.
    private final class Commit: NSObject, UIContextMenuInteractionCommitAnimating {
        var preferredCommitStyle: UIContextMenuInteractionCommitStyle = .pop
        var previewViewController: UIViewController? { nil }
        var animated = 0
        func addAnimations(_ animations: @escaping () -> Void) { animated += 1; animations() }
        func addCompletion(_ completion: @escaping () -> Void) { completion() }
    }

    private final class LinkDelegate: ExactSessionDelegate {
        var opened: [String] = []

        func exactSession(_ session: ExactSession, command name: String, args: [Any]) {
            if name == "openURL", let href = args.first as? String { opened.append(href) }
        }
    }

    private func boot() throws -> ExactSession {
        if ExactEnv.agentPaints { throw XCTSkip("the agent paints the popover (D4)") }
        let plan = try Data(contentsOf: URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_FIXTURE_PLAN"], "build.mjs --test --ios compiles the fixture's plan")))
        let session = ExactApp.shared.makeSession(label: "context-menu")
        self.session = session
        let view = ExactView(session: session), host = UIViewController()
        let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        window.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        self.window = window
        XCTAssertNil(session.boot(plan: plan, size: window.bounds.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        waitForActivation(session)
        return session
    }
    private func node(_ session: ExactSession, _ testId: String) throws -> NodeView {
        try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == testId }, testId)
    }
    private func text(_ node: NodeView) -> String { node.paragraphSpec().runs.map(\.text).joined() }
    private func interaction(_ node: NodeView) -> UIContextMenuInteraction? {
        node.interactions.compactMap { $0 as? UIContextMenuInteraction }.first
    }
    /// The view of the controller that holds `node`: the host's preview target
    private func holder(of node: NodeView) throws -> UIView {
        try XCTUnwrap(sequence(first: try XCTUnwrap(node.superview) as UIResponder, next: { $0.next }).lazy.compactMap { ($0 as? UIViewController)?.viewIfLoaded }.first)
    }

    func testTheTargetCarriesTheAncestorsScale() throws {
        let session = try boot()
        let host = session.presenter.menus.context
        let peek = try node(session, "peek")
        let interaction = try XCTUnwrap(peek.interactions.compactMap { $0 as? UIContextMenuInteraction }.first)
        let configuration = try XCTUnwrap(host.contextMenuInteraction(interaction, configurationForMenuAtLocation: .zero))
        defer { host.contextMenuInteraction(interaction, willEndFor: configuration, animator: nil) }
        let parent = try XCTUnwrap(peek.superview)
        parent.transform = CGAffineTransform(scaleX: 0.5, y: 0.5)
        peek.transform = CGAffineTransform(scaleX: 0.96, y: 0.96)
        defer { parent.transform = .identity; peek.transform = .identity }
        let lifted = try XCTUnwrap(host.contextMenuInteraction(interaction, configuration: configuration, highlightPreviewForItemWithIdentifier: configuration.identifier))
        let over = try holder(of: peek)
        XCTAssertEqual(lifted.target.transform, CGAffineTransform(scaleX: 0.48, y: 0.48), "its own scale and its parent's")
        XCTAssertEqual(lifted.target.center, peek.convert(CGPoint(x: peek.bounds.midX, y: peek.bounds.midY), to: over), "from where it shows")
    }

    func testTheLiftsOutlineIsTheNodesOwn() throws {
        let session = try boot()
        let host = session.presenter.menus.context
        let peek = try node(session, "peek")
        let interaction = try XCTUnwrap(peek.interactions.compactMap { $0 as? UIContextMenuInteraction }.first)
        func outline() throws -> CGPath? {
            let configuration = try XCTUnwrap(host.contextMenuInteraction(interaction, configurationForMenuAtLocation: .zero))
            defer { host.contextMenuInteraction(interaction, willEndFor: configuration, animator: nil) }
            return try XCTUnwrap(host.contextMenuInteraction(interaction, configuration: configuration, highlightPreviewForItemWithIdentifier: configuration.identifier)).parameters.visiblePath?.cgPath
        }
        func corners(_ radii: [Double]) {
            session.presenter.apply(wireBatch([["op": "style", "id": Int(peek.id), "style": [
                "border_radius_top_left": radii[0], "border_radius_top_right": radii[1],
                "border_radius_bottom_right": radii[2], "border_radius_bottom_left": radii[3]]]]))
        }
        corners([12, 12, 4, 4])
        XCTAssertEqual(try outline(), peek.roundedPath(in: peek.bounds).cgPath, "unequal radii: its own outline")
        corners([0, 0, 0, 0])
        XCTAssertNil(try outline(), "no radius: the whole view, as UIKit shows it by default")
    }

    func testTheLiftsGroundIsTheNodesLayerFill() throws {
        let session = try boot()
        let host = session.presenter.menus.context
        let peek = try node(session, "peek")
        let interaction = try XCTUnwrap(peek.interactions.compactMap { $0 as? UIContextMenuInteraction }.first)
        func ground(_ style: [String: Any]) throws -> UIColor? {
            session.presenter.apply(wireBatch([["op": "style", "id": Int(peek.id), "style": style]]))
            peek.layer.displayIfNeeded()
            let configuration = try XCTUnwrap(host.contextMenuInteraction(interaction, configurationForMenuAtLocation: .zero))
            defer { host.contextMenuInteraction(interaction, willEndFor: configuration, animator: nil) }
            return try XCTUnwrap(host.contextMenuInteraction(interaction, configuration: configuration, highlightPreviewForItemWithIdentifier: configuration.identifier)).parameters.backgroundColor
        }
        let unfilled = try ground([:])
        var red: CGFloat = 1, green: CGFloat = 1
        try XCTUnwrap(ground(["background_color": [0, 122, 255, 255]])).getRed(&red, green: &green, blue: nil, alpha: nil)
        XCTAssertEqual(red, 0, accuracy: 0.01, "a filled bubble's ground is its fill, not UIKit's white")
        XCTAssertEqual(green, 122.0 / 255, accuracy: 0.01)
        // Extended (LLP 1123 D13), the bubble's fill is its extension picture's, and the ground still
        if #available(iOS 26, *) {
            peek.setExtension(UIEdgeInsets(top: 10, left: 0, bottom: 0, right: 0))
            defer { peek.setExtension(.zero) }
            var extendedGreen: CGFloat = 0
            try XCTUnwrap(ground(["background_color": [0, 122, 255, 255]])).getRed(nil, green: &extendedGreen, blue: nil, alpha: nil)
            XCTAssertNotNil(peek.boxExtension, "the bubble extends")
            XCTAssertEqual(extendedGreen, 122.0 / 255, accuracy: 0.01, "an extended bubble's ground is its fill too")
        }
        XCTAssertEqual(try ground(["background_color": [0, 122, 255, 255], "background_clip": "text"]), unfilled,
                       "a fill clipped to the text is drawn in the copy: UIKit's own ground")
    }

    /// UIKit hides the long-pressed node and grows its highlight in the
    /// preview's container, the route's mount, after the route and at depth 0.
    /// The positioned route ranks above 0, so the highlight takes the route's
    /// depth: it paints over the route, and the node never shows blank.
    func testTheHighlightUIKitAddsBesideTheRoutePaintsOverIt() throws {
        let session = try boot()
        let host = session.presenter.menus.context
        let peek = try node(session, "peek")
        let interaction = try XCTUnwrap(peek.interactions.compactMap { $0 as? UIContextMenuInteraction }.first)
        let configuration = try XCTUnwrap(host.contextMenuInteraction(interaction, configurationForMenuAtLocation: .zero))
        defer { host.contextMenuInteraction(interaction, willEndFor: configuration, animator: nil) }
        let lifted = try XCTUnwrap(host.contextMenuInteraction(interaction, configuration: configuration, highlightPreviewForItemWithIdentifier: configuration.identifier))
        let mount = try XCTUnwrap(lifted.target.container as? PresentationMount, "the route's mount")
        let route = try XCTUnwrap(mount.subviews.last as? NodeView)
        XCTAssertGreaterThan(route.paintZPosition, 0, "the positioned route ranks above the origin")
        let highlight = UIView(frame: peek.convert(peek.bounds, to: mount))
        mount.addSubview(highlight)
        defer { highlight.removeFromSuperview() }
        XCTAssertEqual(highlight.layer.zPosition, route.paintZPosition, accuracy: 1e-9)
        XCTAssertTrue(NodeView.hitOrder(mount.subviews).first === highlight)
    }

    func testTheRowLiftsIntoUIKitsMenuWithItsPreviewAndThePreviewPopsIntoItsScreen() throws {
        let session = try boot()
        let host = session.presenter.menus.context
        let peek = try node(session, "peek")
        let interaction = try XCTUnwrap(peek.interactions.compactMap { $0 as? UIContextMenuInteraction }.first, "the node's context menu is UIKit's")
        XCTAssertEqual(peek.contextRecognizer?.isEnabled, false, "its plain long press is off: the interaction recognizes it")
        XCTAssertEqual(text(try node(session, "peeks")), "peeks 0")

        let configuration = try XCTUnwrap(host.contextMenuInteraction(interaction, configurationForMenuAtLocation: CGPoint(x: 10, y: 10)))
        XCTAssertEqual(text(try node(session, "peeks")), "peeks 0", "nothing fires before the menu is shown")
        let lifted = try XCTUnwrap(host.contextMenuInteraction(interaction, configuration: configuration, highlightPreviewForItemWithIdentifier: configuration.identifier))
        XCTAssertTrue(lifted.view === peek, "the node itself, which UIKit hides and returns")
        XCTAssertEqual(lifted.parameters.visiblePath?.cgPath, UIBezierPath(roundedRect: peek.bounds, cornerRadius: 12).cgPath,
                       "its one radius as UIKit's rounded rectangle, which UIKit keeps through the lift")
        let over = try holder(of: peek)
        XCTAssertTrue(lifted.target.container === over && over !== peek.superview && lifted.target.center == peek.convert(CGPoint(x: peek.bounds.midX, y: peek.bounds.midY), to: over),
                      "from the node's place, over its controller's view, so no ancestor that clips cuts it")
        let owner = try XCTUnwrap(host.open)
        let controller = try XCTUnwrap(host.lift(owner))
        let menu = try XCTUnwrap(host.menu(owner))
        host.contextMenuInteraction(interaction, willDisplayMenuFor: configuration, animator: nil)
        XCTAssertEqual(peek.alpha, 1, "the host leaves the node's opacity alone while the menu shows")
        XCTAssertEqual(text(try node(session, "peeks")), "peeks 1", "its own contextmenu, once, before the rows or preview are read")

        let preview = try node(session, "peek-preview")
        XCTAssertTrue(preview.superview === controller.view, "the preview row is the controller's view's content")
        XCTAssertFalse(preview.isHidden)
        XCTAssertEqual(controller.preferredContentSize, CGSize(width: 300, height: 180), "sized by its layout: the popover's 300-point content box")
        XCTAssertEqual(preview.frame.origin, .zero)
        // Two sections (the hr), the preview not among the items.
        let titles = menu.children.flatMap { ($0 as? UIMenu)?.children ?? [$0] }.map { ($0 as? UIAction)?.title ?? "" }
        XCTAssertEqual(titles, ["Bump", "Clear"])
        XCTAssertEqual((menu.children.last as? UIMenu)?.children.first.flatMap { ($0 as? UIAction)?.attributes.contains(.destructive) }, true)
        XCTAssertEqual(session.presenter.menus.observation()?["kind"] as? String, "contextmenu")

        // Tapping the preview: its press pushes Detail, unanimated, and UIKit pops into it.
        let before = session.presenter.navigationHost.activeKey
        let commit = Commit()
        commit.preferredCommitStyle = .dismiss
        host.contextMenuInteraction(interaction, willPerformPreviewActionForMenuWith: configuration, animator: commit)
        XCTAssertEqual(commit.preferredCommitStyle, .pop, "a press that navigates commits with .pop")
        XCTAssertEqual(commit.animated, 1, "with an animation, without which UIKit shrinks the preview away")
        XCTAssertNotEqual(session.presenter.navigationHost.activeKey, before, "the press pushed")
        XCTAssertNotNil(try node(session, "route-detail"))
        XCTAssertNil(host.contextMenuInteraction(interaction, configuration: configuration, dismissalPreviewForItemWithIdentifier: configuration.identifier),
                     "after it navigated, the source is under the new screen: no return to it")
        host.contextMenuInteraction(interaction, willEndFor: configuration, animator: nil)
        XCTAssertEqual(peek.alpha, 1, "and after it ends")
        XCTAssertNil(host.open)
        let popover = try XCTUnwrap(session.presenter.views.values.first { $0.props["id"] == "peek" })
        XCTAssertTrue(popover.container.subviews.contains(preview), "the row is back in its popover")
        XCTAssertTrue(popover.isHidden, "which stays hidden")
    }

    /// The view's focus (key commands): a bar's touch sets it aside until the
    /// next touch on the page, a second bar touch keeps it held, and a
    /// context menu shown meanwhile returns it when it ends.
    func testABarsTouchSetsTheViewsFocusAsideUntilTheNextTouch() throws {
        let session = try boot()
        let view = try XCTUnwrap(session.view)
        let focus = session.presenter.menus.focus
        let shortcut = try node(session, "peek")
        session.presenter.apply(wireBatch([["op": "props", "id": shortcut.id, "set": ["accessibilityKeyShortcuts": "Meta+k"]]]))
        if !view.isFirstResponder { XCTAssertTrue(view.becomeFirstResponder()) }
        focus.setAside(untilTouch: true)
        XCTAssertFalse(view.isFirstResponder)
        focus.setAside(untilTouch: true)
        XCTAssertTrue(focus.untilNextTouch, "a second bar touch keeps what is held")
        focus.touched()
        XCTAssertTrue(view.isFirstResponder, "back at the next touch on the page")
        focus.setAside(untilTouch: true)
        focus.setAside()
        XCTAssertFalse(focus.untilNextTouch, "a menu with an end returns it")
        focus.restore()
        XCTAssertTrue(view.isFirstResponder)
        // A focused node is blurred by a bar's touch, and the view, which hears
        // key commands, takes the focus at the next touch on the page.
        let node = try node(session, "peek")
        XCTAssertTrue(node.becomeFirstResponder())
        focus.setAside(untilTouch: true)
        XCTAssertFalse(node.isFirstResponder)
        XCTAssertFalse(view.isFirstResponder, "not while the bar's menu may be up")
        XCTAssertTrue(focus.untilNextTouch)
        focus.touched()
        XCTAssertTrue(view.isFirstResponder)
        // A context menu over a focused node: neither the node nor the view
        // keeps the focus while it shows (a resigning node hands it to the
        // view), and the node has it back after.
        XCTAssertTrue(node.becomeFirstResponder())
        focus.setAside()
        XCTAssertFalse(node.isFirstResponder)
        XCTAssertFalse(view.isFirstResponder)
        focus.restore()
        XCTAssertTrue(node.isFirstResponder)
    }

    /// A text field or text area being edited keeps its focus, and its
    /// keyboard, through a bar's touch and a context menu alike: UIKit and
    /// the app decide when it goes, not the menu (MenuFocusIOS).
    func testAFieldBeingEditedKeepsItsFocusThroughABarsTouchAndAMenu() throws {
        let session = try boot()
        let focus = session.presenter.menus.focus
        // The first of the fixture's text fields that takes the focus.
        let field = try XCTUnwrap(session.presenter.views.values.compactMap(\.field).first { $0.window != nil && $0.becomeFirstResponder() },
                                  "the fixture has a text field")
        focus.setAside(untilTouch: true)
        XCTAssertTrue(field.isFirstResponder, "a bar's touch leaves the field alone")
        XCTAssertFalse(focus.untilNextTouch)
        focus.setAside()
        XCTAssertTrue(field.isFirstResponder, "and so does a context menu")
        focus.restore()
        XCTAssertTrue(field.isFirstResponder)
        _ = field.resignFirstResponder()
    }

    func testAMenuDismissedWithoutAChoiceReturnsItsRowAndAPreviewThatDoesNotNavigateDismisses() throws {
        let session = try boot()
        let host = session.presenter.menus.context
        let peek = try node(session, "peek")
        let interaction = try XCTUnwrap(peek.interactions.compactMap { $0 as? UIContextMenuInteraction }.first)
        let configuration = try XCTUnwrap(host.contextMenuInteraction(interaction, configurationForMenuAtLocation: .zero))
        let owner = try XCTUnwrap(host.open)
        _ = try XCTUnwrap(host.lift(owner))
        let preview = try node(session, "peek-preview")
        let returned = try XCTUnwrap(host.contextMenuInteraction(interaction, configuration: configuration, dismissalPreviewForItemWithIdentifier: configuration.identifier))
        let over = try holder(of: peek)
        XCTAssertTrue(returned.view === peek && returned.target.container === over && returned.target.center == peek.convert(CGPoint(x: peek.bounds.midX, y: peek.bounds.midY), to: over),
                      "dismissed, UIKit returns the node to its place")
        host.contextMenuInteraction(interaction, willEndFor: configuration, animator: nil)
        XCTAssertNil(host.open)
        let popover = try XCTUnwrap(session.presenter.views.values.first { $0.props["id"] == "peek" })
        XCTAssertTrue(popover.container.subviews.contains(preview))

        // A menu item presses its row.
        var pressed: [String] = []
        let press = session.presenter.onPress
        session.presenter.onPress = { [weak presenter = session.presenter] id in pressed.append(presenter?.views[id]?.props["testId"] ?? "?"); press?(id) }
        _ = try XCTUnwrap(host.contextMenuInteraction(interaction, configurationForMenuAtLocation: .zero))
        let menu = try XCTUnwrap(host.menu(try XCTUnwrap(host.open)))
        let bump = try XCTUnwrap(menu.children.flatMap { ($0 as? UIMenu)?.children ?? [$0] }.first as? UIAction)
        UIButton().sendAction(bump)
        XCTAssertEqual(pressed, ["peek-bump"])
        host.contextMenuInteraction(interaction, willEndFor: try XCTUnwrap(host.contextMenuInteraction(interaction, configurationForMenuAtLocation: .zero)), animator: nil)

        // A preview whose press stays on this screen dismisses.
        session.presenter.onPress = { _ in }
        let again = try XCTUnwrap(host.contextMenuInteraction(interaction, configurationForMenuAtLocation: .zero))
        _ = try XCTUnwrap(host.lift(try XCTUnwrap(host.open)))
        let commit = Commit()
        host.contextMenuInteraction(interaction, willPerformPreviewActionForMenuWith: again, animator: commit)
        XCTAssertEqual(commit.preferredCommitStyle, .dismiss)
        XCTAssertEqual(commit.animated, 0)
        host.contextMenuInteraction(interaction, willEndFor: again, animator: nil)
        session.presenter.onPress = press
    }

    /// A web link lifts with the host's own menu where no authored menu answers
    func testAWebLinkLiftsItselfWithTheLinksOwnMenu() throws {
        let session = try boot()
        XCTAssertNil(ExactEnv.appMetadata["ExactOrigin"], "the fixture has no declared origin")
        let host = session.presenter.menus.context
        func titles(_ menu: UIMenu) -> [String] { menu.children.map { ($0 as? UIAction)?.title ?? "" } }

        let link = try node(session, "plain-link")
        let linked = try XCTUnwrap(interaction(link), "a link gets UIKit's interaction with no menu authored")
        let configuration = try XCTUnwrap(host.contextMenuInteraction(linked, configurationForMenuAtLocation: CGPoint(x: link.bounds.midX, y: link.bounds.midY)))
        let lifted = try XCTUnwrap(host.contextMenuInteraction(linked, configuration: configuration, highlightPreviewForItemWithIdentifier: configuration.identifier))
        XCTAssertTrue(lifted.view === link, "the link itself lifts")
        let owner = try XCTUnwrap(host.open)
        XCTAssertNil(host.lift(owner), "no preview: UIKit shows the lifted link")
        let menu = try XCTUnwrap(host.menu(owner))
        XCTAssertEqual(menu.title, "https://example.com/fixture")
        XCTAssertEqual(titles(menu), ["Open Link", "Copy Link", "Share…"])
        host.contextMenuInteraction(linked, willEndFor: configuration, animator: nil)

        // A card's authored menu answers a long press on a link inside it.
        XCTAssertNil(interaction(try node(session, "card-link")))
        let card = try node(session, "peek-card"), inCard = try node(session, "card-link")
        let cardMenu = try XCTUnwrap(interaction(card))
        let authored = try XCTUnwrap(host.contextMenuInteraction(cardMenu, configurationForMenuAtLocation: inCard.convert(CGPoint(x: inCard.bounds.midX, y: inCard.bounds.midY), to: card)))
        XCTAssertEqual(host.open?.isLink, false)
        XCTAssertEqual(host.open?.name, "peek")
        host.contextMenuInteraction(cardMenu, willEndFor: authored, animator: nil)

        // The paragraph shares one interaction across its runs. Only the web link gets a menu.
        let paragraph = try node(session, "link-text")
        let sibling = try XCTUnwrap(paragraph.inlineText.first { $0.text == "Open " })
        let inAppLink = try XCTUnwrap(paragraph.inlineText.first { $0.props["testId"] == "run-link" })
        let suffix = try XCTUnwrap(paragraph.inlineText.last)
        let url = "https://example.com/detail"
        let webLink = InlineText(id: suffix.id, parent: paragraph.id, props: ["text": " Example ", "href": url],
                                 style: InlineStyle(), handlers: [], paints: true)
        var batch = wireBatch([["op": "paragraph", "id": paragraph.id, "runs": []]])
        batch.ops[0].runs = [sibling, webLink, inAppLink]
        session.presenter.apply(batch)

        let run = try XCTUnwrap(session.presenter.inlineText(webLink.id))
        let inAppRun = try XCTUnwrap(session.presenter.inlineText(inAppLink.id))
        let delegate = LinkDelegate()
        session.delegate = delegate
        waitFor("the paragraph's ink is drawn") { paragraph.textRasterReady && paragraph.textRasterLayer != nil }
        let box = try XCTUnwrap(paragraph.inlineRects(run).first)
        let siblingBox = try XCTUnwrap(paragraph.inlineRects(sibling).first)
        let inAppBox = try XCTUnwrap(paragraph.inlineRects(inAppRun).first)
        let inlineMenu = try XCTUnwrap(interaction(paragraph))
        let inAppPoint = CGPoint(x: inAppBox.midX, y: inAppBox.midY)
        XCTAssertEqual(paragraph.inlineActivationTarget(at: inAppPoint)?.id, inAppRun.id)
        XCTAssertNil(host.contextMenuInteraction(inlineMenu, configurationForMenuAtLocation: inAppPoint))
        XCTAssertNil(host.open)
        let at = try XCTUnwrap(host.contextMenuInteraction(inlineMenu, configurationForMenuAtLocation: CGPoint(x: box.midX, y: box.midY)))
        let runOwner = try XCTUnwrap(host.open)
        XCTAssertTrue(runOwner.isLink)
        XCTAssertEqual(runOwner.name, url)
        XCTAssertEqual(runOwner.inlineID, run.id)
        XCTAssertEqual(titles(try XCTUnwrap(host.menu(runOwner))), ["Open Link", "Copy Link", "Share…"])
        let ink = try XCTUnwrap(paragraph.textRasterLayer)
        ink.displayIfNeeded()
        func alpha(in line: CGRect) throws -> [UInt8] {
            let crop = paragraph.layer.convert(line, to: ink).insetBy(dx: 1, dy: 1).integral
            let width = Int(crop.width), height = Int(crop.height)
            let context = try XCTUnwrap(CGContext(data: nil, width: width, height: height, bitsPerComponent: 8,
                bytesPerRow: width * 4, space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
            context.translateBy(x: -crop.minX, y: -crop.minY)
            ink.render(in: context)
            let bytes = try XCTUnwrap(context.data).assumingMemoryBound(to: UInt8.self)
            return (0 ..< width * height).map { bytes[$0 * 4 + 3] }
        }
        let linkPixels = try alpha(in: box), siblingPixels = try alpha(in: siblingBox)
        XCTAssertTrue(linkPixels.contains { $0 > 0 }, "the link has visible ink before the lift")
        XCTAssertTrue(siblingPixels.contains { $0 > 0 }, "the sibling has visible ink before the lift")
        XCTAssertNil(ink.mask)
        _ = try XCTUnwrap(host.contextMenuInteraction(inlineMenu, configuration: at, highlightPreviewForItemWithIdentifier: at.identifier))
        XCTAssertTrue(try alpha(in: box).allSatisfy { $0 == 0 }, "the lift cuts the link's words out of the paragraph")
        XCTAssertEqual(try alpha(in: siblingBox), siblingPixels, "the sibling's ink stays visible")
        let commit = Commit()
        host.contextMenuInteraction(inlineMenu, willPerformPreviewActionForMenuWith: at, animator: commit)
        XCTAssertEqual(commit.preferredCommitStyle, .dismiss)
        XCTAssertEqual(delegate.opened, [url])
        host.contextMenuInteraction(inlineMenu, willEndFor: at, animator: nil)
        XCTAssertNil(ink.mask)
        XCTAssertEqual(try alpha(in: box), linkPixels, "the link's ink returns when the menu ends")
    }

    func testAnInAppLinkGetsNoInteractionWhenTheAppDeclaresNoOrigin() throws {
        let session = try boot()
        XCTAssertNil(ExactEnv.appMetadata["ExactOrigin"], "the fixture has no declared origin")
        let link = try node(session, "plain-link")
        XCTAssertNotNil(interaction(link))
        session.presenter.apply(wireBatch([["op": "props", "id": link.id, "set": ["href": "/detail"]]]))
        XCTAssertEqual(link.props["href"], "/detail")
        let point = CGPoint(x: link.bounds.midX, y: link.bounds.midY)
        XCTAssertNil(interaction(link))
        XCTAssertNotEqual(session.routerOp?["url"] as? String, "/detail")
        XCTAssertTrue(link.activate(at: link.convert(point, to: nil)) === link)
        XCTAssertEqual(session.routerOp?["url"] as? String, "/detail", "the standalone link navigates to Detail")
    }

    func testAnInAppLinkRunGetsNoInteractionWhenTheAppDeclaresNoOrigin() throws {
        let session = try boot()
        XCTAssertNil(ExactEnv.appMetadata["ExactOrigin"], "the fixture has no declared origin")
        let paragraph = try node(session, "link-text")
        let run = try XCTUnwrap(paragraph.inlineText.first { $0.props["testId"] == "run-link" },
                                "the fixture contains an inline link to Detail")
        let box = try XCTUnwrap(paragraph.inlineRects(run).first)
        let point = CGPoint(x: box.midX, y: box.midY)
        XCTAssertEqual(paragraph.inlineActivationTarget(at: point)?.id, run.id)
        XCTAssertNil(interaction(paragraph))
        XCTAssertNotEqual(session.routerOp?["url"] as? String, "/detail")
        XCTAssertTrue(paragraph.activateInline(run.id))
        XCTAssertEqual(session.routerOp?["url"] as? String, "/detail", "the inline link navigates to Detail")
    }

    func testAWebLinksMenuTakesTheURLABrowserWouldResolve() {
        let base = NodeView.linkMenuBase(origin: "https://example.com", visit: "/docs/guide?tab=2#top")
        let url = { (href: String) in NodeView.linkURL(href, relativeTo: base)?.absoluteString }
        XCTAssertEqual(url("/detail"), "https://example.com/detail")
        XCTAssertEqual(url("about"), "https://example.com/docs/about")
        XCTAssertEqual(url("#moves"), "https://example.com/docs/guide?tab=2#moves")
        XCTAssertEqual(url("?tab=3"), "https://example.com/docs/guide?tab=3")
        XCTAssertEqual(url("//cdn.example.org/x"), "https://cdn.example.org/x")
        XCTAssertEqual(url("https://other.example/a b"), "https://other.example/a%20b")
        XCTAssertNil(NodeView.linkURL("/detail", relativeTo: nil))
        XCTAssertNil(NodeView.linkURL("//cdn.example.org/x", relativeTo: nil))
        XCTAssertEqual(NodeView.linkURL("https://other.example/", relativeTo: nil)?.absoluteString, "https://other.example/")
        XCTAssertNil(url("https:/detail"), "an HTTPS URL needs a host")
        XCTAssertNil(url("https:detail"), "an HTTPS URL with no leading slash also needs a host")
        for href in ["mailto:a@example.com", "tel:+15555550100", "javascript:alert(1)", "JavaScript:void(0)", "myapp://open", "http://[", ""] {
            XCTAssertNil(url(href), href)
        }
        XCTAssertNotNil(NodeView.linkURL("HTTP://example.com", relativeTo: nil))
    }
}
#endif
