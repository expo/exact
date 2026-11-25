#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

/// LLP 1021 §5.1 over the native fixture's Chat title: a long press on the
/// bar's title opens its pressable group's context popover, which stands only
/// while the bar shows that title for that group; with no preview row the
/// avatar lifts alone. UIKit synthesizes no long press for a unit test: the
/// interaction's delegate is asked as UIKit asks it.
extension NavigationBasicsIOSTests {
    private func titleMenu(_ label: String) throws -> (ExactSession, RouteController, HeaderTitleView, UIContextMenuInteraction) {
        if ExactEnv.agentPaints { throw XCTSkip("UIKit owns these menus under native presentation") }
        let session = try fixture(label)
        let (_, nav, chat) = try chat(session)
        nav.view.layoutIfNeeded()
        nav.navigationBar.layoutIfNeeded()
        let view = try XCTUnwrap(chat.navigationItem.titleView as? HeaderTitleView)
        until("the projected title is mounted") { view.window != nil && view.bounds.width > 0 }
        let interaction = try XCTUnwrap(view.interactions.compactMap { $0 as? UIContextMenuInteraction }.first)
        return (session, chat, view, interaction)
    }

    private func configureTitle(_ session: ExactSession, _ view: HeaderTitleView, _ interaction: UIContextMenuInteraction) throws -> UIContextMenuConfiguration {
        try XCTUnwrap(session.presenter.menus.context.contextMenuInteraction(interaction,
            configurationForMenuAtLocation: CGPoint(x: view.bounds.midX, y: view.bounds.midY)))
    }

    private final class TitleCommit: NSObject, UIContextMenuInteractionCommitAnimating {
        var preferredCommitStyle: UIContextMenuInteractionCommitStyle = .pop
        var previewViewController: UIViewController? { nil }
        func addAnimations(_ animations: @escaping () -> Void) { animations() }
        func addCompletion(_ completion: @escaping () -> Void) { completion() }
    }

    /// UIKit asks for the highlight as the press begins, before the preview:
    /// the title lifts whole with its card, the group hears `contextmenu`
    /// only once the preview is asked for, and a tap on the card presses it.
    func testATitlesPreviewRowLiftsFromThePopoverAndATapPressesIt() throws {
        let (session, _, view, interaction) = try titleMenu("title-preview")
        let p = session.presenter, menus = p.menus.context
        let source = try node(session, "title-group"), card = try node(session, "title-card")
        XCTAssertTrue(try node(session, "header-chat").isHidden)
        XCTAssertFalse(p.menus.eligible(source, contextMenu: true), "the page hides the group for the bar")
        XCTAssertTrue(p.navigationHost.projectedTitle(for: source) === view)
        let configuration = try configureTitle(session, view, interaction)
        let owner = try XCTUnwrap(menus.open)
        let lifted = try XCTUnwrap(menus.contextMenuInteraction(interaction, configuration: configuration,
            highlightPreviewForItemWithIdentifier: configuration.identifier))
        XCTAssertTrue(lifted.view === view, "with a preview row the whole title is the source")
        waitForMainQueue()
        XCTAssertEqual(text(session, "title-holds"), "title holds 0", "a press that only began fires no `contextmenu`")
        let preview = try XCTUnwrap(menus.lift(owner))
        until("the preview fires the group's `contextmenu`") { self.text(session, "title-holds") == "title holds 1" }
        XCTAssertTrue(card.superview === preview.view)
        XCTAssertNotNil(menus.menu(owner))
        menus.contextMenuInteraction(interaction, willPerformPreviewActionForMenuWith: configuration, animator: TitleCommit())
        until("the card pressed") { self.text(session, "title-presses") == "title presses 1" }
        menus.contextMenuInteraction(interaction, willEndFor: configuration, animator: nil)
        XCTAssertNil(menus.open)
    }

