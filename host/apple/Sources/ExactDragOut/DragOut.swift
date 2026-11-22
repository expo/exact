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
/// node's admitted props when a lift begins, and nothing runs in the app.
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
        guard let node = lifted(by: interaction, at: session) else { return [] }
        return DragItems.items(of: node, in: presenter?.session)
    }

    /// The node `interaction` lifts at `session`'s touch: where the touch
    /// lands now, as the window routes it, through a canvas surface's
    /// placement too, must be the node, able to lift, and the touch not kept
    /// from it (`keepsContact`). The hit test runs views' own code, so the
    /// node must still be the same one, in the same incarnation and window,
    /// holding the same interaction, after it.
    private func lifted(by interaction: UIDragInteraction, at session: UIDragSession) -> NodeView? {
        guard let presenter, let id = interactions.first(where: { $0.value === interaction })?.key, let node = presenter.views[id],
              let window = node.window else { return nil }
        let incarnation = node.incarnation
        guard let hit = window.hitTest(session.location(in: window), with: nil), presenter.views[id] === node, node.incarnation == incarnation,
              node.window === window,
              interactions[id] === interaction, interaction.view === node,
              Self.lifts(node), hit.isDescendant(of: node), !Self.keepsContact(node, from: hit, in: presenter) else { return nil }
        return node
    }

    func dragInteraction(_ interaction: UIDragInteraction, previewForLifting item: UIDragItem, session: UIDragSession) -> UITargetedDragPreview? {
        interaction.view.flatMap(Self.preview(of:))
    }

    /// Back to where the node is now; nil, which fades the item in place,
    /// when its view has gone or is another node's.
    func dragInteraction(_ interaction: UIDragInteraction, previewForCancelling item: UIDragItem, withDefault defaultPreview: UITargetedDragPreview) -> UITargetedDragPreview? {
        guard let lifted = item.localObject as? DragItems.Lifted, let view = lifted.view, presenter?.views[lifted.id] === view,
              view.incarnation == lifted.incarnation else { return nil }
        return Self.preview(of: view)
    }

    /// The node itself, over the controller's view, so a strip that clips does
    /// not cut the lift, on its own layer fill, which UIKit's lifted copy
    /// leaves out: the context menu's preview of a node (`LiftTarget`), which
    /// a node with both lifts once (D4).
    static func preview(of view: UIView) -> UITargetedDragPreview? {
        guard !view.isHidden, let target = LiftTarget.target(view) else { return nil }
        let parameters = UIDragPreviewParameters()
        parameters.visiblePath = LiftTarget.rounded(view)
        parameters.backgroundColor = view.layer.backgroundColor.flatMap { $0.alpha > 0 ? UIColor(cgColor: $0) : nil } ?? .clear
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
