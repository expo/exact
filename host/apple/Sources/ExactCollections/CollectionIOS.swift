// UIKit keeps gesture recognition; this recognizer only observes a bounded pin.
#if os(iOS) || os(tvOS)
import UIKit
import ExactKit
import ExactCore

private final class CollectionContact: UIGestureRecognizer {
    weak var collections: CollectionHost?
    private weak var contact: UITouch?
    override func canPrevent(_ other: UIGestureRecognizer) -> Bool { false }
    override func canBePrevented(by other: UIGestureRecognizer) -> Bool { false }
    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        guard contact == nil, let touch = touches.first else { return }
        contact = touch
        // A finger coming down on a list's port is the reader's gesture
        // against its `scrollIntoView` (LLP 1070.000 §2.5), reported with the
        // pin's flush
        collections?.gestureBegan(around: touch.view, at: touch.timestamp)
        var view = touch.view
        while let current = view {
            if let node = current as? NodeView {
                collections?.pointer(node.id); return
            }
            view = current.superview
        }
        collections?.pointer(nil)
    }
    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) {
        if let contact, touches.contains(contact) { collections?.releaseInteractionLater(); self.contact = nil; state = .failed }
    }
    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) {
        collections?.releaseInteractionLater(); contact = nil; state = .cancelled
    }
    override func reset() { super.reset(); contact = nil }
}

