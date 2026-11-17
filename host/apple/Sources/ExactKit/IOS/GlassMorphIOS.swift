// A glass that joins or leaves a glass group moves as UIKit's own glass
// does in a `UIGlassContainerEffect` (LLP 1053.000.000.001): a glass that a
// person's input adds grows out of the nearest glass of its group within
// the group's spacing, a bead at that glass's edge that swells to its own
// box, bridged to it while they are close, and splits off; one that input
// removes is drawn back into that neighbour, its effect set to nil in the
// same animation so it dematerializes as it goes. With no neighbour within
// the spacing a joining glass materializes in place and a leaving one
// dematerializes in place. The motion is UIKit's spring on the render
// server, as a bar's items move when UIKit replaces them.
//
// A glass that arrives with its group (a screen shown), or that data or a
// timer adds or removes, is drawn at once, as UIKit draws glass set outside
// an animation; so is every one under Reduce Motion, under the agent's
// frozen clock, and one under an `animation` its author wrote on it or on a
// box between it and its group (the author says how it moves). A glass its
// path isolates (D4) merges with nothing, so it only materializes. The
// glass leaving is a paint-only copy in the group's container: the node and
// its views are gone from the tree, hit-testing and accessibility at once.
#if os(iOS) || os(tvOS)
import UIKit

/// A glass a batch removed, as it was drawn in its group.
struct GlassDeparture {
    weak var group: GlassGroupView?
    /// Its box in the group's content view.
    let frame: CGRect
    let effect: UIVisualEffect
    /// Its corner radius, as its material is drawn (`applyMaterialRadius`).
    let corners: CGFloat
    /// What it showed on its glass, as a picture.
    let content: UIView?
}

extension GlassGroups {
    /// The batch's joins and departures, from the boxes it laid out.
    func morph() {
        let joins = joining.allObjects, leaves = departed
        joining.removeAllObjects(); departed = []
        guard #available(iOS 26.0, tvOS 26.0, *) else { return }
        // A glass grows out of one that was there before this batch, and
        // every start is read before any moves.
        let staying = glass.allObjects.filter { node in !joins.contains { $0 === node } }
        let starts = joins.map { ($0, $0.joinStart(among: staying)) }
        for (node, start) in starts { node.grow(from: start) }
        for leave in leaves { leave.drawIntoNeighbour(among: staying) }
    }

    /// `node` and the glass under it are leaving: each one grouped is kept as
    /// it is drawn now, before its views go, and drawn into a neighbour at
    /// the batch's end. Each is captured once, whichever of its ancestors'
    /// destroys comes first.
    func depart(_ node: NodeView) {
        guard groups.count > 0, #available(iOS 26.0, tvOS 26.0, *), node.window != nil else { return }
        for member in glass.allObjects where member === node || member.isDescendant(of: node) {
            guard member.window != nil, let capture = member.departure() else { continue }
            glass.remove(member)
            departed.append(capture)
        }
    }

    /// The glass of `group` a box at `frame` (in the group's content view)
    /// would merge with: the nearest within the group's spacing, joined and
    /// shown, other than `except`.
    static func neighbour(of frame: CGRect, in group: GlassGroupView, among glass: [NodeView], except: NodeView? = nil) -> CGRect? {
        var best: (CGRect, CGFloat)?
        for other in glass where other !== except {
            guard other.window === group.window, let body = other.glassBody, !body.isHidden else { continue }
            let path = other.glassPath()
            guard path.container?.glassGroupView === group, path.reasons.isEmpty else { continue }
            let box = body.convert(body.bounds, to: group.contentView)
            let dx = max(0, max(box.minX - frame.maxX, frame.minX - box.maxX))
            let dy = max(0, max(box.minY - frame.maxY, frame.minY - box.maxY))
            let gap = hypot(dx, dy)
            if gap <= max(group.spacing, 0) + 0.5, best.map({ gap < $0.1 }) ?? true { best = (box, gap) }
        }
        return best?.0
    }

    /// The bead a glass at `frame` starts from or ends as: on `neighbour`'s
    /// edge, at the point nearest the glass's centre, half the smaller side
    /// of the two, so half of it is inside the neighbour and merged with it.
    static func bead(for frame: CGRect, at neighbour: CGRect) -> CGRect {
        let centre = CGPoint(x: min(max(frame.midX, neighbour.minX), neighbour.maxX),
                             y: min(max(frame.midY, neighbour.minY), neighbour.maxY))
        let side = max(1, min(frame.width, frame.height, neighbour.width, neighbour.height) / 2)
        return CGRect(x: centre.x - side / 2, y: centre.y - side / 2, width: side, height: side)
    }

