// The drag-out capability (LLP 1094.000, the roster's `drag_out`): a node
// with `draggable` lifts what its `dataTransfer` declares into another app.
// An app's composition links this module when its plan has a `draggable`,
// and installs it before any session is made; the core reaches it only
// through `DragOutLink`. iOS and iPadOS (D4) here, the Mac (D5) in
// `DragOutMac.swift`; tvOS has no drag.
#if os(iOS)
import ExactCore
import ExactKit
import UIKit
import UniformTypeIdentifiers

public enum ExactDragOut {
    public static func install() { DragOutLink.installed = DragOutHost() }
}

final class DragOutHost: DragOutCapability {
    func sources(_ presenter: Presenter) -> DragOutSources { DragSources(presenter) }
}

/// One `UIDragInteraction` on each node carrying `draggable=true`, keyed by
/// node id, as `ContextMenuHost` keys its menus. The items are read from the
/// node's admitted props when a lift begins, and nothing runs in the app
/// until UIKit's callback returns: then each node that joined the drag hears
/// `dragstart`, and at the drag's end `dragend` (D16).
final class DragSources: NSObject, DragOutSources, UIDragInteractionDelegate {
    private weak var presenter: Presenter?
    private var interactions: [UInt32: UIDragInteraction] = [:]
    /// The recognizers and interactions UIKit added with each of ours, which
    /// park with it in the node pool.
    private var companions: [ObjectIdentifier: Set<ObjectIdentifier>] = [:]
    /// Views parked with an interaction on them, which the next sync gives to
    /// the node the view is lent to, or takes off.
    private let parkedViews = NSHashTable<NodeView>.weakObjects()
    /// A hash of the `url` value each draggable node last had journaled as
    /// unreadable, kept only while the node is in the tree.
    private var unreadable: [UInt32: Int] = [:]
    /// The drags under way, by session: the nodes that joined each, with the
    /// point in each node's box where it joined, and the app's own drop side's
    /// view of it (D16).
    private var drags: [ObjectIdentifier: Drag] = [:]
    private final class Drag {
        var own: OwnDrag?
        var sources: [(node: DragItems.Lifted, line: String)] = []
    }
    /// The node a tap adds to a drag, and where in its box, from UIKit's
    /// question until it adds the node's items.
    private var adding: (node: DragItems.Lifted, line: String)?

    init(_ presenter: Presenter) { self.presenter = presenter }

    func sync() {
        guard let presenter else { return }
        var wanted: [UInt32: NodeView] = [:]
        // None while the agent draws the app itself, as for menus (D4)
        var draggable = Set<UInt32>()
        if !ExactEnv.agentPaints {
            for v in presenter.carrying("draggable") where v.props["draggable"] == "true" {
                draggable.insert(v.id)
                if Self.holds(v) { wanted[v.id] = v }
                journalUnreadableURL(of: v, in: presenter)
            }
        }
        unreadable = unreadable.filter { draggable.contains($0.key) }
        // A drag's source keeps its interaction while the drag runs, whose end it hears through it, though it no
        // longer lifts (`lifts`)
        for source in drags.values.flatMap(\.sources) where wanted[source.node.id] == nil {
            if let view = live(source.node), interactions[view.id]?.view === view { wanted[view.id] = view }
        }
        for (id, interaction) in interactions where wanted[id] == nil || interaction.view !== wanted[id] {
            if let view = interaction.view { detach(interaction, from: view) } else { companions[ObjectIdentifier(interaction)] = nil }
            interactions[id] = nil
        }
        // A parked view lent to a new node keeps its interaction if that node
        // holds one too, else it comes off.
        for v in parkedViews.allObjects where presenter.views[v.id] === v {
            parkedViews.remove(v)
            if !wanted.values.contains(where: { $0 === v }) { ours(v).forEach { detach($0, from: v) } }
        }
        for (id, v) in wanted {
            let interaction = interactions[id] ?? ours(v).first ?? attach(to: v)
            interactions[id] = interaction
            // `enabledByDefault` depends on the device (D4).
            interaction.isEnabled = true
            // Beside a swipe a pointer waits for the lift delay, as a finger does (D8).
            if #available(iOS 27.0, *) { interaction.allowsPointerDragBeforeLiftDelay = !swipes(v) }
        }
    }

