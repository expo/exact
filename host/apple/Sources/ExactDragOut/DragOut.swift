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

    init(_ presenter: Presenter) { self.presenter = presenter }

    func sync() {
        guard let presenter else { return }
        var wanted: [UInt32: NodeView] = [:]
        // None while the agent draws the app itself, as for menus (D4).
        if !ExactEnv.agentPaints {
            for v in presenter.carrying("draggable") where v.props["draggable"] == "true" { wanted[v.id] = v }
        }
        for (id, interaction) in interactions where wanted[id] == nil || interaction.view !== wanted[id] {
            if let view = interaction.view { detach(interaction, from: view) } else { companions[ObjectIdentifier(interaction)] = nil }
            interactions[id] = nil
        }
        // A parked view lent to a new node keeps its interaction if that node
        // is draggable too, else it comes off.
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

    func reset() {
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
        guard let presenter, let id = interactions.first(where: { $0.value === interaction })?.key, let node = presenter.views[id],
              node.props["draggable"] == "true", !Self.startsInControl(node, at: session.location(in: node), in: presenter) else { return [] }
        return DragItems.items(of: node, in: presenter.session)
    }

    func dragInteraction(_ interaction: UIDragInteraction, previewForLifting item: UIDragItem, session: UIDragSession) -> UITargetedDragPreview? {
        interaction.view.flatMap(Self.preview(of:))
    }

    /// Back to where the node is now; nil, which fades the item in place,
    /// when its view has gone or is another node's.
    func dragInteraction(_ interaction: UIDragInteraction, previewForCancelling item: UIDragItem, withDefault defaultPreview: UITargetedDragPreview) -> UITargetedDragPreview? {
        guard let lifted = item.localObject as? DragItems.Lifted, let view = lifted.view, presenter?.views[lifted.id] === view else { return nil }
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

    /// Whether a touch at `point` lands in a text input, or a control that
    /// tracks a drag or a held press (a slider, switch, stepper, segmented
    /// control or date picker), of a node inside `node`, which keeps the
    /// contact (D8). Any other control is a press, a button or a checkbox: a
    /// rested finger there lifts the node, as a long press there opens the
    /// node's context menu. The nodes are the presenter's, by identity.
    static func startsInControl(_ node: NodeView, at point: CGPoint, in presenter: Presenter) -> Bool {
        let nodes = Set(presenter.views.values.map(ObjectIdentifier.init))
        var view = node.hitTest(point, with: nil), control = false
        while let v = view, v !== node {
            control = control || v is UITextInput || v is UISlider || v is UISwitch || v is UIStepper || v is UISegmentedControl || v is UIDatePicker
            if control, nodes.contains(ObjectIdentifier(v)) { return true }
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
