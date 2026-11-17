#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

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
        RunLoop.main.run(until: Date().addingTimeInterval(0.3))
        return session
    }
    private func node(_ session: ExactSession, _ testId: String) throws -> NodeView {
        try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == testId }, testId)
    }
    private func text(_ node: NodeView) -> String { node.paragraphSpec().runs.map(\.text).joined() }
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
        let before = session.presenter.navigation.activeKey
        let commit = Commit()
        commit.preferredCommitStyle = .dismiss
        host.contextMenuInteraction(interaction, willPerformPreviewActionForMenuWith: configuration, animator: commit)
        XCTAssertEqual(commit.preferredCommitStyle, .pop, "a press that navigates commits with .pop")
        XCTAssertEqual(commit.animated, 1, "with an animation, without which UIKit shrinks the preview away")
        XCTAssertNotEqual(session.presenter.navigation.activeKey, before, "the press pushed")
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

    /// A link no authored menu answers lifts with the host's own menu: the
    /// link itself on its platter, its URL over Open Link, Copy Link and Share.
    func testALinkNoMenuAnswersLiftsItselfWithTheLinksOwnMenu() throws {
        let session = try boot()
        let host = session.presenter.menus.context
        func titles(_ menu: UIMenu) -> [String] { menu.children.map { ($0 as? UIAction)?.title ?? "" } }
        func interaction(_ node: NodeView) -> UIContextMenuInteraction? { node.interactions.compactMap { $0 as? UIContextMenuInteraction }.first }

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

        // A run's link: its words leave the paragraph while their picture is
        // lifted, and tapping that picture follows the link.
        let paragraph = try node(session, "link-text")
        let run = try XCTUnwrap(paragraph.inlineText.first { $0.props["testId"] == "run-link" })
        let box = try XCTUnwrap(paragraph.inlineRects(run).first)
        let runs = try XCTUnwrap(interaction(paragraph))
        let at = try XCTUnwrap(host.contextMenuInteraction(runs, configurationForMenuAtLocation: CGPoint(x: box.midX, y: box.midY)))
        let runOwner = try XCTUnwrap(host.open)
        XCTAssertTrue(runOwner.isLink)
        XCTAssertEqual(runOwner.name, "/detail")
        XCTAssertEqual(runOwner.inlineID, run.id)
        XCTAssertEqual(titles(try XCTUnwrap(host.menu(runOwner))), ExactEnv.appMetadata["ExactOrigin"] == nil ? ["Open Link"] : ["Open Link", "Copy Link", "Share…"],
                       "a path in the app is copied and shared only on the app's declared origin")
        let ink = try XCTUnwrap(paragraph.textRasterLayer)
        XCTAssertNil(ink.mask)
        _ = try XCTUnwrap(host.contextMenuInteraction(runs, configuration: at, highlightPreviewForItemWithIdentifier: at.identifier))
        let cut = try XCTUnwrap(ink.mask as? CAShapeLayer, "the run's words are cut out of the paragraph")
        XCTAssertFalse(try XCTUnwrap(cut.path).contains(paragraph.layer.convert(CGPoint(x: box.midX, y: box.midY), to: ink), using: .evenOdd))
        let commit = Commit()
        host.contextMenuInteraction(runs, willPerformPreviewActionForMenuWith: at, animator: commit)
        XCTAssertEqual(commit.preferredCommitStyle, .dismiss)
        XCTAssertNotNil(try node(session, "route-detail"), "the run's link followed")
        host.contextMenuInteraction(runs, willEndFor: at, animator: nil)
        XCTAssertNil(ink.mask, "and back when the menu ends")
    }

    /// A link's Copy Link and Share take its URL as a browser resolves it, on the app's declared origin; only web
    /// links get the host's menu (LLP 1021 §5.1).
    func testALinksMenuTakesTheURLABrowserWouldResolve() {
        let origin = "https://example.com", visit = "/docs/guide?tab=2#top"
        let url = { (href: String) in ContextMenuHost.linkURL(href, origin: origin, visit: visit)?.absoluteString }
        XCTAssertEqual(url("/detail"), "https://example.com/detail")
        XCTAssertEqual(url("about"), "https://example.com/docs/about")
        XCTAssertEqual(url("#moves"), "https://example.com/docs/guide?tab=2#moves")
        XCTAssertEqual(url("?tab=3"), "https://example.com/docs/guide?tab=3")
        XCTAssertEqual(url("//cdn.example.org/x"), "https://cdn.example.org/x")
        XCTAssertEqual(url("https://other.example/a b"), "https://other.example/a%20b")
        XCTAssertNil(ContextMenuHost.linkURL("/detail", origin: nil, visit: visit), "a link in the app has no URL without a declared origin")
        XCTAssertEqual(ContextMenuHost.linkURL("https://other.example/", origin: nil, visit: visit)?.absoluteString, "https://other.example/")
        for href in ["mailto:a@example.com", "tel:+15555550100", "javascript:alert(1)", "JavaScript:void(0)", "myapp://open", ""] {
            XCTAssertFalse(NodeView.hasLinkMenu(href), href)
            XCTAssertNil(url(href), href)
        }
        for href in ["https://example.com", "HTTP://example.com", "/detail", "about", "#moves", "//cdn.example.org/x"] {
            XCTAssertTrue(NodeView.hasLinkMenu(href), href)
        }
    }
}
#endif
