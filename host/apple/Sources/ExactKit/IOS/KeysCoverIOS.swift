// @ref LLP 1021 §5.4 — a content popover opened from glass lies over the
// keys. No window of the app's draws over the keyboard's, so while it does,
// the keys' window is made clear and a picture of the keys lies under the
// popover in its layer, as the platform's chat does under its + menu: the
// editor keeps the focus and the keys stay up as UIKit sees them, and nothing
// moves as the popover opens or closes. An opening decides on its first
// placement and binds what let it (its editor, the keys, its opener); one
// picture serves it, and once that ground goes the keys come back at once and
// the popover is placed again above them until it closes.
#if os(iOS) || os(tvOS)
import UIKit

extension MenuHost {

    /// Places a lifted popover by `place`: over the keys while its opening's
    /// cover stands, else in the region above them, measured there.
    func placeOverOrAboveKeys(_ entry: Lifted, _ popover: NodeView, place: (CGRect) -> Void) {
        #if os(iOS)
        if let cover = keysCover(entry, popover) {
            place(entry.layer.bounds)
            if cover.settle(popover) { return }
            spend(entry)
        }
        #endif
        let above = abovePlacement(entry)
        place(above)
        holdAbove(popover, in: above)
    }

    /// The layer's bounds, ended at the keys' top while they show.
    func abovePlacement(_ entry: Lifted) -> CGRect {
        var placement = entry.layer.bounds
        if let top = presenter?.keyboardTop {
            let end = entry.layer.convert(CGPoint(x: 0, y: top), from: nil).y
            placement.size.height = max(0, min(placement.maxY, end) - placement.minY)
        }
        return placement
    }

    private func holdAbove(_ popover: NodeView, in placement: CGRect) {
        guard presenter?.keyboardTop != nil else { return }
        let height = popover.bounds.height
        let y = max(placement.minY + height / 2, min(popover.center.y, placement.maxY - height / 2))
        if popover.center.y != y { popover.center.y = y }
    }
}

#if os(iOS)
extension MenuHost {
    /// The opening's cover while it stands. The first placement with a size
    /// decides: a content popover (no menu, by invocation or role, not
    /// manual, not nested in another) with no editor of its own, opened from
    /// glass, whose layer shows in the active window, while the session's
    /// editor holds docked keys whose window is known and settled, binds a
    /// cover to them; any other is spent, and stays above the keys for the
    /// rest of its opening, as UIKit lays out its own popover and menu.
    func keysCover(_ entry: Lifted, _ popover: NodeView) -> KeysCover? {
        let layer = entry.layer
        guard !layer.keysSpent else { return nil }
        if let cover = layer.keysCover {
            if cover.stands { return cover }
            spend(entry)
            return nil
        }
        // Undecided till it has a size: its region is told before it is laid out
        guard !popover.bounds.isEmpty else { return nil }
        guard !entry.substitutesMenu, entry.ancestor == nil, !popover.isHidden, popover.props["popover"] != "manual",
              popover.props["accessibilityRole"] != "menu", !Self.edits(popover),
              let presenter, let window = presenter.viewport.window, layer.window === window, KeysCover.shows(layer),
              let opener = entry.source.flatMap({ presenter.views[$0] }), opener.glassBody != nil,
              let editor = presenter.keyboardEditor, editor.window === window, editor.isFirstResponder,
              let sample = KeyboardSampleIOS.existing(in: window)?.sample, sample.docked,
              let keys = KeyboardWindow.showing(sample.frame, over: window) else {
            spend(entry)
            return nil
        }
        let cover = KeysCover(keys, frame: sample.frame, editor: editor, opener: opener, popover: popover, entry: entry, menus: self)
        layer.keysCover = cover
        return cover
    }

    /// The opening's cover gone, the keys back at once, and no other this opening.
    func spend(_ entry: Lifted) {
        // Spent first: the cover's removal, as it goes, can run callbacks that would ask for one
        entry.layer.keysSpent = true
        entry.layer.keysCover = nil
    }