    /// UIKit's default spring, its timing as given.
    static func spring(_ animations: @escaping () -> Void, completion: (() -> Void)? = nil) {
        UIView.animate(options: [.allowUserInteraction, .beginFromCurrentState], animations: animations) { _ in completion?() }
    }
}

extension NodeView {
    /// The group a glass moves in, when it may move now: a person's input
    /// started after the group was shown, the glass is in a window, motion
    /// is not reduced or frozen, and no author's `animation` moves it.
    @available(iOS 26.0, tvOS 26.0, *)
    fileprivate func morphingGroup() -> GlassGroupView? {
        guard let started = presenter?.inputStarted, window != nil, !ExactEnv.agentFreezes, !UIAccessibility.isReduceMotionEnabled,
              let group = sequence(first: superview, next: { $0?.superview }).lazy.compactMap({ $0 as? GlassGroupView }).first,
              let since = group.shownSince, since < started else { return nil }
        var below: UIView? = self
        while let v = below, v !== group {
            if let node = v as? NodeView, node.wrote("animation") { return nil }
            below = v.superview
        }
        return group
    }

    /// Where a joining glass starts: nil when it does not move, else its
    /// group and the bead it grows from (nil to materialize in place).
    @available(iOS 26.0, tvOS 26.0, *)
    fileprivate func joinStart(among glass: [NodeView]) -> (group: GlassGroupView, bead: CGRect?)? {
        guard let body = glassBody, let group = morphingGroup() else { return nil }
        let frame = body.convert(body.bounds, to: group.contentView)
        guard glassPath().reasons.isEmpty, let neighbour = GlassGroups.neighbour(of: frame, in: group, among: glass, except: self) else { return (group, nil) }
        return (group, GlassGroups.bead(for: frame, at: neighbour))
    }

    @available(iOS 26.0, tvOS 26.0, *)
    fileprivate func grow(from start: (group: GlassGroupView, bead: CGRect?)?) {
        guard let (group, bead) = start, let body = glassBody else { return }
        if let material = body as? UIVisualEffectView, let effect = material.effect {
            let rest = material.frame
            material.contentView.alpha = 0
            if let bead, let parent = material.superview {
                material.frame = group.contentView.convert(bead, to: parent)
            } else {
                material.effect = nil
            }
            GlassGroups.spring {
                material.frame = rest
                material.effect = effect
                material.contentView.alpha = 1
            }
        } else if let bead, let parent = body.superview {
            // A native glass button's frame is its control host's: it grows by its transform.
            let bead = group.contentView.convert(bead, to: parent)
            let rest = body.transform
            body.transform = CGAffineTransform(translationX: bead.midX - body.center.x, y: bead.midY - body.center.y)
                .scaledBy(x: bead.width / max(body.bounds.width, 1), y: bead.height / max(body.bounds.height, 1))
            GlassGroups.spring { body.transform = rest }
        }
    }

    /// This glass as it is drawn now, to be drawn into a neighbour once the
    /// batch has laid out what stays; nil when it does not move.
    @available(iOS 26.0, tvOS 26.0, *)
    fileprivate func departure() -> GlassDeparture? {
        // A glass not shown (`visibility: hidden` sets its effect to nil) leaves nothing to draw in.
        guard let body = glassBody, !body.isHidden, !cssVisibilityHidden, glassPath().reasons.isEmpty, let group = morphingGroup() else { return nil }
        let frame = body.convert(body.bounds, to: group.contentView)
        guard frame.width > 0, frame.height > 0 else { return nil }
        if let material = body as? UIVisualEffectView {
            guard let effect = material.effect else { return nil }
            let radius = BorderPaint.reduced(effectiveCornerSizes(in: bounds), in: bounds).first?.width ?? 0
            return GlassDeparture(group: group, frame: frame, effect: effect, corners: radius,
                                  content: GlassDeparture.picture(of: material.contentView))
        }
        // A native glass button: its glass, as a capsule, without its title, which goes with it.
        guard body is NativeButtonIOS, body === nativeGlassBody else { return nil }
        return GlassDeparture(group: group, frame: frame, effect: UIGlassEffect(style: .regular), corners: min(frame.width, frame.height) / 2, content: nil)
    }
}