    /// A node's `url` that Foundation cannot read (`URL(string:)`), which
    /// the drag leaves out, journaled once as its value arrives, whether or
    /// not the node carries anything else, and without the URL itself, which
    /// may carry a token.
    private func journalUnreadableURL(of node: NodeView, in presenter: Presenter) {
        guard let value = node.props["dataTransferUrl"], !value.isEmpty, URL(string: value) == nil else { unreadable[node.id] = nil; return }
        guard unreadable[node.id] != value.hashValue else { return }
        unreadable[node.id] = value.hashValue
        presenter.session?.log("drag: url not carried: Foundation cannot read it as a URL; percent-encode what a browser would")
    }

    func reset() {
        unreadable.removeAll()
        drags.removeAll()
        adding = nil
        for (_, interaction) in interactions {
            if let view = interaction.view { detach(interaction, from: view) } else { companions[ObjectIdentifier(interaction)] = nil }
        }
        interactions.removeAll()
    }

    func owns(_ interaction: UIInteraction, on view: UIView) -> Bool {
        (interaction as? UIDragInteraction)?.delegate === self || came(ObjectIdentifier(interaction), on: view)
    }
    func owns(_ recognizer: UIGestureRecognizer, on view: UIView) -> Bool { came(ObjectIdentifier(recognizer), on: view) }

    func parked(_ view: NodeView) {
        interactions = interactions.filter { $0.value.view !== view }
        if !ours(view).isEmpty { parkedViews.add(view) }
    }
    func dropped(_ view: NodeView) {
        parkedViews.remove(view)
        ours(view).forEach { detach($0, from: view) }
    }

    /// The drags an assistive technology offers on an element (D9): that of
    /// the nearest node at or above `view` that holds one of these
    /// interactions, and, when `view` is the element, each in its subtree, in
    /// node order, each at its node's activation point (`init(name:view:)`),
    /// where VoiceOver's touch lands. UIKit leaves out one whose view is
    /// obscured or out of a visible window (Document:
    /// `UIAccessibilityLocationDescriptor.h:43-45`), and the lift decides
    /// whether a drag begins (`lifted`). Nil keeps UIKit's own where it
    /// offers the same (Document: `UIAccessibility.h:445-450`): the one drag,
    /// on the element's view or in its subtree, unless a canvas surface
    /// places its node (LLP 1014 D5), whose activation point follows the
    /// placement and UIKit's own does not. Each is named by its node's
    /// accessible name, else by what it carries (`DragItems.name`). Where
    /// UIKit's own would also find an interaction that cannot lift now, an
    /// inert node's, the offer is explicit, if empty.
    func descriptors(of view: UIView, isElement: Bool) -> [UIAccessibilityLocationDescriptor]? {
        guard let presenter else { return nil }
        let installed = interactions.sorted { $0.key < $1.key }.compactMap { id, interaction in
            presenter.views[id].flatMap { $0 === interaction.view ? $0 : nil }
        }
        // Read again as asked: a node's inertness can change with no batch for it
        let held = installed.filter(Self.lifts)
        let around = held.filter { view.isDescendant(of: $0) }
        // The nearest is inside every other around it
        let above = around.first { node in around.allSatisfy { node.isDescendant(of: $0) } }
        let inside = isElement ? held.filter { $0 !== view && $0.isDescendant(of: view) } : []
        let nodes = (above.map { [$0] } ?? []) + inside
        // UIKit's own finds every interaction in the view's subtree, an inert node's too
        let unliftable = isElement && installed.contains { !Self.lifts($0) && $0.isDescendant(of: view) }
        if !unliftable, nodes.isEmpty || (isElement && nodes.count == 1 && nodes[0].isDescendant(of: view) && nodes[0].placedAncestor == nil) {
            return nil
        }
        return nodes.map { node in
            UIAccessibilityLocationDescriptor(name: node.accessibleName.isEmpty ? DragItems.name(of: node) ?? "" : node.accessibleName, view: node)
        }
    }