    static func edits(_ view: UIView) -> Bool {
        view is UIKeyInput || view.subviews.contains(where: edits)
    }
}

/// An opening's hold on the keys: bound, as it decided, to the keys' window,
/// their frame, the editor holding them, its popover and its opener. Laid,
/// the keys' window is clear and their picture lies under the popover in its
/// layer, taking a touch on the keys as a touch outside the popover: it
/// light-dismisses it and presses nothing. While laid it checks on each frame
/// that its ground holds (`holds`) and retires when it does not; the scene's
/// deactivation and its layer leaving its window retire it at once.
final class KeysCover {
    private weak var keys: UIWindow?
    private let keysID: ObjectIdentifier
    private let frame: CGRect
    private weak var editor: UIView?
    private weak var opener: NodeView?
    private let openerIncarnation: UInt64
    private weak var popover: NodeView?
    private weak var entry: MenuHost.Lifted?
    private weak var menus: MenuHost?
    private let picture: KeysPicture
    private(set) var isLaid = false
    private var deactivates: NSObjectProtocol?

    init(_ keys: UIWindow, frame: CGRect, editor: UIView, opener: NodeView, popover: NodeView, entry: MenuHost.Lifted, menus: MenuHost) {
        self.keys = keys
        keysID = ObjectIdentifier(keys)
        self.frame = frame
        self.editor = editor
        self.opener = opener
        openerIncarnation = opener.incarnation
        self.popover = popover
        self.entry = entry
        self.menus = menus
        picture = KeysPicture(menus: menus)
    }

    /// Whether what it is bound to stands: the keys up in their window where
    /// it took them, clear only while it is laid; its editor holding them; its
    /// opening still lifted, from its opener, a content popover still (not
    /// manual, no menu, no editor of its own: a light dismissal is what a tap
    /// on the picture gives); its layer showing in their window.
    var stands: Bool {
        guard let keys, let editor, let entry, let presenter = menus?.presenter, let popover, let opener,
              let window = editor.window, entry.layer.window === window, Self.shows(entry.layer),
              editor.isFirstResponder, presenter.keyboardEditor === editor,
              menus?.lifts(entry) == true, entry.popover === popover,
              entry.source.flatMap({ presenter.views[$0] }) === opener, opener.incarnation == openerIncarnation,
              popover.props["popover"] != "manual", popover.props["accessibilityRole"] != "menu", !MenuHost.edits(popover),
              let sample = KeyboardSampleIOS.existing(in: window)?.sample, sample.docked, Self.near(sample.frame, frame),
              KeyboardWindow.showing(sample.frame, over: window) === keys else { return false }
        return keys.alpha == (isLaid ? 0 : 1)
    }

    /// After its popover is placed in the keys' region: true when that
    /// placement stands, the picture laid under a popover that meets the keys
    /// (taken the first time), or the popover clear of them, never yet laid.
    func settle(_ popover: NodeView) -> Bool {
        guard let layer = entry?.layer, let window = layer.window else { return false }
        guard meets(popover) else { return !isLaid }
        return reaches(layer, window) && lay(in: layer, over: window)
    }

    /// The picture gone and, when no other cover of theirs is laid, the keys as they were, at once.
    func remove() {
        // The picture goes and the keys come back in one change, which no animation the caller runs in carries
        UIView.performWithoutAnimation {
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            picture.removeFromSuperview()
            if isLaid {
                isLaid = false
                FrameClock.shared.drop(self)
                if let deactivates { NotificationCenter.default.removeObserver(deactivates) }
                deactivates = nil
                KeysClearing.release(keysID, by: ObjectIdentifier(self))
            }
            CATransaction.commit()
        }
    }

    /// Its ground gone: the keys back at once, and its popover placed again above them for the rest of its opening.
    func retire() {
        guard let entry, let menus else { return remove() }
        menus.spend(entry)
        menus.relift(entry)
    }

    deinit { remove() }

