// CSS scroll snap on AppKit (LLP 1008 §9, "Scroll snap"). NSScrollView has
// no projected rest to replace, as UIKit's drag end has: when a trackpad's
// fingers lift, the rule (`ScrollSnap.swift`) picks a rest from where the
// momentum would carry the scroll, the scroll eases there, and the momentum
// AppKit sends after is dropped (`ChainingScrollView.scrollWheel`). A mouse
// wheel's notches, which have no lift, rest where the rule picks once they
// stop.
#if os(macOS)
import AppKit

extension NodeView {
    /// Where the clip's origin rests instead of `proposed`, on each axis
    /// `scroll-snap-type` names, the scroll released at `release`; `nil`
    /// when it rests where it would. A `drag` that began at `start` and
    /// lifted at `velocity` (points per millisecond) flicks on past where it
    /// began (`ScrollSnap.rest`). `keeping`: only where `resnap` moves it.
    func snappedOrigin(_ proposed: CGPoint, release: CGPoint, drag: (start: CGPoint, velocity: CGPoint)? = nil, keeping: Bool = false) -> CGPoint? {
        guard let kind = ScrollSnap.Kind(style["scroll_snap_type"]?.string), let sv = scroll, let document = sv.documentView else { return nil }
        let clip = sv.contentView
        // The clip's coordinates in the node's border box, where a list's rows are.
        let box = convert(clip.frame.origin, from: clip.superview)
        var result = proposed, snapped = false
        for vertical in [false, true] where vertical ? kind.y : kind.x {
            // The snapport is the clip less its insets and `scroll-padding`
            // (§4.1); positions are where its start is, an origin plus `shift`.
            let size = vertical ? clip.bounds.height : clip.bounds.width
            let inset = clip.contentInsets
            let padding = { (side: String) in self.style["scroll_padding_\(side)"]?.length(basis: size) ?? 0 }
            let lead = vertical ? inset.top : inset.left, trail = vertical ? inset.bottom : inset.right
            let shift = lead + padding(vertical ? "top" : "left")
            let port = max(1, size - lead - trail - padding(vertical ? "top" : "left") - padding(vertical ? "bottom" : "right"))
            let low = (vertical ? document.frame.minY : document.frame.minX) - lead
            let high = (vertical ? document.frame.maxY : document.frame.maxX) + trail - size
            let range = (low + shift)...(max(low, high) + shift)
            var areas: [ScrollSnap.Area] = []
            func visit(_ view: NSView) {
                for case let node as NodeView in view.subviews where !node.isHidden {
                    if let align = ScrollSnap.align(node.style["scroll_snap_align"]?.string, vertical: vertical) {
                        // The area is the border box grown by `scroll-margin` (§4.3).
                        let rect = IntoView.inset(node.convert(node.bounds, to: clip), style: node.style, family: "scroll_margin", outward: true)
                        areas.append(ScrollSnap.area(lo: vertical ? rect.minY : rect.minX, hi: vertical ? rect.maxY : rect.maxX, align: align,
                                                     stop: node.style["scroll_snap_stop"]?.string == "always", port: port, range: range, id: node.id))
                    }
                    // A nested scroll container captures its own snap areas (`ScrollSnap.ownsAreas`).
                    if node.scroll == nil && !ScrollSnap.ownsAreas(node.style["overflow_x"]?.string, node.style["overflow_y"]?.string) { visit(node.container) }
                }
            }
            visit(document)
            let wanted = (vertical ? proposed.y : proposed.x) + shift
            // A virtualized list's rows past its mounted ones, as on iOS.
            let distance = areas.map { abs(min($0.to, max($0.from, wanted)) - wanted) }.min() ?? .infinity
            let toBox = vertical ? box.y : box.x
            if distance > 0, let entry = presenter?.collections.entries[id], entry.snapshot.horizontal != vertical,
               let position = presenter?.session?.runtime.collectionSnap(id, proposed: Double(wanted + toBox), port: Double(port),
                    minimum: Double(range.lowerBound + toBox), maximum: Double(range.upperBound + toBox), distance: Double(distance)) {
                areas.append(ScrollSnap.Area(from: CGFloat(position) - toBox, to: CGFloat(position) - toBox, stop: false))
            }
            let release = (vertical ? release.y : release.x) + shift
            let axis = vertical ? 1 : 0
            func pick() -> CGFloat? {
                keeping
                    ? ScrollSnap.resnap(areas, rested: sv.snapRested[axis], current: wanted, mandatory: kind.mandatory, port: port)
                    : ScrollSnap.rest(areas, projected: wanted, release: release, start: drag.map { (vertical ? $0.start.y : $0.start.x) + shift },
                                      way: drag.map { ScrollSnap.way(vertical ? $0.velocity.y : $0.velocity.x) } ?? 0, mandatory: kind.mandatory, port: port)
            }
            guard var rest = pick() else { continue }
            // An unmounted `always` row between the release and the rest holds it too.
            if !keeping, abs(rest - release) > 0.5, presenter?.collections.entries[id]?.snapshot.horizontal == !vertical,
               let stop = presenter?.session?.runtime.collectionSnapStop(id, release: Double(release + toBox), rest: Double(rest + toBox), port: Double(port),
                                                                         minimum: Double(range.lowerBound + toBox), maximum: Double(range.upperBound + toBox)) {
                areas.append(ScrollSnap.Area(from: CGFloat(stop) - toBox, to: CGFloat(stop) - toBox, stop: true))
                rest = pick() ?? rest
            }
            sv.snapRested[axis] = ScrollSnap.Rested(rest, in: areas)
            if vertical { result.y = rest - shift } else { result.x = rest - shift }
            snapped = true
        }
        return snapped ? result : nil
    }

