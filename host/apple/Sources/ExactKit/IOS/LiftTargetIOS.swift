// Where UIKit grows a menu out of a node and returns it there: a long
// press's context menu (`ContextMenusIOS`) and a custom invoker's pull-down
// (`MenuButton`) take the node itself as their preview, so UIKit hides it
// while its menu shows, as it hides a stock button whose menu it presents.
#if os(iOS)
import UIKit

package enum LiftTarget {
    /// `view`'s outline in its own coordinates where its corners are rounded,
    /// else nil. One radius, round or Apple's continuous, is UIKit's own
    /// rounded rectangle, which UIKit keeps through the lift's morph: UIKit
    /// applies any other path only after the morph's first frames, which show
    /// the node square (measured, iOS 27.1). One radius within the tolerance the
    /// layer's own (`applyBoxLayer`) has.
    package static func rounded(_ view: UIView) -> UIBezierPath? {
        guard let node = view as? NodeView else {
            return view.layer.cornerRadius > 0 ? UIBezierPath(roundedRect: view.bounds, cornerRadius: view.layer.cornerRadius) : nil
        }
        let corners = node.cornerSizes(in: node.bounds), shape = node.style["corner_shape"]
        if let corner = corners.first, corner.width > 0, corners.allSatisfy({ abs($0.width - corner.width) < 0.01 && abs($0.height - corner.width) < 0.01 }),
           shape == nil || CornerShape(shape)?.isAppleContinuous == true {
            return UIBezierPath(roundedRect: node.bounds, cornerRadius: corner.width)
        }
        return corners.contains(where: { $0.width > 0 }) ? node.roundedPath(in: node.bounds) : nil
    }

    /// The view of the controller that holds `view` (its route's, a sheet's, or
    /// the root controller's in an app with no routes), above every box that
    /// clips in it and below the bars. The superview where no controller holds
    /// it; nil outside a window.
    static func container(of view: UIView) -> UIView? {
        guard view.window != nil, let parent = view.superview else { return nil }
        return holder(above: parent)
    }

    /// The view of the controller that holds `parent`, else `parent`
    static func holder(above parent: UIView) -> UIView {
        sequence(first: parent, next: { $0.superview }).first(where: { $0.next is UIViewController }) ?? parent
    }

    /// Where a pull-down's invoker stands as UIKit highlights and returns it,
    /// and where UIKit keeps its stand-in for about a second after the menu has
    /// gone: the invoker's parent, as UIKit's own target for a stock button, so
    /// it moves with its content. A pull-down's preview does not grow (its menu
    /// grows in UIKit's own layer), so a box that clips cuts it only where the
    /// invoker is already cut: then `container(of:)`, above it.
    static func standing(of view: UIView) -> UIView? {
        guard let parent = view.superview, let root = container(of: view) else { return nil }
        for ancestor in sequence(first: parent, next: { $0.superview }) {
            if ancestor === root { break }
            let clips = ancestor.layer.masksToBounds || ancestor.layer.mask != nil
            if clips, !ancestor.bounds.insetBy(dx: -0.5, dy: -0.5).contains(view.convert(view.bounds, to: ancestor)) { return root }
        }
        return parent
    }

    /// Where UIKit highlights and returns a pull-down's invoker: in
    /// `standing(of:)`, centered on the view, with the view's transform, as
    /// UIKit's own target for a view carries it (`UITargetedPreview(view:parameters:)`),
    /// and each ancestor's scale and rotation; converting the center takes the
    /// translations.
    package static func target(_ view: UIView, detached: Bool = false) -> UIPreviewTarget? {
        guard let container = detached ? container(of: view) : standing(of: view), let parent = view.superview else { return nil }
        var transform = view.transform
        for ancestor in sequence(first: parent, next: { $0.superview }) {
            if ancestor === container { break }
            let t = ancestor.transform
            transform = transform.concatenating(CGAffineTransform(a: t.a, b: t.b, c: t.c, d: t.d, tx: 0, ty: 0))
        }
        transform.tx = 0
        transform.ty = 0
        return UIPreviewTarget(container: container, center: view.convert(CGPoint(x: view.bounds.midX, y: view.bounds.midY), to: container), transform: transform)
    }

    /// Where UIKit grows, lifts and returns a long-pressed `view` itself: on
    /// `stage`, which is its parent's coordinate space, as UIKit's own target
    /// for a view in its parent places it (`inParent`).
    static func target(_ view: UIView, on stage: LiftStage) -> UIPreviewTarget? {
        guard stage.holds(view) else { return nil }
        stage.follow()
        return inParent(view, on: stage)
    }

    /// `view` where its parent places it, in the parent's own space (before any
    /// transform of the parent's children), which `container` is or takes: its
    /// middle, which its transform moves from its anchor (a card turned about
    /// its edge), and its transform's scale and rotation, as UIKit's own target
    /// for a view in its parent places it
    private static func inParent(_ view: UIView, on container: UIView) -> UIPreviewTarget {
        let layer = view.layer, b = layer.bounds, a = layer.anchorPoint
        let middle = CGPoint(x: b.midX - (b.minX + a.x * b.width), y: b.midY - (b.minY + a.y * b.height)).applying(view.transform)
        var transform = view.transform
        transform.tx = 0
        transform.ty = 0
        return UIPreviewTarget(container: container, center: CGPoint(x: layer.position.x + middle.x, y: layer.position.y + middle.y), transform: transform)
    }

    /// `view` as its own preview, clipped to its corners, over nothing (a
    /// glass face is see-through); nil (UIKit's default) once it has left
    /// the window or is hidden.
    static func preview(of view: UIView) -> UITargetedPreview? {
        guard !view.isHidden, let target = target(view) else { return nil }
        let parameters = UIPreviewParameters()
        parameters.visiblePath = rounded(view)
        parameters.backgroundColor = .clear
        return UITargetedPreview(view: view, parameters: parameters, target: target)
    }
}