    /// A title taken out of its bar while its menu is open ends the menu,
    /// which the page cannot reopen on its own terms.
    func testADetachedTitlesMenuIsNotReopenedOnThePagesTerms() throws {
        let (session, _, view, interaction) = try titleMenu("title-detached")
        let p = session.presenter, menus = p.menus.context
        _ = try configureTitle(session, view, interaction)
        let owner = try XCTUnwrap(menus.open), source = try node(session, "title-group")
        view.removeInteraction(interaction)
        try node(session, "header-chat").isHidden = false
        XCTAssertTrue(p.menus.eligible(source, contextMenu: true))
        XCTAssertNil(menus.menu(owner))
        XCTAssertNil(menus.lift(owner))
    }

    func testATitleWithoutAPreviewRowLiftsItsAvatarAloneWhereItIs() throws {
        let (session, _, view, interaction) = try titleMenu("title-avatar-lift")
        let menus = session.presenter.menus.context
        try node(session, "title-card").props["contextPreview"] = nil
        view.transform = CGAffineTransform(translationX: 7, y: 3).scaledBy(x: 0.8, y: 0.8)
        view.avatar.transform = CGAffineTransform(scaleX: 0.75, y: 0.75)
        let configuration = try configureTitle(session, view, interaction)
        let lifted = try XCTUnwrap(menus.contextMenuInteraction(interaction, configuration: configuration,
            highlightPreviewForItemWithIdentifier: configuration.identifier))
        XCTAssertTrue(lifted.view === view.avatar)
        XCTAssertFalse(try XCTUnwrap(menus.open.flatMap { menus.menu($0) }, "the menu, with no preview").children.isEmpty)
        XCTAssertEqual(lifted.parameters.backgroundColor, .clear)
        let path = try XCTUnwrap(lifted.parameters.visiblePath)
        XCTAssertEqual(path.bounds, view.avatar.bounds)
        XCTAssertFalse(path.contains(CGPoint(x: 1, y: 1)), "in the avatar's round outline")
        XCTAssertEqual(lifted.target.transform.a, 0.6, accuracy: 0.001, "the title's scale and the avatar's")
        view.center.x += 11
        let returned = try XCTUnwrap(menus.contextMenuInteraction(interaction, configuration: configuration,
            dismissalPreviewForItemWithIdentifier: configuration.identifier))
        XCTAssertTrue(returned.view === view.avatar)
        XCTAssertEqual(returned.target.center, view.avatar.convert(CGPoint(x: view.avatar.bounds.midX, y: view.avatar.bounds.midY), to: returned.target.container),
                       "the dismissal targets where the avatar is now")
        menus.contextMenuInteraction(interaction, willEndFor: configuration, animator: nil)
    }

    func testATitlesMenuNeedsItsGroupEnabledAndTheTitleShownAndUncovered() throws {
        let (session, chat, view, interaction) = try titleMenu("title-eligibility")
        let p = session.presenter, source = try node(session, "title-group")
        let location = CGPoint(x: view.bounds.midX, y: view.bounds.midY)
        func configuration() -> UIContextMenuConfiguration? {
            p.menus.context.contextMenuInteraction(interaction, configurationForMenuAtLocation: location)
        }
        for prop in ["disabled", "inert"] {
            source.props[prop] = "true"
            XCTAssertNil(configuration(), prop)
            source.props[prop] = nil
        }
        view.isHidden = true
        XCTAssertNil(configuration(), "a hidden title")
        view.isHidden = false
        view.alpha = 0
        XCTAssertNil(configuration(), "a transparent title")
        view.alpha = 1
        let cover = UIView(frame: try XCTUnwrap(view.window).bounds)
        view.window?.addSubview(cover)
        XCTAssertNil(configuration(), "a covered title")
        cover.removeFromSuperview()
        chat.navigationItem.titleView = UILabel()
        XCTAssertNil(configuration(), "a hatch's title in its place")
        chat.navigationItem.titleView = view
        chat.navigationController?.navigationBar.layoutIfNeeded()
        let valid = try XCTUnwrap(configuration())
        p.menus.context.contextMenuInteraction(interaction, willEndFor: valid, animator: nil)
        p.apply(wireBatch([["op": "style", "id": source.id, "style": ["display": "none"]]]))
        XCTAssertNil(configuration(), "a group its author hides is not one the bar hides")
    }