    private func ours(_ view: UIView) -> [UIDragInteraction] {
        view.interactions.compactMap { $0 as? UIDragInteraction }.filter { $0.delegate === self }
    }
    private func attach(to view: UIView) -> UIDragInteraction {
        let interaction = UIDragInteraction(delegate: self)
        let before = Self.companions(of: view)
        view.addInteraction(interaction)
        companions[ObjectIdentifier(interaction)] = Self.companions(of: view).subtracting(before).subtracting([ObjectIdentifier(interaction)])
        return interaction
    }
    private func detach(_ interaction: UIDragInteraction, from view: UIView) {
        let added = companions.removeValue(forKey: ObjectIdentifier(interaction)) ?? []
        view.removeInteraction(interaction)
        for other in view.interactions where added.contains(ObjectIdentifier(other)) { view.removeInteraction(other) }
        for other in view.gestureRecognizers ?? [] where added.contains(ObjectIdentifier(other)) { view.removeGestureRecognizer(other) }
    }
    private func came(_ id: ObjectIdentifier, on view: UIView) -> Bool {
        ours(view).contains { companions[ObjectIdentifier($0)]?.contains(id) == true }
    }
    private static func companions(of view: UIView) -> Set<ObjectIdentifier> {
        Set((view.gestureRecognizers ?? []).map(ObjectIdentifier.init) + view.interactions.map(ObjectIdentifier.init))
    }
    private func swipes(_ v: NodeView) -> Bool { v.handlers.contains("swipeleft") || v.handlers.contains("swiperight") }

    // MARK: UIDragInteractionDelegate

    func dragInteraction(_ interaction: UIDragInteraction, itemsForBeginning session: UIDragSession) -> [UIDragItem] {
        guard let node = lifted(by: interaction, at: session) ?? asked(by: interaction) else { return [] }
        return DragItems.items(of: node, in: presenter?.session)
    }

    /// A tap on another node that could lift, while a drag is under way,
    /// adds that node's items to it (D4, "Adding to a drag"): checked as a
    /// lift is, and none from a node whose items the drag already carries.
    func dragInteraction(_ interaction: UIDragInteraction, itemsForAddingTo session: UIDragSession, withTouchAt point: CGPoint) -> [UIDragItem] {
        guard let view = interaction.view, let window = view.window,
              let node = lifted(by: interaction, atWindowPoint: view.convert(point, to: window)) else { return [] }
        let carried = session.items.contains { item in
            (item.localObject as? DragItems.Lifted).map { $0.id == node.id && $0.incarnation == node.incarnation } ?? false
        }
        guard !carried else { return [] }
        adding = (DragItems.Lifted(node), Self.line(node, at: view.convert(point, to: node)))
        return DragItems.items(of: node, in: presenter?.session)
    }

    // MARK: The drag's start and end (D16)

    /// The lifted node joins the drag at the session's touch; it hears its
    /// `dragstart` once this returns, or as the app's own drop side first
    /// hears the drag.
    func dragInteraction(_ interaction: UIDragInteraction, sessionWillBegin session: UIDragSession) {
        let drag = Drag()
        // One source a node, whose items are several when it carries several values
        var nodes: [DragItems.Lifted] = []
        for case let node as DragItems.Lifted in session.items.map(\.localObject)
        where !nodes.contains(where: { $0.id == node.id && $0.incarnation == node.incarnation }) { nodes.append(node) }
        let lifted = nodes.compactMap { node in live(node).map { (node, Self.line($0, at: session.location(in: $0))) } }
        drag.sources = lifted
        let own = OwnDrag { [weak self] in
            for source in lifted { self?.tell(source.0, 69, source.1) }
        }
        drag.own = own
        drags[ObjectIdentifier(session)] = drag
        session.localContext = own
    }

    /// A tapped node joins the drag: it hears its `dragstart` once this
    /// returns, after the drag's own.
    func dragInteraction(_ interaction: UIDragInteraction, session: UIDragSession, willAdd items: [UIDragItem], for addingInteraction: UIDragInteraction) {
        guard let drag = drags[ObjectIdentifier(session)], let adding,
              items.contains(where: { ($0.localObject as? DragItems.Lifted).map { $0.id == adding.node.id && $0.incarnation == adding.node.incarnation } ?? false })
        else { return }
        self.adding = nil
        drag.sources.append(adding)
        DispatchQueue.main.async { [weak self] in
            drag.own?.starting()
            self?.tell(adding.node, 69, adding.line)
        }
    }