/// Where UIKit grows, lifts and returns a long-pressed node's own preview: a
/// view of the host's over the view of the controller that holds the node
/// (`LiftTarget.container(of:)`), above every box that clips, so that none
/// cuts the preview, its corners or its shadow as UIKit grows it (a strip as
/// tall as its cards would cut them). While it holds what UIKit draws there,
/// it takes the node's parent's coordinate space each frame (its bounds, its
/// place, and its ancestors' scale, rotation, opacity and visibility, as they show), so the preview
/// moves with its content as a parent would carry it: a press that turns into
/// a scroll returns the node where the scroll has taken it. It goes once UIKit
/// has taken back what it drew in it, unless the host holds it for a menu that
/// showed (`held`), or when the host lets it go while it holds nothing
/// (`release`): UIKit's return animation may outlast the presentation that
/// asked for it, and a press let go before its menu shows ends no presentation
/// at all.
final class LiftStage: UIView {
    private weak var parent: UIView?
    private weak var node: UIView?
    private var link: CADisplayLink?
    /// The host holds it for a menu that showed, until the presentation ends (`release`): UIKit may take what it drew
    /// here into a layer of its own while the menu shows and return the node here after
    var held = false

    init?(for view: UIView) {
        guard let parent = view.superview, let container = LiftTarget.container(of: view) else { return nil }
        self.parent = parent
        node = view
        super.init(frame: .zero)
        isUserInteractionEnabled = false
        container.addSubview(self)
        follow()
    }

    required init?(coder: NSCoder) { nil }

    /// Whether this stage is where `view` lifts now: its parent's, over the controller's view that holds it, in a window
    func holds(_ view: UIView) -> Bool {
        guard let parent = view.superview, parent === self.parent, window != nil else { return false }
        return superview === LiftTarget.container(of: view)
    }

    /// Whether a box that clips, between `view`'s parent and its controller's view, would cut `view` as UIKit grows it:
    /// by about 6% (filmed, iOS 27.1), with a shadow past that, for which `growth` leaves room. Only then does `view`
    /// lift on a stage: elsewhere it lifts in its parent, which moves, fades and clips it as it is (`LiftTarget.target`).
    static func cut(_ view: UIView) -> Bool {
        guard let parent = view.superview, let container = LiftTarget.container(of: view) else { return false }
        let grown = view.bounds.insetBy(dx: -growth, dy: -growth)
        return sequence(first: parent, next: { $0.superview }).prefix { $0 !== container }
            .contains { $0.layer.masksToBounds && !$0.bounds.contains(view.convert(grown, to: $0)) }
    }
    /// The room UIKit's lift takes past a view's bounds as it grows it and casts its shadow
    static let growth: CGFloat = 20

    /// Whether a stage can take `view`'s parent's coordinate space and place `view` in it: `view` and each box from
    /// its parent up to the controller's view map a plane to a plane (no 3D transform, no perspective over its
    /// children), and the parent draws its children in its own space (no transform of its sublayers, which a stage of
    /// its space would leave out)
    static func carries(_ view: UIView) -> Bool {
        guard let parent = view.superview, let container = LiftTarget.container(of: view) else { return false }
        return planar(from: parent, to: container) && [view.layer, view.layer.presentation()].compactMap { $0 }.allSatisfy(upright)
    }

    /// Whether a stage can take the space from `parent` up to `container`: each box maps a plane to a plane, and
    /// neither `parent` nor `container` transforms its children (the stage, `container`'s child, would take that
    /// transform again), as laid out and as it shows. `strictly`, as a lift begins, each box also stands upright
    /// (scale and translation only) and clips, if at all, to its square bounds, with no mask or rounded corner, so
    /// that what the clips leave of the node is a rectangle in each clip's space; a lift under way only hides once
    /// the clips leave the node's rectangle nothing, which a later turn or rounding can only overstate.
    private static func planar(from parent: UIView, to container: UIView, strictly: Bool = true) -> Bool {
        let layers = { (view: UIView) in [view.layer, view.layer.presentation()].compactMap { $0 } }
        return (layers(parent) + layers(container)).allSatisfy { CATransform3DIsIdentity($0.sublayerTransform) }
            && sequence(first: parent, next: { $0.superview }).prefix { $0 !== container }.allSatisfy { view in
                layers(view).allSatisfy { CATransform3DIsAffine($0.transform) && CATransform3DIsAffine($0.sublayerTransform) }
                    && (!strictly || view.layer.mask == nil && (!view.layer.masksToBounds || view.layer.cornerRadius == 0)
                        && layers(view).allSatisfy { upright($0) && uprightChildren($0) })
            }
    }

