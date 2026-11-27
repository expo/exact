#if os(iOS)
import ExactKit
import UIKit

/// One text input over prose that shares a selectable container. Its surface
/// paints over the container's children with pass-through hit testing so
/// UIKit's translucent highlight and handles appear above child backgrounds,
/// while paragraph touches route to it.
final class SelectionDocument {
    struct Part {
        let node: NodeView, input: SelectableText, start: Int, length: Int
        var range: NSRange { NSRange(location: start, length: length) }
    }
    private struct ClipFingerprint: Equatable {
        let frame: CGRect
        let clips: [CGRect]
        let paths: [CGPath]
        let radii: [CGFloat]
    }
    weak var owner: NodeView?
    let surface = SelectionSurface()
    let parts: [Part]
    let text: NSString
    private var reported: [UInt32: NSRange] = [:]
    private var geometry: [ClipFingerprint] = []

    /// Walk `start` upward through `superview`s, stopping before `stop`.
    private static func ancestors(from start: UIView?, upTo stop: UIView? = nil) -> [UIView] {
        var result: [UIView] = []
        var at = start
        while let view = at, view !== stop {
            result.append(view)
            at = view.superview
        }
        return result
    }

    /// The shared selection container of `node`, and whether `node` can route
    /// touches to it without crossing a nested scroll view or pressable ancestor.
    private static func containerInfo(of node: NodeView) -> (owner: NodeView, sharesSurface: Bool)? {
        let selectAll = node.selectAllElement.flatMap { $0 === node ? nil : $0 }
        var crossedBoundary = false
        for view in ancestors(from: node.superview) {
            guard let parent = view as? NodeView else { continue }
            if let selectAll {
                if parent === selectAll { return (selectAll, !crossedBoundary) }
                if parent.scrollView != nil || parent.pressable { crossedBoundary = true }
            } else {
                if ["text", "all"].contains(parent.style["user_select"]?.string ?? "") {
                    return (parent, !crossedBoundary)
                }
                if parent.scrollView != nil || parent.pressable { return nil }
            }
        }
        return selectAll.map { ($0, false) }
    }
    static func container(of node: NodeView) -> NodeView? { containerInfo(of: node)?.owner }
    /// Whether `node` can route touches to `owner`'s shared surface without
    /// crossing an intervening `UIScrollView` or pressable ancestor.
    static func sharesTouchSurface(_ node: NodeView, with owner: NodeView) -> Bool {
        guard let info = containerInfo(of: node) else { return false }
        return info.owner === owner && info.sharesSurface
    }
    static func paragraphs(in owner: NodeView, host: TextSelectionHost) -> [NodeView] {
        var result: [NodeView] = []
        func walk(_ view: UIView) {
            guard !view.isHidden else { return }
            if let node = view as? NodeView, node.isParagraph {
                if host.admits(node) { result.append(node) }
                return
            }
            for child in view.subviews where !(child is SelectionSurface) { walk(child) }
        }
        walk(owner)
        return result
    }
    init(owner: NodeView, paragraphs: [NodeView], host: TextSelectionHost) {
        self.owner = owner
        var parts: [Part] = [], strings: [String] = [], start = 0
        for node in paragraphs {
            let text = SelectableText.shaped(node)
            parts.append(Part(node: node, input: SelectableText(node: node, host: host, attach: false), start: start, length: text.length))
            strings.append(text as String)
            start += text.length + 2
        }
        self.parts = parts
        text = strings.joined(separator: "\n\n") as NSString
        surface.autoresizingMask = owner.container is UIScrollView ? [] : [.flexibleWidth, .flexibleHeight]
        surface.isAccessibilityElement = false
        owner.container.addSubview(surface)
        syncFrame()
        surface.setPaintForeground()
        surface.observePointers()
        geometry = fingerprint()
    }
    /// Match the owner's container extent. A scroll container's `bounds` moves
    /// with `contentOffset` and only covers the viewport, so size the surface
    /// to cover the full scroll content where selection handles can live.
    func syncFrame() {
        guard let owner else { return }
        let container = owner.container
        let size: CGSize
        if let scroll = container as? UIScrollView {
            size = CGSize(width: max(scroll.bounds.width, scroll.contentSize.width),
                          height: max(scroll.bounds.height, scroll.contentSize.height))
        } else {
            size = container.bounds.size
        }
        let target = CGRect(origin: .zero, size: size)
        if surface.frame != target { surface.frame = target }
    }
    private func fingerprint() -> [ClipFingerprint] {
        let container = owner?.container
        return parts.map { part in
            var clips: [CGRect] = [], paths: [CGPath] = [], radii: [CGFloat] = []
            for view in Self.ancestors(from: part.node, upTo: container) {
                if let clip = nonrectangularClip(of: view) { paths.append(clip.path) }
                if view.clipsToBounds {
                    clips.append(view.convert(view.bounds, to: surface))
                    if view.layer.cornerRadius > 0 { radii.append(view.layer.cornerRadius) }
                }
            }
            return ClipFingerprint(frame: part.node.convert(part.node.bounds, to: surface), clips: clips, paths: paths, radii: radii)
        }
    }
    func geometryChanged() -> Bool {
        syncFrame()
        let now = fingerprint()
        guard now != geometry else { return false }
        geometry = now
        return true
    }
    func contains(_ node: NodeView) -> Bool { parts.contains { $0.node === node } }
    func part(for view: UIView) -> Part? { parts.first { $0.node === view } }
    func selects(_ view: UIView, in range: NSRange) -> Bool {
        range.length > 0 && parts.contains { $0.node === view && NSIntersectionRange(range, $0.range).length > 0 }
    }
    func settle(host: TextSelectionHost) -> Bool {
        guard let owner else { return false }
        syncFrame()
        let now = Self.paragraphs(in: owner, host: host)
        return now.count == parts.count && zip(now, parts).allSatisfy { node, part in
            node === part.node && SelectableText.shaped(node).length == part.length
                && SelectableText.shaped(node) == part.input.shapedText
        }
    }
    func part(at offset: Int) -> Part? { parts.last { $0.start <= offset } ?? parts.first }
    func paragraph(at point: CGPoint) -> NodeView? {
        parts.first { clipRect($0.node.convert($0.node.bounds, to: surface), from: $0.node)?.contains(point) == true }?.node
    }
    func index(at point: CGPoint) -> Int {
        guard let part = parts.min(by: { distance(point, to: $0.node) < distance(point, to: $1.node) }) else { return 0 }
        return part.start + part.input.index(at: part.node.convert(point, from: surface))
    }
    private func distance(_ point: CGPoint, to node: NodeView) -> CGFloat {
        guard let box = clipRect(node.convert(node.bounds, to: surface), from: node) else { return .greatestFiniteMagnitude }
        let x = max(box.minX - point.x, 0, point.x - box.maxX), y = max(box.minY - point.y, 0, point.y - box.maxY)
        return x * x + y * y
    }
    /// Affine transform mapping points from `view`'s bounds to `surface`.
    private func transformToSurface(from view: UIView) -> CGAffineTransform {
        let o = view.convert(CGPoint.zero, to: surface)
        let x = view.convert(CGPoint(x: 1, y: 0), to: surface)
        let y = view.convert(CGPoint(x: 0, y: 1), to: surface)
        return CGAffineTransform(a: x.x - o.x, b: x.y - o.y, c: y.x - o.x, d: y.y - o.y, tx: o.x, ty: o.y)
    }
    /// The nonrectangular clip path of `view` in `surface` coordinates (if any),
    /// along with `view`'s bounds in `surface` coordinates.
    private func nonrectangularClip(of view: UIView) -> (bounds: CGRect, path: CGPath)? {
        var raw: CGPath?
        var offset = CGPoint.zero
        if let clipped = view as? ClipPathProviding, let path = clipped.presentedClipPath {
            raw = path
        } else if let mask = view.layer.mask as? CAShapeLayer, let path = mask.path {
            raw = path
            offset = mask.frame.origin
        } else if view.clipsToBounds, view.layer.cornerRadius > 0, !view.bounds.isEmpty {
            let r = min(view.layer.cornerRadius, min(view.bounds.width, view.bounds.height) / 2)
            raw = CGPath(roundedRect: view.bounds, cornerWidth: r, cornerHeight: r, transform: nil)
        }
        guard let raw else { return nil }
        var tx = CGAffineTransform(translationX: offset.x, y: offset.y).concatenating(transformToSurface(from: view))
        guard let mapped = raw.copy(using: &tx) else { return nil }
        return (view.convert(view.bounds, to: surface), mapped)
    }
    /// Mask `surface` to subtract the clipped-out regions of any nonrectangular
    /// clipping ancestor strictly between a selected paragraph and `owner.container`,
    /// keeping unclipped siblings and selection handles outside those boxes intact.
    func syncSurfaceMask(for range: NSRange) {
        guard range.length > 0, let container = owner?.container else {
            if surface.layer.mask != nil { surface.layer.mask = nil }
            return
        }
        var seen = Set<ObjectIdentifier>()
        var clips: [(bounds: CGRect, path: CGPath)] = []
        for part in parts where NSIntersectionRange(range, part.range).length > 0 {
            for view in Self.ancestors(from: part.node, upTo: container) {
                guard seen.insert(ObjectIdentifier(view)).inserted,
                      let clip = nonrectangularClip(of: view) else { continue }
                clips.append(clip)
            }
        }
        guard !clips.isEmpty else {
            if surface.layer.mask != nil { surface.layer.mask = nil }
            return
        }
        var visible: CGPath = CGPath(rect: surface.bounds.insetBy(dx: -64, dy: -64), transform: nil)
        for clip in clips {
            let cutout = CGPath(rect: clip.bounds, transform: nil).subtracting(clip.path)
            if !cutout.isEmpty { visible = visible.subtracting(cutout) }
        }
        let layer = (surface.layer.mask as? CAShapeLayer) ?? CAShapeLayer()
        layer.frame = surface.bounds
        layer.path = visible
        if surface.layer.mask !== layer { surface.layer.mask = layer }
    }
    /// Clip a highlight or endpoint rectangle in `surface` coordinates against
    /// every clipping ancestor (scrollport, overflow clip, rounded corner, or
    /// `clip-path`) between `node` and `owner.container`.
    private func clipRect(_ rect: CGRect, from node: NodeView, allowZeroWidth: Bool = false) -> CGRect? {
        guard let container = owner?.container else { return rect }
        var box = rect
        for view in Self.ancestors(from: node, upTo: container) {
            if view.clipsToBounds {
                box = box.intersection(view.convert(view.bounds, to: surface))
                if box.isNull || box.height <= 0 || (!allowZeroWidth && box.width <= 0) { return nil }
            }
            if let clip = nonrectangularClip(of: view) {
                let probe = box.width > 0 ? box : CGRect(x: box.minX - 0.5, y: box.minY, width: 1, height: box.height)
                let hit = CGPath(rect: probe, transform: nil).intersection(clip.path)
                guard !hit.isEmpty else { return nil }
                let tight = hit.boundingBoxOfPath
                box = allowZeroWidth && box.width == 0
                    ? CGRect(x: box.minX, y: tight.minY, width: 0, height: tight.height)
                    : box.intersection(tight)
                if box.isNull || box.height <= 0 || (!allowZeroWidth && box.width <= 0) { return nil }
            }
        }
        return box
    }
    func rects(_ range: NSRange) -> [UITextSelectionRect] {
        guard range.length > 0 else { return [] }
        var boxes: [UITextSelectionRect] = []
        for part in parts {
            let selected = NSIntersectionRange(range, part.range)
            guard selected.length > 0 else { continue }
            let local = NSRange(location: selected.location - part.start, length: selected.length)
            for slice in part.input.selectableSlices(in: local) {
                let docStart = part.start + slice.location
                let docEnd = part.start + NSMaxRange(slice)
                for case let rect as SelectableRect in part.input.selectionRects(for: SelectableRange(slice)) {
                    let mapped = part.node.convert(rect.rect, to: surface)
                    guard let clipped = clipRect(mapped, from: part.node, allowZeroWidth: rect.rect.width == 0) else { continue }
                    boxes.append(rect.converted(clipped, first: docStart == range.location, last: docEnd == NSMaxRange(range)))
                }
            }
        }
        // The separator between paragraphs has document offsets but no glyph run.
        for (offset, first, present) in [
            (range.location, true, boxes.contains { $0.containsStart }),
            (NSMaxRange(range), false, boxes.contains { $0.containsEnd }),
        ] {
            guard !present, let part = part(at: offset) else { continue }
            let local = part.input.endpoint(at: min(part.length, offset - part.start), first: first)
            let mapped = part.node.convert(local.rect, to: surface)
            guard let clipped = clipRect(mapped, from: part.node, allowZeroWidth: true) else { continue }
            let rect = local.converted(clipped)
            if first { boxes.insert(rect, at: 0) } else { boxes.append(rect) }
        }
        return boxes
    }
    func caret(at offset: Int) -> CGRect {
        guard let part = part(at: offset) else { return .zero }
        let raw = part.node.convert(part.input.caretRect(for: SelectablePosition(min(part.length, offset - part.start))), to: surface)
        return clipRect(raw, from: part.node, allowZeroWidth: true) ?? .zero
    }
    func atomic(_ range: NSRange) -> NSRange {
        guard range.length > 0 else { return range }
        var lo = Int.max, hi = Int.min
        for part in parts {
            let overlap = NSIntersectionRange(range, part.range)
            guard overlap.length > 0 else { continue }
            if part.node.textSelectable, let outer = part.node.selectAllElement {
                let members = parts.filter { $0.node === outer || $0.node.isDescendant(of: outer) }
                if let first = members.first, let last = members.last {
                    lo = min(lo, first.start); hi = max(hi, NSMaxRange(last.range))
                }
                continue
            }
            let whole = part.input.atomic(NSRange(location: overlap.location - part.start, length: overlap.length))
            if whole.length > 0 { lo = min(lo, part.start + whole.location); hi = max(hi, part.start + NSMaxRange(whole)) }
        }
        return hi > lo ? NSRange(location: lo, length: hi - lo) : NSRange(location: 0, length: 0)
    }
    func report(_ range: NSRange, host: TextSelectionHost) {
        for part in parts where part.node.handlers.contains("selectionchange") {
            let selected = NSIntersectionRange(range, part.range)
            let localShaped = selected.length > 0
                ? part.input.atomic(NSRange(location: selected.location - part.start, length: selected.length))
                : NSRange(location: 0, length: 0)
            let found = SelectableText.sourceSelection(in: part.node, shaped: localShaped)
            let local = found?.range ?? NSRange(location: 0, length: 0)
            guard reported[part.node.id] ?? NSRange(location: 0, length: 0) != local else { continue }
            reported[part.node.id] = local
            host.report(part.node, found?.text ?? "", local)
        }
    }
}