    /// Whether `view` shows: in a window of an active scene, with no hidden or clear ancestor.
    static func shows(_ view: UIView) -> Bool {
        guard let window = view.window, window.windowScene?.activationState == .foregroundActive else { return false }
        return sequence(first: view, next: { $0.superview }).allSatisfy { !$0.isHidden && $0.alpha > 0.01 }
    }

    /// Whether a touch at `point` (the window's points) that hits `hit` in the
    /// app's window lands over keys a cover holds clear, or on the popover over them.
    static func takes(_ hit: UIView, at point: CGPoint) -> Bool {
        guard let cover = PopoverLayer.containing(hit)?.keysCover, cover.holds else { return false }
        return cover.frame.contains(point)
    }

    /// While laid: its ground stands, its picture lies where the keys show,
    /// and its popover still meets them from a layer that reaches over them all.
    private var holds: Bool {
        guard isLaid, stands, let popover, let layer = entry?.layer, picture.superview === layer, let window = layer.window,
              Self.near(picture.convert(picture.bounds, to: nil), frame) else { return false }
        return meets(popover) && reaches(layer, window) && takesTouches(layer, window)
    }

    /// Whether a touch anywhere over the keys reaches `layer`, as the window hit-tests it: no view of the app's
    /// above the layer (a sibling over it, transparent or not) takes one first. Sampled at the keys' corners and
    /// middle.
    private func takesTouches(_ layer: UIView, _ window: UIWindow) -> Bool {
        let inside = frame.insetBy(dx: 1, dy: 1)
        return [CGPoint(x: inside.minX, y: inside.minY), CGPoint(x: inside.maxX, y: inside.minY), CGPoint(x: inside.midX, y: inside.midY),
                CGPoint(x: inside.minX, y: inside.maxY), CGPoint(x: inside.maxX, y: inside.maxY)].allSatisfy {
            window.hitTest($0, with: nil)?.isDescendant(of: layer) == true
        }
    }

    private func meets(_ popover: NodeView) -> Bool {
        !popover.isHidden && popover.convert(popover.bounds, to: nil).intersects(frame)
    }

    /// Whether `layer` reaches over all the keys as hit-testing does, drawn
    /// one to one: each axis unscaled, unturned and unmirrored.
    private func reaches(_ layer: UIView, _ window: UIWindow) -> Bool {
        guard let reach = NativeGeometry.hitRegion(of: layer, in: window), reach.insetBy(dx: -0.5, dy: -0.5).contains(frame) else { return false }
        let o = layer.convert(CGPoint.zero, to: window)
        return Self.near(layer.convert(CGPoint(x: 1, y: 0), to: window), CGPoint(x: o.x + 1, y: o.y))
            && Self.near(layer.convert(CGPoint(x: 0, y: 1), to: window), CGPoint(x: o.x, y: o.y + 1))
    }

    /// The picture at the bottom of `layer` where the keys show; the first
    /// time, taken and the keys cleared with it, as one change no animation
    /// carries. False when the keys give no picture.
    private func lay(in layer: PopoverLayer, over window: UIWindow) -> Bool {
        if !isLaid {
            guard let keys, keys.alpha == 1, let shot = keys.snapshotView(afterScreenUpdates: false) else { return false }
            // The windows share the screen's points but not, on iOS 27, a UIScreen object, across which
            // UIKit converts no rect: the picture is placed by the windows' frames
            shot.frame.origin = CGPoint(x: keys.frame.minX - window.frame.minX - frame.minX, y: keys.frame.minY - window.frame.minY - frame.minY)
            picture.addSubview(shot)
        }
        UIView.performWithoutAnimation {
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            let place = layer.convert(frame, from: nil)
            if picture.frame != place { picture.frame = place }
            if picture.superview !== layer || layer.subviews.first !== picture {
                layer.insertSubview(picture, at: 0)
                // Under every popover, a negatively ranked one included (PaintOrder)
                picture.setPaintBehind()
            }
            if !isLaid, let keys { KeysClearing.hold(keys, by: ObjectIdentifier(self)) }
            CATransaction.commit()
        }
        guard !isLaid else { return true }
        isLaid = true
        FrameClock.shared.want(self, .keysCover, rate: CAFrameRateRange(minimum: 10, maximum: 30, preferred: 30)) { [weak self] _ in
            guard let self, !holds else { return }
            retire()
        }
        deactivates = NotificationCenter.default.addObserver(forName: UIScene.willDeactivateNotification, object: window.windowScene, queue: .main) { [weak self] _ in
            self?.retire()
        }
        return true
    }