    /// A drop never moves anything of the app's: a destination takes a copy.
    func dragInteraction(_ interaction: UIDragInteraction, sessionAllowsMoveOperation session: UIDragSession) -> Bool { false }

    /// Each node that joined the drag, still mounted as that node, hears
    /// `dragend` once this returns, after the app's own drop of it lands: a
    /// copy, or none for a cancelled or refused drag. The drag's record goes
    /// first, so a `dragend` action that lifts again begins another.
    func dragInteraction(_ interaction: UIDragInteraction, session: UIDragSession, didEndWith operation: UIDropOperation) {
        guard let drag = drags.removeValue(forKey: ObjectIdentifier(session)) else { return }
        DispatchQueue.main.async { [weak self] in
            drag.own?.ended(taken: operation == .copy) { taken in
                guard let self else { return }
                for source in drag.sources { self.tell(source.node, 70, source.line + "\n" + (taken ? "copy" : "none")) }
                self.sync()
            }
        }
    }

    /// `node`'s view while it is still that node, mounted.
    private func live(_ node: DragItems.Lifted) -> NodeView? {
        guard let view = presenter?.views[node.id], view === node.view, view.incarnation == node.incarnation else { return nil }
        return view
    }

    /// `node` hears an event of `kind`, its point line `line`, if it is still that node.
    private func tell(_ node: DragItems.Lifted, _ kind: UInt32, _ line: String) {
        guard let view = live(node) else { return }
        presenter?.clipboard(view.id, kind, line)
    }

    /// A `DragEvent`'s point line at `point` in `node`: from its content box,
    /// with no modifier keys, which a drag session does not report.
    private static func line(_ node: NodeView, at point: CGPoint) -> String {
        let box = node.contentBox()
        return "\(Double(point.x - box.minX)),\(Double(point.y - box.minY))"
    }

    /// The node from whose view UIKit shows a context menu (D4). A request for
    /// its items then is the menu's, Ask Siri's (WWDC26 session 278) or a drag of
    /// the menu's preview, with no touch on the node, so it is answered as a
    /// hand-built delegate answers any.
    private func asked(by interaction: UIDragInteraction) -> NodeView? {
        guard let presenter, let id = interactions.first(where: { $0.value === interaction })?.key, let node = presenter.views[id],
              interaction.view === node, Self.lifts(node), presenter.menus.showsContextMenu(of: node) else { return nil }
        return node
    }

    /// The node `interaction` lifts at `session`'s touch: where the touch
    /// lands now, as the window routes it, through a canvas surface's
    /// placement too, must be the node, able to lift, and the touch not kept
    /// from it (`keepsContact`). The hit test runs views' own code, so the
    /// node must still be the same one, in the same incarnation and window,
    /// holding the same interaction, after it.
    private func lifted(by interaction: UIDragInteraction, at session: UIDragSession) -> NodeView? {
        guard let window = interaction.view?.window else { return nil }
        return lifted(by: interaction, atWindowPoint: session.location(in: window))
    }

    /// The node `interaction` lifts at `point` in its window, as `lifted(by:at:)` reads a session's touch.
    private func lifted(by interaction: UIDragInteraction, atWindowPoint point: CGPoint) -> NodeView? {
        guard let presenter, let id = interactions.first(where: { $0.value === interaction })?.key, let node = presenter.views[id],
              let window = node.window, interaction.view?.window === window else { return nil }
        let incarnation = node.incarnation
        guard let hit = window.hitTest(point, with: nil), presenter.views[id] === node, node.incarnation == incarnation,
              node.window === window,
              interactions[id] === interaction, interaction.view === node,
              Self.lifts(node), hit.isDescendant(of: node), !Self.keepsContact(node, from: hit, in: presenter) else { return nil }
        return node
    }