    func testATitlesMenuEndsWithItsSourceAndItsRoute() throws {
        let (session, _, view, interaction) = try titleMenu("title-retirement")
        let p = session.presenter, menus = p.menus.context
        let configuration = try configureTitle(session, view, interaction)
        let owner = try XCTUnwrap(menus.open), source = try node(session, "title-group")
        _ = try XCTUnwrap(menus.lift(owner))
        let (_, _, chat) = try chat(session)
        chat.navigationItem.titleView = UILabel()
        menus.sync()
        XCTAssertNil(menus.open, "a hatch's title in its place ends the menu")
        XCTAssertNil(menus.menu(owner))
        XCTAssertNil(menus.contextMenuInteraction(interaction, configuration: configuration, dismissalPreviewForItemWithIdentifier: configuration.identifier))
        chat.navigationItem.titleView = view
        // The title projected again owns a new menu, without reviving the old one
        try tapNode(session, "toggle-subtitle")
        until("the title projects its group again") { view.interactions.contains { $0 is UIContextMenuInteraction } }
        let next = try XCTUnwrap(view.interactions.compactMap { $0 as? UIContextMenuInteraction }.first)
        let current = try configureTitle(session, view, next)
        try tapNode(session, "chat-detail")
        until("Detail covers Chat and the menu ends") { !p.navigation.inTransition && p.navigationHost.projectedTitle(for: source) == nil && menus.open == nil }
        XCTAssertNil(menus.contextMenuInteraction(next, configuration: current, dismissalPreviewForItemWithIdentifier: current.identifier))
    }

    func testTheAgentsLongPressOnATitleOpensItsGroupsPopover() throws {
        guard ExactEnv.agentPaints else { throw missing("agent", "run with EXACT_AGENT=1, an agent socket and EXACT_AGENT_CHROME=platform") }
        if ExactEnv.authoredChrome { throw XCTSkip("the agent opens a title's menu from UIKit's bar, under --chrome platform") }
        let session = try fixture("title-agent"), p = session.presenter
        let (_, _, chat) = try chat(session)
        let view = try XCTUnwrap(chat.navigationItem.titleView as? HeaderTitleView)
        let source = try node(session, "title-group"), popover = try XCTUnwrap(p.carrying("popover").first { $0.props["id"] == "title-menu" })
        let agent = Agent(session: session), request: [String: Any] = ["id": Int(source.id), "contextmenu": true]
        let reply = agent.tap(request)
        XCTAssertNil(reply["error"])
        XCTAssertEqual(reply["injected"] as? Bool, true)
        XCTAssertTrue(p.menus.isOpen(popover))
        let titleBox = view.convert(view.bounds, to: nil), menuBox = popover.convert(popover.bounds, to: nil)
        XCTAssertGreaterThanOrEqual(menuBox.minY, titleBox.midY, "the popover opens from the title, not the hidden page header")
        XCTAssertTrue(menuBox.minX < titleBox.maxX && titleBox.minX < menuBox.maxX)
        until("the group heard it") { self.text(session, "title-holds") == "title holds 1" }
        _ = agent.back()
        source.props["contextPopover"] = "missing"
        source.handlers.remove("contextmenu")
        XCTAssertNotNil(agent.tap(request)["error"], "no popover by that id, and no handler: nothing opened")
        source.props["contextPopover"] = "title-menu"
        source.handlers.insert("contextmenu")
        source.props["disabled"] = "true"
        XCTAssertNotNil(agent.tap(request)["error"])
        source.props["disabled"] = nil
        let cover = UIView(frame: try XCTUnwrap(view.window).bounds)
        view.window?.addSubview(cover)
        XCTAssertNotNil(agent.tap(request)["error"], "a covered title")
        cover.removeFromSuperview()
        XCTAssertFalse(p.menus.isOpen(popover))
    }
}
#endif
