// @ref LLP 1075.003 §9.17 — the element the root tablist names with
// `navigationTabAccessory` is UIKit's tab bar accessory on iOS 26 and later.
//
// The host moves the element's view into the content view of the
// `UITabAccessory` it sets as its tab container's `bottomAccessory`, and the
// kernel lays the element out in that view's bounds, which UIKit sets: a
// content owner's block filled on both axes (LLP 1123 D2). The walks that
// follow the authored tree go from the element to its parent (`paneParent`),
// as from the sidebar's footer.
#if os(iOS)
import UIKit
import ExactCore
import ExactKit

/// The accessory's content view: the element's view while Exact shows it,
/// and, once the element leaves, a picture of it for UIKit to animate out.
final class TabAccessoryContent: UIView, RelayMount {
    weak var node: NodeView?
    /// The element's parent in the authored tree, and its children's ids as
    /// its children ops last gave them, the element's among them.
    weak var home: NodeView?
    var homeOrder: [UInt32]
    /// The tab container whose `bottomAccessory` this is.
    weak var controller: UITabBarController?
    var layoutChanged: (() -> Void)?
    /// The block as UIKit last laid the content view out, sampled in its
    /// layout and its tab container's, and kept through a moment out of the window.
    var told: HostBlock?
    private(set) var picture: UIView?
    /// The authored ancestors' gestures (a long press, a double tap), which UIKit gives only
    /// to recognizers in the views a touch falls in.
    var relay: MountRelay?

    init(node: NodeView, home: NodeView, order: [UInt32], controller: UITabBarController, presenter: Presenter) {
        self.node = node
        self.home = home
        self.controller = controller
        homeOrder = order
        super.init(frame: .zero)
        relay = MountRelay(presenter, mount: self)
    }

    required init?(coder: NSCoder) { nil }

    /// Whether the element's view is still here.
    var holds: Bool { node?.superview === self }

    /// Whether UIKit has laid the content view out once, which its tab container's layout does not show.
    private(set) var laidOut = false

    override func layoutSubviews() {
        super.layoutSubviews()
        laidOut = true
        layoutChanged?()
    }

    /// A touch the element lets through (`pointer-events: none`) goes on to what UIKit holds under it: the content
    /// view is no authored box, so never a target of its own.
    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        let hit = super.hitTest(point, with: event)
        return hit === self ? nil : hit
    }

    override func willRemoveSubview(_ subview: UIView) {
        // A host that takes the view elsewhere (an arrangement) leaves the picture for the exit
        if subview === node { freeze() }
        super.willRemoveSubview(subview)
    }

    /// Covers the element with a picture of it as drawn now, which takes no
    /// input and which VoiceOver does not read: a batch that may take the
    /// element away calls this before its ops hide or dismantle it.
    func freeze() {
        guard picture == nil, let node, holds, !node.isHidden, !bounds.isEmpty else { return }
        // The content view's layers as shown now, within the bounds UIKit clips to, the element's transform and
        // running animations included: the render server has no snapshot of a view it has not shown yet
        let picture = GlassDeparture.picture(of: self)
        picture.accessibilityElementsHidden = true
        addSubview(picture)
        self.picture = picture
    }

    /// The element's authored inertness, `aria-hidden` and tint, which the ancestors that hold it on the page no
    /// longer give its view. A pass ends an edit in it before it goes inert (`projectTabAccessoryOnce`).
    func follow(_ node: NodeView) {
        let inert = node.authoredInert
        isUserInteractionEnabled = !inert
        accessibilityElementsHidden = inert || node.authoredLine.contains { $0.hidesAccessibility }
        // The ancestors' `accent-color` and `-exact-tint-adjustment`, which UIKit's tab container would give it
        // otherwise, as authored: UIKit's own values read dimmed while an alert dims the views behind it
        let tint = home?.ownAccent, adjustment = home?.authoredTintAdjustment ?? .automatic
        if carriedAccent !== tint { carriedAccent = tint; tintColor = tint }
        if carriedAdjustment != adjustment { carriedAdjustment = adjustment; tintAdjustmentMode = adjustment }
    }
    private var carriedAccent: UIColor?, carriedAdjustment = UIView.TintAdjustmentMode.automatic

    /// Takes the picture away while the element still shows.
    func thaw() {
        picture?.removeFromSuperview()
        picture = nil
    }
}

extension NavigationHost {
    /// The element the root tablist names as its tab bar's accessory, while
    /// Exact's own tab container shows the root's tabs in UIKit's bar or
    /// sidebar and no arrangement holds the element as one of its panes.
    func tabAccessoryNode() -> NodeView? {
        guard !tabAccessoryHandingOff, let tabs = tabController, tabOwner === tabs, let root = container,
              let shape = presenter.rootTabs[root.id], let id = shape.accessory,
              let list = presenter.views[shape.tablist], adopts(tablist: list),
              let node = presenter.views[id], let home = node.authoredLine.first(where: { $0 !== node }) else { return nil }
        // A flight, a context menu's preview or an arrangement holding the view keeps it
        if node.superview !== tabAccessory, heldElsewhere(node, under: home) { return nil }
        return node
    }