    func dragInteraction(_ interaction: UIDragInteraction, previewForLifting item: UIDragItem, session: UIDragSession) -> UITargetedDragPreview? {
        guard let node = carried(by: item), interaction.view === node else { return nil }
        return Self.preview(of: node)
    }

    /// Back to where the node is now; nil, which fades the item in place,
    /// when its view has gone or is another node's.
    func dragInteraction(_ interaction: UIDragInteraction, previewForCancelling item: UIDragItem, withDefault defaultPreview: UITargetedDragPreview) -> UITargetedDragPreview? {
        carried(by: item).flatMap(Self.preview(of:))
    }

    /// The node whose data `item` carries, while its view is still that node's: nil once the view has gone or been lent
    /// to another node, whose preview would not be the item's.
    private func carried(by item: UIDragItem) -> NodeView? {
        guard let lifted = item.localObject as? DragItems.Lifted, let view = lifted.view, presenter?.views[lifted.id] === view,
              view.incarnation == lifted.incarnation else { return nil }
        return view
    }

    /// The node itself, over the controller's view, so a strip that clips does
    /// not cut the lift, on its own layer fill, which UIKit's lifted copy
    /// leaves out, or as its clip path shapes it (`LiftTarget.clipLift`): the
    /// context menu's preview of a node, which a node with both lifts once
    /// (D4).
    static func preview(of view: NodeView) -> UITargetedDragPreview? {
        guard !view.isHidden else { return nil }
        let parameters = UIDragPreviewParameters()
        if let lift = LiftTarget.clipLift(view) {
            parameters.visiblePath = lift.outline
            parameters.backgroundColor = lift.ground
        } else {
            parameters.visiblePath = LiftTarget.rounded(view)
            parameters.backgroundColor = view.ownFill.flatMap { $0.alpha > 0 ? UIColor(cgColor: $0) : nil } ?? .clear
        }
        guard let target = LiftTarget.target(view, around: parameters.visiblePath?.bounds) else { return nil }
        return UITargetedDragPreview(view: view, parameters: parameters, target: target)
    }

    /// Whether `node` holds an interaction (D9): draggable, not disabled, and
    /// carrying something, which UIKit lifts nothing without. Inertness is
    /// read as a lift begins instead (`lifts`), since a modal makes a node
    /// inert and takes that back with no batch for the node.
    static func holds(_ node: NodeView) -> Bool {
        node.props["draggable"] == "true" && !node.disabled && DragItems.name(of: node) != nil
    }

    /// Whether `node` can lift now (D8): it holds an interaction and is not
    /// inert, which takes no touch.
    static func lifts(_ node: NodeView) -> Bool { holds(node) && !node.inert }

    /// Whether a touch that lands on `hit` keeps its contact from `node`'s
    /// lift (D8): a disabled node from `hit` up to `node`, or one inside it
    /// that lifts itself, as the Mac and Android take the innermost that
    /// carries something; selectable text, `node` itself included, whose selection
    /// takes the touch (`touchSelectsText`); or a text input, or a control
    /// that tracks a drag or a held press (a slider, switch, stepper,
    /// segmented control or date picker), of a node inside `node`. Any other
    /// control is a press, a button or a checkbox: a rested finger there
    /// lifts the node, as a long press there opens the node's context menu.
    /// The nodes are the presenter's, by identity.
    static func keepsContact(_ node: NodeView, from hit: UIView?, in presenter: Presenter) -> Bool {
        if node.disabled || (hit === node && node.touchSelectsText) { return true }
        let nodes = Dictionary(presenter.views.values.map { (ObjectIdentifier($0), $0) }, uniquingKeysWith: { first, _ in first })
        var view = hit, control = false
        while let v = view, v !== node {
            let inner = nodes[ObjectIdentifier(v)]
            if inner?.disabled == true || inner.map(Self.lifts) == true || inner?.touchSelectsText == true { return true }
            control = control || v is UITextInput || v is UISlider || v is UISwitch || v is UIStepper || v is UISegmentedControl || v is UIDatePicker
            if control, inner != nil { return true }
            view = v.superview
        }
        return false
    }
}
#elseif !os(macOS)
public enum ExactDragOut {
    public static func install() {}
}
#endif