    private static func near(_ a: CGRect, _ b: CGRect) -> Bool {
        near(a.origin, b.origin) && abs(a.width - b.width) < 0.5 && abs(a.height - b.height) < 0.5
    }

    private static func near(_ a: CGPoint, _ b: CGPoint) -> Bool { abs(a.x - b.x) < 0.01 && abs(a.y - b.y) < 0.01 }
}

/// The keys' picture under a popover: a touch on it is a touch outside the popover.
private final class KeysPicture: UIView {
    private weak var menus: MenuHost?
    init(menus: MenuHost) {
        self.menus = menus
        super.init(frame: .zero)
        clipsToBounds = true
    }
    required init?(coder: NSCoder) { nil }
    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) { menus?.lightDismiss(self) }
}

/// The keys' windows made clear, each while one of its covers is laid: its
/// alpha as it was comes back when its last cover goes, unless UIKit has
/// set another since.
enum KeysClearing {
    private struct Clear { weak var window: UIWindow?; let alpha: CGFloat; var covers: Set<ObjectIdentifier> }
    nonisolated(unsafe) private static var clear: [ObjectIdentifier: Clear] = [:]

    static func hold(_ window: UIWindow, by cover: ObjectIdentifier) {
        let id = ObjectIdentifier(window)
        if clear[id] == nil {
            clear[id] = Clear(window: window, alpha: window.alpha, covers: [])
            window.alpha = 0
        }
        clear[id]?.covers.insert(cover)
    }

    static func release(_ window: ObjectIdentifier, by cover: ObjectIdentifier) {
        guard var entry = clear[window] else { return }
        entry.covers.remove(cover)
        if entry.covers.isEmpty {
            // Unrecorded before the alpha comes back: a cover that a callback of that change lays records the window
            // again and clears it
            clear[window] = nil
            if entry.window?.alpha == 0 { entry.window?.alpha = entry.alpha }
        } else {
            clear[window] = entry
        }
    }
}
#else
final class KeysCover {
    static func takes(_ hit: UIView, at point: CGPoint) -> Bool { false }
}
#endif
#if os(iOS)

/// The window UIKit draws the keys in, known by what UIKit says of it rather
/// than by its class (LLP 1021 §5.4, measured on iOS 26.0 to 27.1): of the
/// windows no scene lists in its `windows`, the one that alone became visible
/// shortly before the keyboard's local show and holds the frame it gives. It
/// stays known while it stays visible (UIKit may keep it up, empty, while the
/// keyboard is down), through a show with no window new (the keys passing to
/// another field, or showing again), and is forgotten as it hides or by an
/// ambiguous show. Its keys show once the keyboard says it has shown at the
/// frame it gave, till it hides. The watch starts with the first session view
/// (`ExactView`).
enum KeyboardWindow {
    private struct Seen { weak var window: UIWindow?; let at: CFTimeInterval }
    nonisolated(unsafe) private static weak var known: UIWindow?
    /// The keys' frame the keyboard last gave, in screen points, and whether it has shown there.
    nonisolated(unsafe) private static var end: CGRect?
    nonisolated(unsafe) private static var settled = false
    /// The windows no scene lists that became visible since the keyboard last showed or hid.
    nonisolated(unsafe) private static var pending: [Seen] = []
    nonisolated(unsafe) private static var watching: [NSObjectProtocol] = []
    /// How long before its show the keys' window becomes visible, at most: measured at 20 to 100 ms.
    private static let lead: CFTimeInterval = 0.5