    /// Whether `layer`'s transform of its children turns none of them
    private static func uprightChildren(_ layer: CALayer) -> Bool {
        let t = layer.sublayerTransform
        return CATransform3DIsAffine(t) && abs(t.m12) < 1e-6 && abs(t.m21) < 1e-6
    }

    /// Whether `layer` is turned by no 3D transform, rotation or skew: scaled and moved only
    private static func upright(_ layer: CALayer) -> Bool {
        let t = layer.transform
        return CATransform3DIsAffine(t) && abs(t.m12) < 1e-6 && abs(t.m21) < 1e-6
    }

    /// The parent's coordinate space, where the stage's superview shows it now: as an ancestor's running animation
    /// shows it, else as laid out (a change not yet drawn is not on the screen's copy)
    func follow() {
        guard let container = superview, let window else { return }
        // A parent freed since has left the window too
        guard let parent else {
            if !isHidden { isHidden = true }
            return
        }
        let boxes = Array(sequence(first: parent, next: { $0.superview }).prefix { $0 !== container })
        let animating = (boxes + [node].compactMap { $0 }).contains { $0.layer.animationKeys()?.isEmpty == false }
        let shown = { (layer: CALayer) in animating ? layer.presentation() ?? layer : layer }
        let into = shown(container.layer)
        let cut: Bool = {
            guard let node else { return false }
            // What the clips leave of it together, each narrowing in its own space what the last left
            var space = shown(node.layer), left = space.bounds
            for box in boxes where box.layer.masksToBounds {
                let clip = shown(box.layer)
                left = clip.convert(left, from: space).intersection(clip.bounds)
                space = clip
                if left.isNull || left.isEmpty { return true }
            }
            return false
        }()
        // The boxes it stands above hide and fade it as they hide and fade the node, and the clips among them hide it
        // once they leave none of the node (scrolled out of its strip); so does the parent leaving the window or its
        // controller's view, or taking on a projection no stage can follow
        let hidden = parent.window !== window || LiftTarget.holder(above: parent) !== container || boxes.contains { shown($0.layer).isHidden }
            || !Self.planar(from: parent, to: container, strictly: false) || cut
        if isHidden != hidden { isHidden = hidden }
        guard !hidden else { return }
        let opacity = boxes.reduce(Float(1)) { $0 * shown($1.layer).opacity }
        if layer.opacity != opacity { layer.opacity = opacity }
        let from = shown(parent.layer)
        let origin = into.convert(CGPoint.zero, from: from)
        let x = into.convert(CGPoint(x: 1, y: 0), from: from), y = into.convert(CGPoint(x: 0, y: 1), from: from)
        let linear = CGAffineTransform(a: x.x - origin.x, b: x.y - origin.y, c: y.x - origin.x, d: y.y - origin.y, tx: 0, ty: 0)
        if transform != linear { transform = linear }
        if bounds != from.bounds { bounds = from.bounds }
        center = into.convert(CGPoint(x: from.bounds.midX, y: from.bounds.midY), from: from)
    }

    /// The host is done with it: it goes now if UIKit holds nothing in it, else once UIKit has taken that back;
    /// whether it went now
    @discardableResult
    func release() -> Bool {
        held = false
        guard subviews.isEmpty else { return false }
        end()
        return true
    }

    func end() {
        link?.invalidate()
        link = nil
        removeFromSuperview()
    }

    override func didAddSubview(_ subview: UIView) {
        super.didAddSubview(subview)
        guard link == nil else { return }
        let link = CADisplayLink(target: Tick(self), selector: #selector(Tick.fire))
        link.add(to: .main, forMode: .common)
        self.link = link
    }

    override func willRemoveSubview(_ subview: UIView) {
        super.willRemoveSubview(subview)
        // Not as it is torn down (`end`, then freed with UIKit's view still in it): it is out of the view tree then
        guard subviews.count == 1, superview != nil else { return }
        // Once the turn is over: UIKit may put another view in as it takes this one out. Empty, it follows nothing,
        // and goes unless the host holds it.
        DispatchQueue.main.async { [weak self] in
            guard let self, subviews.isEmpty else { return }
            link?.invalidate()
            link = nil
            if !held { end() }
        }
    }

    /// The display link's target, which holds the stage weakly: the link holds its target
    private final class Tick: NSObject {
        weak var stage: LiftStage?
        init(_ stage: LiftStage) { self.stage = stage }
        @objc func fire(_ link: CADisplayLink) {
            if let stage { stage.follow() } else { link.invalidate() }
        }
    }
}
#endif