final class SelectionSurface: UIView, SharedSelectionSurface {
    weak var input: SelectableText?
    private weak var touched: NodeView?
    var owner: NodeView? { input?.document?.owner }
    func paragraph(under touch: UITouch) -> NodeView? { input?.document?.paragraph(at: touch.location(in: self)) }
    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        let hit = super.hitTest(point, with: event)
        return hit === self ? nil : hit
    }
    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard input?.selected.length ?? 0 == 0 else { touched = nil; return }
        touched = touches.first.flatMap { input?.document?.paragraph(at: $0.location(in: self)) }
        touched?.touchesBegan(touches, with: event)
    }
    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) { touched?.touchesMoved(touches, with: event) }
    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        if input?.selected.length ?? 0 > 0 { touched?.touchesCancelled(touches, with: event) }
        else { touched?.touchesEnded(touches, with: event) }
        touched = nil
    }
    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) { touched?.touchesCancelled(touches, with: event); touched = nil }
    override func canPerformAction(_ action: Selector, withSender sender: Any?) -> Bool {
        if action == #selector(copy(_:)) { return input?.selected.length ?? 0 > 0 }
        if action == #selector(selectAll(_:)) { return input?.canSelectAll == true }
        return super.canPerformAction(action, withSender: sender)
    }
    override func copy(_ sender: Any?) {
        if owner?.dispatchCopy() == true { return }
        if let input { input.copyDocument() }
    }
    override func selectAll(_ sender: Any?) { _ = input?.selectAllDocument() }
}
#endif
