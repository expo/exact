// A layout-only box still covers earlier siblings for input (LLP 1171 D4).
// Keep that region in its parent's child order without a view or a layer.
#if os(iOS) || os(tvOS)
import UIKit

extension Presenter {
    func promoteLayoutBox(_ id: UInt32, _ op: BatchOp) -> [UInt32] {
        guard let parent = op.layoutParent.flatMap({ views[$0] }),
              let box = views[id], let rect = op.layoutFrame else { return [] }
        var token: LayoutHit?
        var above: UIView? = parent
        while let view = above {
            if let hit = view.layoutTokens.first(where: { $0.id == id }) { token = hit; break }
            above = view.superview
        }
        parent.container.addSubview(box)
        token?.promoted = HeldNode(box)
        if let token { token.reserve(above?.layoutTokens ?? []) }
        box.promotedLayoutHit = token
        var moved = [id]
        // Restore the previous geometry even when new layout is withheld.
        applyFrame(box, rect)
        let keep = Set(op.layoutKeep ?? [])
        for record in op.layoutChildren ?? [] {
            let id = record.id, rect = record.frame
            guard let child = views[id] else { flats.frame(id, rect); continue }
            moved.append(id)
            let move: () -> Void = {
                box.container.addSubview(child)
                self.applyFrame(child, rect)
            }
            // The logical ancestry and the captured responder hops stay the same.
            if keep.contains(id) { GlassGroups.moving(in: child, move) } else { move() }
        }
        return moved
    }
}

private var layoutHitsKey: UInt8 = 0
private var promotedLayoutHitKey: UInt8 = 0
extension UIView {
    var promotedLayoutHit: LayoutHit? {
        get { objc_getAssociatedObject(self, &promotedLayoutHitKey) as? LayoutHit }
        set { objc_setAssociatedObject(self, &promotedLayoutHitKey, newValue, .OBJC_ASSOCIATION_RETAIN_NONATOMIC) }
    }
    var layoutTokens: [LayoutHit] {
        (layoutHits?.boxes.map(\.2) ?? []) + (promotedLayoutHit?.borrowed ?? [])
    }
    var layoutHits: LayoutHits? {
        get { objc_getAssociatedObject(self, &layoutHitsKey) as? LayoutHits }
        set {
            let old = layoutHits
            objc_setAssociatedObject(self, &layoutHitsKey, newValue, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
            for (_, _, token) in old?.boxes ?? [] where newValue?.boxes.contains(where: { $0.2 === token }) != true {
                token.retire(from: old?.owner)
            }
        }
    }
}

/// A contact keeps the omitted node's lifetime, including promotion to a view.
final class LayoutHit {
    let id: UInt32
    private var owner: HeldNode
    private var active = true
    private var transferring = false
    private(set) var borrowed: [LayoutHit] = []
    var promoted: HeldNode?
    private var cancelContact: (() -> Void)?
    weak var feedback: RecognizedPress?

    init(_ id: UInt32, owner: NodeView) { self.id = id; self.owner = HeldNode(owner) }
    var isLive: Bool {
        guard let presenter = owner.node?.presenter else { return false }
        if active, owner.live(in: presenter) != nil { return true }
        return promoted?.live(in: presenter).map { !$0.disabled && !$0.inert } ?? false
    }
    // A promoted ancestor takes over hit regions that later ops may publish in either order.
    func reserve(_ tokens: [LayoutHit]) {
        borrowed = tokens.filter { $0.promoted == nil }
        for token in borrowed { token.transferring = true }
    }
    func adopt(_ node: NodeView) {
        owner = HeldNode(node)
        active = true
        transferring = false
    }
    func finishTransfers() {
        for token in borrowed where token.transferring {
            token.transferring = false
            token.active = false
            if !token.isLive { token.cancel() }
        }
        borrowed.removeAll()
    }
    func capture(_ contact: PressContact, at node: NodeView) {
        cancelContact = { [weak contact, weak node] in
            if let contact, node?.pressContact === contact { node?.cancelPressContact() }
        }
    }
    func cancel() {
        transferring = false
        active = false
        promoted = nil
        cancelContact?()
        feedback?.cancel()
    }
    func retire(from node: NodeView?) {
        guard !transferring, let node, owner.node === node else { return }
        active = false
        if !isLive { cancel() }
    }
}

final class LayoutHits {
    static func at(_ holder: UIView) -> LayoutHits? {
        var at: UIView? = holder
        while let view = at {
            if let node = view as? NodeView { return node.container === holder ? node.layoutHits : nil }
            at = view.superview
        }
        return nil
    }