    static func watch() {
        guard watching.isEmpty else { return }
        let center = NotificationCenter.default
        watching = [
            center.addObserver(forName: UIWindow.didBecomeVisibleNotification, object: nil, queue: .main) { note in
                guard let window = note.object as? UIWindow, let scene = window.windowScene, !scene.windows.contains(window) else { return }
                pending.append(Seen(window: window, at: CACurrentMediaTime()))
            },
            center.addObserver(forName: UIWindow.didBecomeHiddenNotification, object: nil, queue: .main) { note in
                if let window = note.object as? UIWindow, window === known { forget() }
            },
            center.addObserver(forName: UIResponder.keyboardWillShowNotification, object: nil, queue: .main) { note in
                show(local(note))
            },
            center.addObserver(forName: UIResponder.keyboardWillChangeFrameNotification, object: nil, queue: .main) { note in
                guard let frame = local(note), let window = known, !window.isHidden, window.frame.contains(frame) else { return }
                (end, settled) = (frame, false)
            },
            center.addObserver(forName: UIResponder.keyboardDidShowNotification, object: nil, queue: .main) { note in
                if let frame = local(note), frame == end { settled = true }
            },
            center.addObserver(forName: UIResponder.keyboardDidChangeFrameNotification, object: nil, queue: .main) { note in
                if let frame = local(note), frame == end { settled = true }
            },
            center.addObserver(forName: UIResponder.keyboardWillHideNotification, object: nil, queue: .main) { _ in hide() },
            center.addObserver(forName: UIResponder.keyboardDidHideNotification, object: nil, queue: .main) { _ in hide() },
        ]
    }

    /// A show, at `frame` when it is this app's keyboard's: the one window
    /// that became visible just before and holds it is the keys', or the
    /// known one, when none did and it still holds it; none, if several did,
    /// or if one did while the known one still holds it.
    private static func show(_ frame: CGRect?) {
        defer { pending.removeAll() }
        guard let frame else { return forget() }
        let now = CACurrentMediaTime()
        var candidates: [UIWindow] = []
        for seen in pending where now - seen.at <= lead {
            if let window = seen.window, !window.isHidden, window.frame.contains(frame), !candidates.contains(where: { $0 === window }) {
                candidates.append(window)
            }
        }
        // A newcomer while the known window still holds the frame is as ambiguous as two newcomers
        let knownHolds = known.map { !$0.isHidden && $0.frame.contains(frame) } ?? false
        if candidates.count == 1, !knownHolds || known === candidates[0] {
            known = candidates[0]
        } else if !candidates.isEmpty || !knownHolds {
            known = nil
        }
        (end, settled) = known == nil ? (nil, false) : (frame, false)
    }

    private static func forget() {
        known = nil
        hide()
    }

    private static func hide() {
        (end, settled) = (nil, false)
        pending.removeAll()
    }

    /// A notification's end frame, in screen points, when it says the keyboard is this app's.
    private static func local(_ note: Notification) -> CGRect? {
        guard (note.userInfo?[UIResponder.keyboardIsLocalUserInfoKey] as? Bool) == true else { return nil }
        return (note.userInfo?[UIResponder.keyboardFrameEndUserInfoKey] as? NSValue)?.cgRectValue
    }

    /// The keys' window while it shows `keys`, a frame in `window`'s points,
    /// settled where the keyboard last said: visible, unlisted, above
    /// `window` and as large; nil otherwise.
    static func showing(_ keys: CGRect, over window: UIWindow) -> UIWindow? {
        guard settled, let candidate = known, let end, candidate !== window, !candidate.isHidden,
              let scene = candidate.windowScene, !scene.windows.contains(candidate),
              candidate.windowLevel > window.windowLevel, candidate.frame == window.frame, candidate.bounds.contains(keys) else { return nil }
        let shown = keys.offsetBy(dx: window.frame.minX, dy: window.frame.minY)
        let near = abs(end.minX - shown.minX) < 1 && abs(end.minY - shown.minY) < 1 && abs(end.width - shown.width) < 1 && abs(end.height - shown.height) < 1
        return near ? candidate : nil
    }
}
#endif
#endif