extension CollectionHost {
    private func hidden(_ node: UIView) -> Bool {
        var view: UIView? = node
        while let current = view {
            if current.isHidden { return true }
            view = current.superview
        }
        return false
    }
    /// A list's facts on its own axes (LLP 1070 H1): a row list's offset,
    /// port and measured sizes run along x, its cross along y.
    package func geometry(_ id: UInt32) -> CollectionFacts? {
        #if DEBUG
        geometryReads += 1
        #endif
        guard let node = presenter?.views[id], let scroll = node.scroll, !hidden(node) else { return nil }
        // Layout's box, not the one drawn on the pixel grid (`PixelGridIOS.swift`): the runner lays rows to it.
        // What it draws through (`drawnThrough`), the page band or the kernel's reach, is port, and on its main axis padding
        let horizontal = entries[id]?.snapshot.horizontal ?? false
        let bounds = node.layoutBounds, insets = scroll.adjustedContentInset, outset = node.scrollOutset
        let through = drawnThrough(id, node, scroll)
        let records = rowRecords(id)
        let key = CollectionRecords.Port(bounds: bounds, insets: insets, outset: outset, through: through)
        let at = (animating.contains(id) ? owedTargets[id] ?? animationTargets[id] : nil) ?? scroll.contentOffset
        if records?.port == key, var facts = records?.facts {
            let content = records!.content
            facts.offset = Double(horizontal ? max(-CGFloat(facts.paddingStart), at.x + insets.left - through.left - content.minX)
                : max(-CGFloat(facts.paddingStart), at.y + insets.top - through.top - content.minY))
            return facts
        }
        let content = node.contentBox()
        func keep(_ facts: CollectionFacts) -> CollectionFacts {
            records?.port = key; records?.facts = facts; records?.content = content
            return facts
        }
        // The physical port is the scroll view's frame, page bands and reach included, less the part of each inset it
        // does not draw through. What it draws through on its main axis is padding before and after the rows.
        let portWidth = max(0, bounds.width + outset.left + outset.right - (insets.left - through.left) - (insets.right - through.right))
        let portHeight = max(0, bounds.height + outset.top + outset.bottom - (insets.top - through.top) - (insets.bottom - through.bottom))
        guard portWidth.isFinite, portHeight.isFinite else { return nil }
        // Animating to a smooth correction: where it is headed is the port
        // the runner plans from (`CollectionHost.animating`).
        let measured = entries[id]?.snapshot.rows.first.flatMap { records?.rows[$0.view]?.cross }.map { CGFloat($0) }
        if horizontal {
            let available = max(0, portHeight - content.minY - (node.layoutBounds.height - content.maxY))
            let cross = measured ?? available
            guard cross.isFinite else { return nil }
            let start = max(0, node.padding("left")) + through.left, end = max(0, node.padding("right")) + through.right
            // From the first item: negative in the padding before it, down
            // to that padding (LLP 1010 §6.9).
            return keep(CollectionFacts(offset: Double(max(-start, at.x + insets.left - through.left - content.minX)),
                portMain: Double(portWidth), portCross: Double(portHeight), cross: Double(cross),
                measurements: [], focus: nil, interaction: nil, paddingStart: Double(start), paddingEnd: Double(end)))
        }
        let available = max(0, portWidth - content.minX - (node.layoutBounds.width - content.maxX))
        let width = measured ?? available
        guard width.isFinite else { return nil }
        let start = max(0, node.padding("top")) + through.top, end = max(0, node.padding("bottom")) + through.bottom
        return keep(CollectionFacts(offset: Double(max(-start, at.y + insets.top - through.top - content.minY)),
            portMain: Double(portHeight), portCross: Double(portWidth), cross: Double(width),
            measurements: [], focus: nil, interaction: nil, paddingStart: Double(start), paddingEnd: Double(end)))
    }
    /// A smooth correction's native targets move with a page band's inset
    /// (LLP 1130 D3), as the offset itself does (`NodeView.setPageBand`), and
    /// so does where the batch found the port: its offset counts the band as
    /// padding before the first row, so the top of a list the band grew
    /// under mid-batch is the new band's top.
    func offsetsMoved(_ id: UInt32, by dy: CGFloat) {
        animationTargets[id]?.y += dy; owedTargets[id]?.y += dy
        entries[id]?.batchStart? += Double(dy)
        offsetDrivers[id]?.moveOffsets(by: dy)
    }
    /// What UIKit's bars cover of the list's port at its start and end: what it draws through (`drawnThrough`), which
    /// `geometry` counts as port and padding. The runner aligns a row clear of them, as
    /// `scrollToItem(at:at:animated:)` aligns within the adjusted content insets.
    func covered(_ id: UInt32) -> [Double] {
        guard let node = presenter?.views[id], let scroll = node.scroll else { return [0, 0] }
        let band = drawnThrough(id, node, scroll)
        return entries[id]?.snapshot.horizontal == true ? [Double(band.left), Double(band.right)] : [Double(band.top), Double(band.bottom)]
    }
    /// `covered`, told the runner when it changes, before the report that goes with it.
    func reportCovered(_ id: UInt32, _ entry: Entry) {
        let covered = covered(id)
        guard covered != entry.covered, let runtime = presenter?.session?.runtime else { return }
        entry.covered = covered
        runtime.listCovered(id, start: covered[0], end: covered[1])
    }
    /// The insets UIKit adjusts the list's port by, at its start edges. A
    /// safe area its frame changed reaches the scroll view in a layout
    /// pass, which is taken now, inside the batch that moved it.
    func portInsets(_ id: UInt32) -> [Double]? {
        guard let node = presenter?.views[id], let scroll = node.scroll else { return nil }
        node.layoutIfNeeded()
        let through = drawnThrough(id, node, scroll), i = scroll.adjustedContentInset
        return [Double(i.top - through.top), Double(i.left - through.left)]
    }
    func drawnThrough(_ id: UInt32) -> UIEdgeInsets? {
        guard let node = presenter?.views[id], let scroll = node.scroll else { return nil }
        return drawnThrough(id, node, scroll)
    }
    /// What a list draws through of its insets (`scrollDrawnThrough`): under reach, on the edges of its main axis only,
    /// its port across being the frame less the whole inset; with none, the page band, on the top and bottom whatever
    /// its axis.
    private func drawnThrough(_ id: UInt32, _ node: NodeView, _ scroll: UIScrollView) -> UIEdgeInsets {
        let t = node.scrollDrawnThrough(scroll)
        guard node.reach != .zero else { return t }
        return entries[id]?.snapshot.horizontal == true ? UIEdgeInsets(top: 0, left: t.left, bottom: 0, right: t.right)
            : UIEdgeInsets(top: t.top, left: 0, bottom: t.bottom, right: 0)
    }
    /// The port back at `top` (`geometry`'s offset), within what it can
    /// reach, unless the reader or a smooth correction is moving it.
    func keepStart(_ id: UInt32, at top: Double) {
        guard let node = presenter?.views[id], let scroll = node.scroll, !animating.contains(id),
              !scroll.isTracking, !scroll.isDragging, !scroll.isDecelerating else { return }
        let content = node.contentBox(), i = scroll.adjustedContentInset
        var target = scroll.contentOffset
        let through = drawnThrough(id, node, scroll)
        if entries[id]?.snapshot.horizontal ?? false { target.x = CGFloat(top) + content.minX - i.left + through.left }
        else { target.y = CGFloat(top) + content.minY - i.top + through.top }
        target = Self.reachable(target, in: scroll)
        if scroll.contentOffset != target { scroll.setContentOffset(target, animated: false) }
    }
    /// An anchor's correction (`CollectionCursor.takeShift`): the offset
    /// moves by `delta` from `start` (where the batch began) in the layout
    /// pass that moved the rows. Assigning
    /// `contentOffset` keeps a pan or a deceleration going from the new
    /// offset at its velocity, as `UICollectionView`'s self-sizing
    /// invalidation does with its content offset adjustment.
    func shift(_ id: UInt32, by delta: Double, extent: Double, from start: Double?) {
        guard let node = presenter?.views[id], let scroll = node.scroll else { return }
        fit(node, scroll, extent: extent)
        guard delta != 0, delta.isFinite else { return }
        let horizontal = entries[id]?.snapshot.horizontal ?? false
        if animating.contains(id), let headed = owedTargets[id] ?? animationTargets[id] {
            // The rows moved under a running smooth correction: where it
            // lands moves with them, retargeted from what shows.
            let moved = horizontal ? CGPoint(x: headed.x + CGFloat(delta), y: headed.y) : CGPoint(x: headed.x, y: headed.y + CGFloat(delta))
            owedTargets[id] = nil
            animateOffset(id, scroll, to: moved)
            return
        }
        // An element reveal heads for where its target now is instead (`IntoView.swift`).
        if presenter?.retargetReveal(scroll) == true { return }
        let insets = scroll.adjustedContentInset
        var target = scroll.contentOffset
        if let start {
            // From where the batch began (`geometry`'s offset): UIKit has
            // already pulled the offset in to content that shrank under it.
            let content = node.contentBox(), through = drawnThrough(id, node, scroll)
            if horizontal { target.x = CGFloat(start) - insets.left + through.left + content.minX } else { target.y = CGFloat(start) - insets.top + through.top + content.minY }
        }
        if horizontal { target.x += CGFloat(delta) } else { target.y += CGFloat(delta) }
        if scroll.isTracking || scroll.isDragging || scroll.isDecelerating {
            // Past the end under a finger or a fling, a correction back toward
            // the end that stays past it is UIKit's rubber band coming back on
            // its own.
            // Corrected each report, a fling stopped dead at the end (r27's
            // tasks list).
            let end = horizontal ? scroll.contentSize.width + insets.right - scroll.bounds.width
                : scroll.contentSize.height + insets.bottom - scroll.bounds.height
            let at = horizontal ? scroll.contentOffset.x : scroll.contentOffset.y, to = horizontal ? target.x : target.y
            if at > end + 0.5, to < at, to >= end - 0.5 { return }
        } else {
            // At rest the port stays inside the content (under a finger or
            // a fling UIKit's own rubber band owns the overshoot).
            if horizontal {
                let maximum = max(-insets.left, scroll.contentSize.width + insets.right - scroll.bounds.width)
                target.x = min(maximum, max(-insets.left, target.x))
            } else {
                let maximum = max(-insets.top, scroll.contentSize.height + insets.bottom - scroll.bounds.height)
                target.y = min(maximum, max(-insets.top, target.y))
            }
        }
        target = PixelGrid.offset(target, in: scroll, clamped: !(scroll.isTracking || scroll.isDragging || scroll.isDecelerating))
        guard scroll.contentOffset != target else { return }
        let moved = horizontal ? target.x - scroll.contentOffset.x : target.y - scroll.contentOffset.y
        scroll.contentOffset = target
        // Not the reader's travel: the fill's velocity reads on from here.
        presenter?.shifted(id, by: moved)
    }
    /// The content size the list's extent needs.
    private func fit(_ node: NodeView, _ scroll: UIScrollView, extent: Double) {
        let content = node.contentBox()
        if entries[node.id]?.snapshot.horizontal ?? false {
            let right = node.layoutBounds.width - content.maxX
            let width = max(node.scrollFloorWidth(scroll), PixelGrid.length(max(CGFloat(extent) + content.minX + right, node.content.width), from: node.grid?.origin ?? .zero, vertical: false, scale: presenter?.gridScale ?? 1))
            if scroll.contentSize.width != width { scroll.contentSize.width = width }
        } else {
            let bottom = node.layoutBounds.height - content.maxY
            let height = max(node.scrollFloor(scroll), PixelGrid.length(max(CGFloat(extent) + content.minY + bottom, node.content.height), from: node.grid?.origin ?? .zero, vertical: true, scale: presenter?.gridScale ?? 1))
            if scroll.contentSize.height != height { scroll.contentSize.height = height }
        }
    }
    /// A smooth correction's target that arrived while one was animating.
    func landAnimation(_ id: UInt32) {
        guard let target = owedTargets.removeValue(forKey: id), let scroll = presenter?.views[id]?.scroll,
              !scroll.isTracking, !scroll.isDragging, !scroll.isDecelerating else { return }
        let gap = abs(target.y - scroll.contentOffset.y) + abs(target.x - scroll.contentOffset.x)
        guard gap > 0.5 else { return }
        if ExactEnv.agentFreezes {
            if animating.contains(id) { stopAnimation(id) }
            scroll.setContentOffset(target, animated: false)
        } else {
            animateOffset(id, scroll, to: target)
        }
    }
    /// A smooth correction: one ease-in-out motion of `smoothDuration` from
    /// where the port is now, driven a display frame at a time on the
    /// scroll view's own `contentOffset`, as UIKit's scroll animation is: the
    /// offset every reader sees (hit-testing, a drag's start, stickies, the
    /// scroll event) is the one on screen. A later target while it runs
    /// retargets it from where it is, never a jump (LLP 1010 §6.8): a list
    /// following its end first heads for an estimated row, then the measured
    /// one. UIKit's `setContentOffset(_:animated:)` could only restart from
    /// rest, so a second target waited and landed at once.
    ///
    /// It begins on the next turn with the last target that turn gives: two
    /// motions begun milliseconds apart showed a step back first.
    func animateOffset(_ id: UInt32, _ scroll: UIScrollView, to target: CGPoint) {
        let target = Self.reachable(target, in: scroll)
        if let driver = offsetDrivers[id], driver.scroll === scroll {
            beginAnimation(id, to: target)
            driver.retarget(target, serial: animationSerial[id] ?? 0)
            return
        }
        let pending = startOwed.contains(id) && animating.contains(id)
        let serial = beginAnimation(id, to: target)
        if pending, let token = pendingToken[id] { pendingSerial[id] = serial; _ = token; return }
        // Its own token: a stale start (a list reset and recreated under the
        // same id) neither starts nor stops the one that replaced it.
        nextPendingToken += 1
        let token = nextPendingToken
        startOwed.insert(id)
        pendingToken[id] = token
        pendingSerial[id] = serial
        DispatchQueue.main.async { [weak self, weak scroll] in
            guard let self, self.pendingToken[id] == token else { return }
            self.startOwed.remove(id); self.pendingToken[id] = nil
            let serial = self.pendingSerial.removeValue(forKey: id)
            guard self.animationSerial[id] == serial, let latest = self.animationTargets[id] else { return }
            guard let scroll, scroll.window != nil, self.presenter?.views[id]?.scroll === scroll else {
                // Its view went away before it began: nothing is in flight,
                // and the list reports where it is.
                self.stopAnimation(id)
                self.reportAgain(id)
                return
            }
            // Its target folded to where the port already is (an estimate,
            // then the measured row): no motion.
            if abs(latest.y - scroll.contentOffset.y) + abs(latest.x - scroll.contentOffset.x) < 0.5 {
                if scroll.contentOffset != latest { scroll.setContentOffset(latest, animated: false) }
                self.animationEnded(id, atTarget: true)
                return
            }
            self.startOffsetDriver(id, scroll, to: latest)
        }
    }
    /// `target` within the offsets the port can reach now.
    static func reachable(_ target: CGPoint, in scroll: UIScrollView) -> CGPoint {
        PixelGrid.offset(target, in: scroll)
    }
    private func startOffsetDriver(_ id: UInt32, _ scroll: UIScrollView, to target: CGPoint) {
        let serial = animationSerial[id] ?? beginAnimation(id, to: target)
        let driver = OffsetDriver(scroll: scroll, to: target, duration: Self.smoothDuration, serial: serial) { [weak self] serial, finished in
            guard let self, self.animationSerial[id] == serial else { return }
            self.offsetDrivers[id] = nil
            if finished {
                self.animationEnded(id, atTarget: true)
            } else {
                // Stopped short (its view left the window): it reports where it is.
                self.stopAnimation(id)
                self.reportAgain(id)
            }
        } reclamp: { [weak self] reached in
            self?.animationTargets[id] = reached
        }
        offsetDrivers[id]?.cancel()
        offsetDrivers[id] = driver
        driver.start()
    }
    /// UIKit's batch-update and scroll timing, which Signal's transcript rides.
    static let smoothDuration: TimeInterval = 0.3
    func correct(_ id: UInt32, top: Double, extent: Double, smooth: Bool = false, authored: Bool = false) {
        guard let node = presenter?.views[id], let scroll = node.scroll else { return }
        let content = node.contentBox(), insets = scroll.adjustedContentInset
        let horizontal = entries[id]?.snapshot.horizontal ?? false
        fit(node, scroll, extent: extent)
        // UIKit owns dragging/deceleration. An authored position is not
        // permission to interrupt that animation with setContentOffset (an
        // anchor's correction moves with it instead: `shift`). The next
        // native offset notification supplies the continuing intent.
        // A `scrollIntoView`'s correction is the exception, as UIKit's own
        // `scrollToItem` is (LLP 1070.000 §2.6): `setContentOffset(_:animated:
        // false)` ends a deceleration and lands, and a finger down drags on
        // from there.
        let moving = scroll.isTracking || scroll.isDragging || scroll.isDecelerating
        guard authored || !moving else { return }
        // An authored correction is a newer position than one an arranged pane owes
        if authored { node.cancelScrollHandoff() }
        // Under Reduce Motion a smooth correction lands at once, as an
        // element's reveal does (LLP 1070.000 §14)
        var smooth = smooth && !DisplayPreferences.reducedMotion
        if moving {
            // What was sampled before it is not the reader's travel after it;
            // a finger still down drags on, and its next move says so
            // (`ScrollPump.scrolled`).
            presenter?.forgetTravel(id)
            if scroll.isTracking {
                // Under a dragging finger a smooth one lands at once too:
                // UIKit's animated call would pull the port from under it.
                // Under a still finger it runs, as UIKit's does, until the
                // finger drags (`scrollViewWillBeginDragging`).
                if scroll.isDragging { smooth = false }
            } else if smooth {
                // During a deceleration (`isDragging` stays true through one)
                // a smooth one first stops it where the port is, then animates
                // from rest: UIKit's animated call made during a deceleration
                // came to rest short of its target.
                if #available(iOS 17.4, tvOS 17.4, *) { scroll.stopScrollingAndZooming() }
                else { scroll.setContentOffset(scroll.contentOffset, animated: false) }
            }
        }
        var target: CGPoint
        let through = drawnThrough(id, node, scroll)
        if horizontal {
            let maximum = max(-insets.left, scroll.contentSize.width + insets.right - scroll.bounds.width)
            target = CGPoint(x: min(maximum, max(-insets.left, CGFloat(top) + content.minX - insets.left + through.left)),
                y: scroll.contentOffset.y)
        } else {
            let maximum = max(-insets.top, scroll.contentSize.height + insets.bottom - scroll.bounds.height)
            target = CGPoint(x: scroll.contentOffset.x,
                y: min(maximum, max(-insets.top, CGFloat(top) + content.minY - insets.top + through.top)))
        }
        target = Self.reachable(target, in: scroll)
        // A smooth correction is UIKit's scroll animation (LLP 1070.000
        // §6.2): the window is already built at the destination; rows in
        // between are not (§11 declares it). Under the agent's frozen clock
        // it lands at once.
        let animate = smooth && !ExactEnv.agentFreezes && node.window != nil
        if !animate, animating.contains(id) {
            // An ordinary correction stops it, even where it already is.
            stopAnimation(id)
            scroll.setContentOffset(target, animated: false)
            return
        }
        if animate {
            // On its way or at rest: one motion from what shows, retargeted
            // if one runs (`animateOffset`).
            // A gap under half a point is no motion: 0.3 s of frames for it
            // moved nothing anyone could see.
            if !animating.contains(id), abs(scroll.contentOffset.y - target.y) + abs(scroll.contentOffset.x - target.x) < 0.5 {
                if scroll.contentOffset != target { scroll.setContentOffset(target, animated: false) }
                return
            }
            animateOffset(id, scroll, to: target)
            return
        }
        guard scroll.contentOffset != target else { return }
        scroll.setContentOffset(target, animated: false)
    }
    /// Coverage runs retain gaps and outlying pins. An offset-only frame
    /// compares the port with those numbers without visiting row views.
    func covers(_ id: UInt32) -> Bool {
        guard let entry = entries[id], let scroll = presenter?.views[id]?.scroll,
              let records = rowRecords(id) else { return false }
        let port = entry.snapshot.horizontal ? scroll.bounds.minX...scroll.bounds.maxX : scroll.bounds.minY...scroll.bounds.maxY
        return records.coverage?.covers(port) ?? false
    }
    /// Rows beyond the coverage run at the port's leading edge, at the
    /// mounted rows' mean size. A pinned row across a gap cannot end travel.
    func rowsToCover(_ id: UInt32, ahead: CGFloat) -> UInt32 {
        guard ahead != 0, let entry = entries[id], let scroll = presenter?.views[id]?.scroll,
              let records = rowRecords(id), records.mean > 0 else { return 0 }
        let port = entry.snapshot.horizontal ? scroll.bounds.minX...scroll.bounds.maxX : scroll.bounds.minY...scroll.bounds.maxY
        let shortfall: CGFloat
        if ahead > 0 {
            let run = records.reach(from: port.lowerBound, forward: true)
            if run.index == entry.snapshot.count - 1 { return 0 }
            shortfall = port.upperBound + ahead - run.edge
        } else {
            let run = records.reach(from: port.upperBound, forward: false)
            if run.index == 0 { return 0 }
            shortfall = run.edge - (port.lowerBound + ahead)
        }
        return shortfall > 0 ? UInt32(min(64, (shortfall / records.mean).rounded(.up))) : 0
    }
    /// Pins the row under a finger in `port`, the viewport or a presentation's
    /// mount (LLP 1123 D2), so a list's window keeps it for the contact.
    @discardableResult
    func watchContacts(in port: UIView) -> UIGestureRecognizer {
        let recognizer = CollectionContact(target: nil, action: nil)
        recognizer.collections = self
        recognizer.cancelsTouchesInView = false
        recognizer.delaysTouchesBegan = false
        recognizer.delaysTouchesEnded = false
        port.addGestureRecognizer(recognizer)
        return recognizer
    }
    /// The reader moves `id`'s port with no finger on it, by a scroll run
    /// for them with UIKit's animation: a tap on the status bar or on the
    /// selected tab, a remote's step. As a drag does, the move stops the
    /// host's animation where the port is, leaves a correction planned before
    /// it stale, and ends the request under way there (LLP 1070.000 §2.5).
    func readerMoves(_ id: UInt32) {
        guard entries[id] != nil else { return }
        gestureBegan(id)
        animationEnded(id, dragging: true)
        userIntent(id, travel: true)
    }
    /// The latest request `id` took, under way or ended: 0 for none, or for
    /// a view that is no list.
    func lastRequest(_ id: UInt32) -> UInt64 { entries[id]?.snapshot.lastRequest ?? 0 }
    /// Every list's latest request, by its view (`lastRequest`).
    func lastRequests() -> [UInt32: UInt64] { entries.mapValues { $0.snapshot.lastRequest } }
    func focusedView() -> UInt32? {
        presenter?.views.values.first(where: {
            $0.isFirstResponder || $0.field?.isFirstResponder == true || $0.textArea?.isFirstResponder == true
        })?.id
    }
    func startTracking() {
        guard let viewport = presenter?.viewport else { return }
        let recognizer = watchContacts(in: viewport)
        let observer = NotificationCenter.default.addObserver(forName: UIApplication.willResignActiveNotification,
            object: nil, queue: .main) { [weak self] _ in self?.releaseInteractionLater() }
        stopTracking = { [weak viewport] in
            viewport?.removeGestureRecognizer(recognizer)
            NotificationCenter.default.removeObserver(observer)
        }
    }
}
#endif