    /// CSS Scroll Snap 1 §5.5: a mandatory scroller at rest on a snap area
    /// follows it when a batch moves it, and when it went, rests on the
    /// position nearest (`ScrollSnap.resnap`).
    func resnap() {
        // Not while a gesture, a wheel, an animation or the snap's own ease moves it.
        guard let sv = scroll, ScrollSnap.Kind(style["scroll_snap_type"]?.string)?.mandatory == true, scrollEnd?.live != true,
              ProcessInfo.processInfo.systemUptime - sv.wheelAt > 0.3,
              !sv.snapSettling, scrollEnd?.elementAnimation == nil, sv.snapWheel == nil else { return }
        let clip = sv.contentView, at = clip.bounds.origin
        guard let rest = snappedOrigin(at, release: at, keeping: true) else { return }
        let to = clip.constrainBoundsRect(NSRect(origin: rest, size: clip.bounds.size)).origin
        guard abs(to.x - at.x) > 0.5 || abs(to.y - at.y) > 0.5 else { return }
        clip.scroll(to: to)
        sv.reflectScrolledClipView(clip)
    }

    /// A trackpad's fingers lifted from this scroller at `velocity` (points
    /// per millisecond, positive as the offset grows): it eases to the rest
    /// the rule picks from where the momentum would carry it, UIKit's normal
    /// deceleration's projection (a rate of 0.998 a millisecond). Whether it
    /// snaps, and the momentum that follows is not the scroll's.
    func snapAtLift(velocity: CGPoint, start: CGPoint) -> Bool {
        guard let clip = scroll?.contentView else { return false }
        let at = clip.bounds.origin, reach: CGFloat = 0.998 / (1 - 0.998)
        guard let rest = snappedOrigin(CGPoint(x: at.x + velocity.x * reach, y: at.y + velocity.y * reach), release: at,
                                       drag: (start, velocity)) else { return false }
        scroll?.snapDropsMomentum = true
        ease(to: rest)
        return true
    }

    /// A wheel without phases (a mouse's notches) moved this scroller from
    /// `origin`, where the wheel found it: once it has been still for
    /// `ScrollEnd.quiet`, it eases to the rest the rule picks, the notches a
    /// flick their way, as a browser's wheel moves a mandatory snap a
    /// position a notch; `now`, for the agent's wheel, whose drive reads the
    /// offset back at once, it goes there.
    func snapAfterWheel(from origin: CGPoint, now: Bool) {
        guard let sv = scroll, ScrollSnap.Kind(style["scroll_snap_type"]?.string) != nil else { return }
        sv.snapWheel?.work.cancel()
        let start = sv.snapWheel?.origin ?? origin
        if now {
            sv.snapWheel = nil
            let at = sv.contentView.bounds.origin
            if let rest = snappedOrigin(at, release: start, drag: (start, CGPoint(x: at.x - start.x, y: at.y - start.y))) { ease(to: rest, animated: false) }
            return
        }
        let work = DispatchWorkItem { [weak self, weak sv] in
            sv?.snapWheel = nil
            guard let self, let at = sv?.contentView.bounds.origin,
                  let rest = self.snappedOrigin(at, release: start, drag: (start, CGPoint(x: at.x - start.x, y: at.y - start.y))) else { return }
            self.ease(to: rest)
        }
        sv.snapWheel = (start, work)
        DispatchQueue.main.asyncAfter(deadline: .now() + ScrollEnd.quiet, execute: work)
    }

    /// Ease the clip to `origin`, as a smooth `scrollIntoView` does
    /// (`IntoView.swift`): its `scroll`s follow, its `scrollend` at the end.
    private func ease(to origin: CGPoint, animated: Bool = true) {
        guard let sv = scroll else { return }
        let clip = sv.contentView
        let to = clip.constrainBoundsRect(NSRect(origin: origin, size: clip.bounds.size)).origin
        cancelElementScroll()
        guard to != clip.bounds.origin else { return }
        guard animated, IntoView.animates([]) else {
            clip.scroll(to: to)
            sv.reflectScrolledClipView(clip)
            return
        }
        let token = UUID(), end = scrollEnd ?? ScrollEnd()
        scrollEnd = end
        end.elementAnimation = token; end.hold()
        sv.snapSettling = true
        NSAnimationContext.runAnimationGroup({ context in
            context.duration = 0.3
            context.timingFunction = CAMediaTimingFunction(name: .easeOut)
            clip.animator().setBoundsOrigin(to)
        }, completionHandler: { [weak self, weak sv] in
            guard end.elementAnimation == token else { return }
            sv?.snapSettling = false
            end.elementAnimation = nil
            // The momentum AppKit's live scroll waited on was dropped.
            end.live = false
            self?.endScroll()
        })
        sv.reflectScrolledClipView(clip)
    }
}
#endif