    /// Shows the element `tabAccessoryNode()` names as the tab bar's accessory,
    /// or takes the accessory away (`reportCovers`). A pass asked for while a
    /// batch is applied runs once the batch ends, when the element's view and
    /// the tabs are those of the whole batch, as the covers that follow it do:
    /// the flag only folds the passes the batch asks for into that one.
    func projectTabAccessory() {
        guard #available(iOS 26.0, *) else { return }
        noteTabAccessory()
        if presenter.applying {
            // Only a claim or a name owes the covers a second pass
            guard tabAccessory != nil || container.flatMap({ presenter.rootTabs[$0.id]?.accessory }) != nil else { return }
            guard !tabAccessoryOwed else { return }
            tabAccessoryOwed = true
            presenter.afterBatch { [weak self] in
                self?.tabAccessoryOwed = false
                self?.reportCovers()
            }
            return
        }
        // UIKit can call back into the covers while a pass sets or clears the accessory: that call's pass runs once
        // this one ends, on what the tree and UIKit say then, so it loses nothing
        if tabAccessoryProjecting { tabAccessoryRerun = true; return }
        tabAccessoryProjecting = true
        defer { tabAccessoryProjecting = false }
        repeat {
            tabAccessoryRerun = false
            projectTabAccessoryOnce()
        } while tabAccessoryRerun
    }

    /// One pass: keep the accessory that shows the named element, or replace or release it. The callbacks an
    /// edit's end and a view's move run (a delegate, a view leaving the window) come before what they could change
    /// is read, or are checked for after it.
    private func projectTabAccessoryOnce() {
        guard #available(iOS 26.0, *) else { return }
        // An edit in an element that goes inert ends first, as one in an inert subtree on the page does
        if let content = tabAccessory, content.isUserInteractionEnabled, content.node?.authoredInert == true { content.endEditing(true) }
        if let content = tabAccessory, let node = tabAccessoryNode(), content.node === node, content.holds,
           let tabs = content.controller, tabs === tabController {
            content.thaw()
            content.follow(node)
            // A hatch that set the property while Exact owns it has it set again
            if tabs.bottomAccessory?.contentView !== content {
                tabs.setBottomAccessory(UITabAccessory(contentView: content), animated: false)
            }
            return
        }
        releaseTabAccessory(animated: true)
        guard tabAccessory == nil, let node = tabAccessoryNode(), let tabs = tabController,
              let home = node.authoredLine.first(where: { $0 !== node }) else { return }
        let content = TabAccessoryContent(node: node, home: home, order: siblings.placedIDs(of: home), controller: tabs, presenter: presenter)
        // The block is told once UIKit lays the content view out
        content.layoutChanged = { [weak self, weak content] in
            guard let self, let content, content === tabAccessory else { return }
            sampleTabAccessory(content)
            presenter.owners.publish()
        }
        // Claimed before UIKit's calls and the view's move, whose callbacks can retire the tabs or take the view
        tabAccessory = content
        // UIKit puts the content view in the window first, so the element moves within it and an editor in it goes
        // on editing; UIKit animates an accessory that comes after the first frame, as it does one an app sets then
        tabs.setBottomAccessory(UITabAccessory(contentView: content), animated: presenter.session?.firstDrawMs != nil)
        // UIKit's calls can retire the tabs: the view moves only while the claim stands
        guard content === tabAccessory, tabs === tabController else { return }
        content.addSubview(node)
        guard content === tabAccessory, node.superview === content, tabs === tabController else {
            // A callback during the move ended the claim or took the view: a view the move still left here goes home
            if content === tabAccessory { releaseTabAccessory(animated: false) }
            else if node.superview === content, presenter.views[home.id] === home, presenter.views[node.id] === node {
                siblings.putBack(node, into: home, order: content.homeOrder)
            }
            return
        }
        content.follow(node)
        presenter.mountsChanged()
    }