#if os(iOS) || os(tvOS)
/// One smooth correction's frames: ease-in-out from where the port was to
/// where it is headed, the offset set each display frame. A retarget once a
/// frame has shown begins a fresh ease from where the port is; one before
/// is still the motion's one target (`retarget`).
final class OffsetDriver: NSObject {
    weak var scroll: UIScrollView?
    private var from: CGPoint
    private(set) var to: CGPoint, began: CFTimeInterval = 0
    /// Whether a frame has moved the port yet.
    private(set) var drawn = false
    private let duration: TimeInterval
    private var serial: Int
    private let done: (Int, Bool) -> Void
    private let reclamp: (CGPoint) -> Void
    init(scroll: UIScrollView, to: CGPoint, duration: TimeInterval, serial: Int, done: @escaping (Int, Bool) -> Void,
         reclamp: @escaping (CGPoint) -> Void) {
        self.scroll = scroll; self.from = scroll.contentOffset; self.to = to
        self.duration = duration; self.serial = serial; self.done = done; self.reclamp = reclamp
    }
    func start() {
        began = CACurrentMediaTime()
        FrameClock.shared.want(self, .offsetDriver, rate: FrameClock.full(on: scroll?.window?.screen)) { [weak self] in self?.frame($0) }
    }
    /// Native offsets move by `dy` under it (a page band's inset, LLP 1130 D3).
    func moveOffsets(by dy: CGFloat) { from.y += dy; to.y += dy }
    func retarget(_ target: CGPoint, serial: Int) {
        guard let scroll else { return }
        self.serial = serial
        // Before its first frame the motion has not shown: a followed end's
        // measured row, a report after its estimate, is still its one target
        // (LLP 1010 §6.8). After it, or once the port moved under it (content
        // that shrank clamped it), a fresh ease from what shows.
        if !drawn, PixelGrid.same(scroll.contentOffset, from, in: scroll) {
            // Folded back onto the port: no motion at all.
            if abs(target.y - from.y) + abs(target.x - from.x) < 0.5 {
                scroll.contentOffset = target
                cancel(); done(serial, true)
                return
            }
            to = target
            return
        }
        from = scroll.contentOffset; to = target
        began = CACurrentMediaTime()
    }
    func cancel() { FrameClock.shared.drop(self) }
    /// CSS `ease-in-out`, cubic-bezier(0.42, 0, 0.58, 1), as UIKit's.
    static func ease(_ x: Double) -> Double {
        let bez = { (a: Double, b: Double, s: Double) in 3 * a * s * (1 - s) * (1 - s) + 3 * b * s * s * (1 - s) + s * s * s }
        var lo = 0.0, hi = 1.0
        for _ in 0..<32 { let m = (lo + hi) / 2; if bez(0.42, 0.58, m) < x { lo = m } else { hi = m } }
        return bez(0, 1, (lo + hi) / 2)
    }
    private func frame(_ link: CADisplayLink) {
        guard let scroll, scroll.window != nil else { cancel(); done(serial, false); return }
        // Content that shrank under it: from where the port can be now,
        // toward the edge it can reach; every frame stays inside the range.
        let reached = CollectionHost.reachable(to, in: scroll)
        let here = CollectionHost.reachable(scroll.contentOffset, in: scroll)
        // (within a hundredth of a pixel: UIKit's read-back is a ULP off the grid; PixelGrid.same)
        if !PixelGrid.same(reached, to, in: scroll) || !PixelGrid.same(here, scroll.contentOffset, in: scroll) {
            from = here; to = reached; began = link.targetTimestamp; reclamp(reached)
            if here == reached { scroll.contentOffset = here; cancel(); done(serial, true); return }
        }
        let x = min(1, (link.targetTimestamp - began) / duration)
        let e = CGFloat(Self.ease(max(0, x)))
        let point = CGPoint(x: from.x + (to.x - from.x) * e, y: from.y + (to.y - from.y) * e)
        scroll.contentOffset = CollectionHost.reachable(point, in: scroll)
        drawn = true
        if x >= 1 { cancel(); done(serial, true) }
    }
}
#endif