extension GlassDeparture {
    /// What a glass shows on it, without the glass: its content view's
    /// layer tree as it is shown now, its children's views and the flat
    /// leaves mounted as layers in it (`FlatLeaves`) alike. A snapshot of the
    /// content view would hold the glass's own rendering too; drawing the
    /// layers leaves out the effect, which is no layer of the content view's.
    /// `render(in:)` draws each layer's own values, not a running
    /// animation's, so a descendant mid-animation is given its shown values
    /// for the drawing (`shown`) and its own back before the batch commits.
    static func picture(of content: UIView) -> UIView {
        let format = UIGraphicsImageRendererFormat.preferred()
        format.scale = content.traitCollection.displayScale > 0 ? content.traitCollection.displayScale : format.scale
        format.opaque = false
        let image = shown(content.layer) { UIGraphicsImageRenderer(bounds: content.bounds, format: format).image { content.layer.render(in: $0.cgContext) } }
        let picture = UIImageView(image: image)
        picture.frame = content.bounds
        return picture
    }

    /// `draw` run with each animated layer under `root` set to the values it
    /// is shown at, which are put back after, with no action.
    private static func shown<T>(_ root: CALayer, _ draw: () -> T) -> T {
        typealias Own = (layer: CALayer, opacity: Float, position: CGPoint, bounds: CGRect, transform: CATransform3D, background: CGColor?)
        var own: [Own] = [], below = [root]
        while let layer = below.popLast() {
            below += layer.sublayers ?? []
            guard layer.animationKeys()?.isEmpty == false, layer.presentation() != nil else { continue }
            own.append((layer, layer.opacity, layer.position, layer.bounds, layer.transform, layer.backgroundColor))
        }
        guard !own.isEmpty else { return draw() }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }
        for o in own {
            guard let at = o.layer.presentation() else { continue }
            (o.layer.opacity, o.layer.position, o.layer.bounds, o.layer.transform, o.layer.backgroundColor) = (at.opacity, at.position, at.bounds, at.transform, at.backgroundColor)
        }
        defer { for o in own { (o.layer.opacity, o.layer.position, o.layer.bounds, o.layer.transform, o.layer.backgroundColor) = (o.opacity, o.position, o.bounds, o.transform, o.background) } }
        return draw()
    }

    /// A paint-only glass where the departed one was, drawn into the nearest
    /// glass that stays within the group's spacing and dematerialized as it
    /// goes, then removed.
    @available(iOS 26.0, tvOS 26.0, *)
    func drawIntoNeighbour(among glass: [NodeView]) {
        guard let group, group.window != nil else { return }
        let ghost = GlassGhost(effect: effect)
        ghost.frame = frame
        ghost.cornerConfiguration = .corners(radius: .fixed(Double(corners)))
        if let content {
            content.frame = ghost.contentView.bounds
            // Centred as the glass draws in, while it fades.
            content.autoresizingMask = [.flexibleLeftMargin, .flexibleRightMargin, .flexibleTopMargin, .flexibleBottomMargin]
            ghost.contentView.addSubview(content)
        }
        group.contentView.insertSubview(ghost, at: 0)
        let neighbour = GlassGroups.neighbour(of: frame, in: group, among: glass)
        GlassGroups.spring({
            // By its transform: UIKit resolves the corners against the box an
            // animated frame ends at, from its first frame, and a glass drawn in
            // would turn square at once.
            if let neighbour {
                let bead = GlassGroups.bead(for: frame, at: neighbour)
                ghost.transform = CGAffineTransform(translationX: bead.midX - frame.midX, y: bead.midY - frame.midY)
                    .scaledBy(x: bead.width / frame.width, y: bead.height / frame.height)
            }
            ghost.effect = nil
            ghost.contentView.alpha = 0
        }, completion: { ghost.removeFromSuperview() })
    }
}

/// A departed glass's paint: no hit target, nothing to assistive technology.
final class GlassGhost: UIVisualEffectView {
    override init(effect: UIVisualEffect?) {
        super.init(effect: effect)
        isUserInteractionEnabled = false
        isAccessibilityElement = false
        accessibilityElementsHidden = true
    }
    required init?(coder: NSCoder) { nil }
}
#endif