    /// Takes the accessory away: the element's view, if it survives, back in
    /// its authored parent in the parent's order, its picture left in the
    /// content view while UIKit animates it out; then `nil` as Exact's last
    /// write to the property of the container it set, where that still holds
    /// Exact's accessory, and the element's block back, unless the caller
    /// publishes it once a handoff ends (`publish`). An edit in a surviving
    /// element goes on, since the element never left the document.
    func releaseTabAccessory(animated: Bool, publish: Bool = true) {
        guard let content = tabAccessory else { return }
        tabAccessory = nil
        content.layoutChanged = nil
        // The row leaving the content view leaves its picture for UIKit's exit (`willRemoveSubview`)
        if let node = content.node, content.holds {
            if let home = content.home, presenter.views[home.id] === home, presenter.views[node.id] === node {
                siblings.putBack(node, into: home, order: content.homeOrder)
            } else {
                node.removeFromSuperview()
            }
        }
        if #available(iOS 26.0, *), let tabs = content.controller, tabs.bottomAccessory?.contentView === content {
            tabs.setBottomAccessory(nil, animated: animated && presenter.session?.firstDrawMs != nil && tabs.viewIfLoaded?.window != nil)
        }
        content.node = nil
        // What UIKit animates out is a picture: it takes no touch and VoiceOver passes it
        content.isUserInteractionEnabled = false
        content.accessibilityElementsHidden = true
        presenter.mountsChanged()
        if publish { presenter.owners.publish() }
    }

    /// Before a batch's ops: the accessory's picture, where the batch destroys
    /// the element or one of its authored ancestors, begins one's authored
    /// exit, or gives one `display: none`, which take it away while it is
    /// still on screen, before the exit's first frame changes it. Other style
    /// changes keep the claim, so they draw nothing. The pass after the batch
    /// takes the picture away if the claim survives (`batchApplied`).
    func freezeTabAccessory(for batch: Batch) {
        guard let content = tabAccessory, let node = content.node, content.holds else { return }
        let line = Set(node.authoredLine.map(\.id))
        let ends = batch.ops.contains { op in
            line.contains(op.id) && (op.op == .destroy || op.op == .exit || op.op == .style && op.style["display"]?.string == "none")
        }
        if ends { content.freeze() }
    }

    /// The element's parent's children as a children op gives them, while
    /// UIKit holds the element.
    func followTabAccessoryHome(_ parent: NodeView, _ ids: [UInt32]) {
        if let content = tabAccessory, content.home === parent { content.homeOrder = ids }
    }

    /// The authored parent of the accessory's element while UIKit holds it.
    func tabAccessoryParent(of view: UIView) -> NodeView? {
        guard let content = tabAccessory, content.node === view, content.holds else { return nil }
        return content.home
    }

    /// The accessory's block, the content view's bounds that UIKit sets,
    /// filled on both axes (LLP 1123 D2), as UIKit last laid it out: none
    /// before its first layout. It has no placement among the root's
    /// children: where it shows is its top-layer box (`tabAccessoryBox`).
    var tabAccessoryBlock: (id: UInt32, block: HostBlock, placement: BlockPlacement?)? {
        guard let content = tabAccessory, let node = content.node, content.holds,
              presenter.views[node.id] === node, let block = content.told else { return nil }
        return (node.id, block, nil)
    }

    /// Where the accessory shows, in the viewport, for as long as the claim
    /// stands: UIKit draws it above the routes, so the runner tests it before
    /// the page, as it does a presented route, where CSS would paint a
    /// positioned panel over a row in flow. Clipped to where UIKit's hit test
    /// reaches it: within every view that holds it, each shown and taking
    /// touches (a navigation root that scrolls moves and clips the tab
    /// container with its content). The content view's own touches follow the
    /// element's inertness, which the runner reads from the tree. Before UIKit
    /// first lays the content view out, and while it is out of the window, the
    /// element shows nowhere: an open box with no area, which the page's walk
    /// passes and no point hits.
    var tabAccessoryBox: TopLayerBox? {
        guard let content = tabAccessory, let node = content.node, content.holds, presenter.views[node.id] === node else { return nil }
        guard let block = tabAccessoryBlock, let holder = content.superview, content.window != nil else {
            return TopLayerBox(view: content, frame: (node.id, .zero, .zero), modal: nil)
        }
        let viewport = presenter.viewport
        let rect = NativeGeometry.bounds(of: content, in: viewport)
        let clip = NativeGeometry.hitRegion(of: holder, in: viewport).map { $0.intersection(rect) } ?? .zero
        return TopLayerBox(view: content, frame: (block.id, presenter.viewportPoints(rect), presenter.viewportPoints(clip.isNull ? .zero : clip)), modal: nil)
    }

    /// The content view's box as UIKit has just laid it out, while it is in the window.
    func sampleTabAccessory(_ content: TabAccessoryContent) {
        guard content.laidOut, content.window != nil, !content.bounds.isEmpty else { return }
        content.told = HostBlock(width: .fill(content.bounds.width), height: .fill(content.bounds.height))
    }

    /// Writes why the root tablist's accessory names no element to a
    /// development build's journal, once for each tablist and note.
    private func noteTabAccessory() {
        // Only where an accessory can show: Exact's own tab container showing the root's tabs
        guard HatchDiagnosticsLink.available, let tabs = tabController, tabOwner === tabs, let root = container,
              let shape = presenter.rootTabs[root.id], let list = presenter.views[shape.tablist], adopts(tablist: list),
              let note = shape.accessoryNote, tabAccessoryNoted.insert("\(shape.tablist) \(note)").inserted else { return }
        HostLog.note(presenter.session, "tab accessory: \(note)")
    }
}
#endif