    static func touched(_ hit: UIView, at point: CGPoint) -> LayoutHit? {
        guard let node = hit as? NodeView, node.layoutHits != nil else { return nil }
        var token: LayoutHit?
        _ = NodeView.hitChildren(in: node.container, at: node.container.convert(point, from: nil), with: nil,
                                 captureLayoutHit: { token = $0 })
        return token
    }

    weak var owner: NodeView?
    let boxes: [(UInt32?, CGRect, LayoutHit)]
    /// The presented child after each box's descendants (nil: none).
    let ends: [UInt32?]
    private init(_ owner: NodeView, boxes: [(UInt32?, CGRect, LayoutHit)], ends: [UInt32?]) {
        self.owner = owner
        self.boxes = boxes
        self.ends = ends
    }

    /// These regions without the boxes `ids` names; nil when none is left.
    func without(_ ids: Set<UInt32>) -> LayoutHits? {
        guard let owner else { return nil }
        let kept = zip(boxes, ends).filter { !ids.contains($0.0.2.id) }
        return kept.isEmpty ? nil : LayoutHits(owner, boxes: kept.map(\.0), ends: kept.map(\.1))
    }

    init?(_ owner: NodeView, _ boxes: [LayoutBoxRecord]?) {
        guard let boxes else { return nil }
        self.owner = owner
        let previous = Dictionary(owner.layoutTokens.map { ($0.id, $0) }, uniquingKeysWith: { old, _ in old })
        var ends: [UInt32?] = []
        self.boxes = boxes.map { box in
            let token = previous[box.id] ?? LayoutHit(box.id, owner: owner)
            token.adopt(owner)
            ends.append(box.end)
            return (box.before, box.frame, token)
        }
        self.ends = ends
    }

    /// The presented children a box's descendants start or end before. Alike
    /// leaves merge into one layer only between two of these, so each layer
    /// lies inside or outside every box.
    var bounds: Set<UInt32> { Set(boxes.compactMap(\.0) + ends.compactMap { $0 }) }

    /// The layer slot just before a box's first descendant, or the next sibling
    /// for an empty box. A rank-zero box stays below positive ranks and above negatives.
    func hit(in holder: UIView, at point: CGPoint, layers: [CALayer], visit: (CALayer) -> UIView?, capture: ((LayoutHit) -> Void)? = nil) -> UIView? {
        guard let owner else { return nil }
        var entries: [(z: CGFloat, slot: Int, order: Int, layer: CALayer?, box: CGRect?, token: LayoutHit?)] = layers.enumerated().map {
            ($0.element.zPosition, $0.offset * 2, 0, $0.element, nil, nil)
        }
        let origin = owner.grid?.origin ?? .zero
        // A merged run owns the layers of all its logical leaves.
        var anchors: [UInt32: CALayer] = [:]
        if let presenter = owner.presenter {
            for entry in presenter.flats.inspectionLayers(under: owner.id) {
                for id in entry.leaves { anchors[id] = entry.layer }
            }
        }
        func slot(_ id: UInt32?) -> Int {
            let layer = id.flatMap { owner.presenter?.views[$0]?.layer ?? anchors[$0] }
            return (layer.flatMap { anchor in layers.firstIndex { $0 === anchor } } ?? layers.count) * 2 - 1
        }
        // A box's descendants' layers lie strictly between its two slots.
        var spans: [(start: Int, end: Int, order: Int, token: LayoutHit)] = []
        for (order, box) in boxes.enumerated() {
            let (before, rect, token) = box
            let scale = owner.presenter?.gridScale(under: origin) ?? holder.traitCollection.displayScale
            let frame = PixelGrid.place(rect, in: origin, scale: scale).drawn
            let start = slot(before)
            entries.append((0, start, order, nil, frame, token))
            spans.append((start, order < ends.count ? slot(ends[order]) : start, order, token))
        }
        entries.sort {
            if $0.z != $1.z { return $0.z > $1.z }
            return $0.slot != $1.slot ? $0.slot > $1.slot : $0.order > $1.order
        }
        for entry in entries {
            if let layer = entry.layer, let hit = visit(layer) {
                // A leaf painted in the owner's layers holds the innermost omitted
                // box around it, so removing that box ends the contact. The leaf
                // can overflow that box's frame: its slot alone says which box
                // holds it. Boxes are in tree order, so of two spanning the same
                // layers the later is inside the earlier.
                if hit === owner, let capture,
                   let span = spans.filter({ $0.start < entry.slot && entry.slot < $0.end })
                    .max(by: { ($0.start, $1.end, $0.order) < ($1.start, $0.end, $1.order) }) {
                    capture(span.token)
                }
                return hit
            }
            if let box = entry.box, box.contains(point), !owner.inert,
               owner.style["pointer_events"]?.string != "none", !owner.cssVisibilityHidden {
                if let token = entry.token { capture?(token) }
                return owner
            }
        }
        return nil
    }
}
#endif
