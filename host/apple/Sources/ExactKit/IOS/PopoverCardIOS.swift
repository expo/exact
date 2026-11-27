// @ref LLP 1021 §5.4 — a content popover opened from glass on iOS 26 is
// UIKit's popover, zoomed out of the glass, with no arrow and no sheet. While
// the keys are up it lies over them: no window of the app's draws over the
// keyboard's, and UIKit lays a popover out of the keyboard's frame, so the
// keys stand down behind a picture of themselves (`StoodDownKeys`), taken
// from the window UIKit draws them in (`KeyboardWindow`).
#if os(iOS)
import ObjectiveC
import UIKit

final class PopoverCard: UIViewController, UIPopoverPresentationControllerDelegate {
    /// Closes the popover once UIKit dismisses the card (a tap outside, a
    /// pull); cleared when the app closes the popover itself (`close`).
    var gone: (() -> Void)?
    /// The keys it stood down, which come back as it goes.
    var keys: StoodDownKeys?
    private var turning: NSKeyValueObservation?

    override func loadView() {
        view = UIView()
        view.backgroundColor = .clear
    }

    /// The popover's box, from its top-left corner, is the card.
    func fit(_ popover: NodeView) {
        let size = popover.bounds.size
        if preferredContentSize != size { preferredContentSize = size }
        let center = CGPoint(x: size.width / 2, y: size.height / 2)
        if popover.center != center { popover.center = center }
    }

    /// The app closed the popover: a picture of the box shrinks back into the
    /// opener as its box goes back to its page.
    func close() {
        gone = nil
        guard let presenting = presentingViewController, !isBeingDismissed else { return }
        if let picture = view.snapshotView(afterScreenUpdates: false) { view.addSubview(picture) }
        presenting.dismiss(animated: true)
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        // A turn closes it at once, the keys back as UIKit brings them
        guard turning == nil, let scene = presentingViewController?.view.window?.windowScene else { return }
        turning = scene.observe(\.effectiveGeometry, options: [.old, .new]) { [weak self] _, change in
            guard let self, change.oldValue?.interfaceOrientation != change.newValue?.interfaceOrientation else { return }
            keys?.giveBack()
            keys = nil
            presentingViewController?.dismiss(animated: false)
        }
    }

    // UIKit starts to dismiss it: the popover closes now, as a light dismiss
    // does on the web; a pull, which can still be cancelled, once it is done
    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        if transitionCoordinator?.isInteractive != true { leave() }
    }

    override func viewDidDisappear(_ animated: Bool) {
        super.viewDidDisappear(animated)
        turning = nil
        leave()
    }

    // The keys come back, and the popover closes, after the batch that may have closed it
    private func leave() {
        let keys = self.keys
        self.keys = nil
        DispatchQueue.main.async { keys?.bringBack() }
        guard let gone else { return }
        self.gone = nil
        if let picture = view.snapshotView(afterScreenUpdates: false) { view.addSubview(picture) }
        gone()
    }

    // A popover on a phone too, where UIKit would make it a sheet
    func adaptivePresentationStyle(for controller: UIPresentationController, traitCollection: UITraitCollection) -> UIModalPresentationStyle { .none }
}

/// The keys put away behind a picture of themselves while a card lies over
/// them: the editor resigns, and the editing route keeps their room, its
/// editor and its focus as the app sees them (`Presenter.keyboardEditor`,
/// `keyboardGuideTop`, `KeyboardSampleIOS.held`) until they are back.
final class StoodDownKeys {
    private weak var presenter: Presenter?
    private weak var window: UIWindow?
    private(set) weak var editor: UIView?
    /// The keys as they stood in their window, which their readers report while they are down.
    let sample: KeyboardSampleIOS.Sample
    /// The app blurred the editor while the keys were down: they go, not come back.
    var blurred = false
    private let picture: UIView
    /// The keys' window, and a second picture laid in it as the keys come back, so they do not slide in.
    private weak var keysWindow: UIWindow?
    private let cover: UIView
    private var done = false

    /// Stands the keys down; nil when no docked keys are up.
    init?(presenter: Presenter) {
        guard let editor = presenter.keyboardEditor, let window = editor.window,
              let sampler = KeyboardSampleIOS.existing(in: window), let sample = sampler.sample, sample.docked,
              let keys = KeyboardWindow.showing(sample.frame, over: window),
              let shot = keys.snapshotView(afterScreenUpdates: false),
              let again = keys.snapshotView(afterScreenUpdates: false) else { return nil }
        self.presenter = presenter
        self.window = window
        self.editor = editor
        self.sample = sample
        keysWindow = keys
        // The keys' picture, cut to where they show, in the app's window and, for their return, in theirs
        let offset = CGPoint(x: keys.frame.minX - window.frame.minX, y: keys.frame.minY - window.frame.minY)
        picture = Self.cut(shot, to: sample.frame, at: offset)
        cover = Self.cut(again, to: sample.frame.offsetBy(dx: -offset.x, dy: -offset.y), at: .zero)
        window.addSubview(picture)
        presenter.heldKeys = self
        sampler.held = sample
        UIView.performWithoutAnimation { _ = editor.resignFirstResponder() }
    }

    // `shot`, a picture of a window whose origin is at `origin`, shown only within `frame`
    private static func cut(_ shot: UIView, to frame: CGRect, at origin: CGPoint) -> UIView {
        let holder = UIView(frame: frame)
        holder.clipsToBounds = true
        holder.isUserInteractionEnabled = false
        shot.frame.origin = CGPoint(x: origin.x - frame.minX, y: origin.y - frame.minY)
        holder.addSubview(shot)
        return holder
    }

    /// The node whose editor the keys belong to.
    var node: NodeView? { editor.flatMap(NodeView.innermost) }

    /// The app's `blur()` while the keys are down, for their editor or for
    /// any: they go as the card closes. True when it was theirs to take.
    func blur(_ args: [Any]) -> Bool {
        guard done == false, let node else { return false }
        if let name = args.first as? String, node.props["id"] != name { return false }
        blurred = true
        return true
    }

    /// The editor takes the keys back as the card goes, under their picture,
    /// which also covers them in their own window as they come, and goes once
    /// they show (or a second later if they never do).
    func bringBack() {
        guard !done else { return }
        if blurred { return slideAway() }
        guard let editor, editor.window != nil else { return finish() }
        let center = NotificationCenter.default
        var observers: [NSObjectProtocol] = []
        observers.append(center.addObserver(forName: UIResponder.keyboardWillShowNotification, object: nil, queue: .main) { [weak self] _ in
            guard let self, !done, let keys = keysWindow else { return }
            keys.addSubview(cover)
        })
        observers.append(center.addObserver(forName: UIResponder.keyboardDidShowNotification, object: nil, queue: .main) { [weak self] _ in
            observers.forEach(center.removeObserver)
            self?.finish()
        })
        UIView.performWithoutAnimation { _ = editor.becomeFirstResponder() }
        DispatchQueue.main.asyncAfter(deadline: .now() + 1) { [weak self] in
            observers.forEach(center.removeObserver)
            self?.finish()
        }
    }

    /// The keys back at once, without the picture, as UIKit brings them (a turn).
    func giveBack() {
        guard !done else { return }
        if !blurred { editor?.becomeFirstResponder() }
        finish()
    }

    // The keys leave as a keyboard does: the picture slides down on the
    // keyboard's curve, the route's room let go in the same animation.
    private func slideAway() {
        let picture = self.picture
        UIView.animate(withDuration: 0.25, delay: 0, options: [UIView.AnimationOptions(rawValue: 7 << 16), .beginFromCurrentState], animations: {
            picture.frame.origin.y += picture.bounds.height
            self.release()
            self.presenter?.session?.view?.layoutIfNeeded()
        }, completion: { _ in self.finish() })
    }

    /// The picture goes and the route has the keyboard UIKit has. An editor
    /// that ends without the keys reports its blur, held back till now.
    func finish() {
        guard !done else { return }
        done = true
        picture.removeFromSuperview()
        cover.removeFromSuperview()
        let node = self.node
        release()
        if let presenter, let node, editor?.isFirstResponder != true { presenter.blur(node.id) }
    }

    private func release() {
        guard let presenter, presenter.heldKeys === self else { return }
        presenter.heldKeys = nil
        if let window { KeyboardSampleIOS.existing(in: window)?.held = nil }
        presenter.session?.view?.setNeedsLayout()
    }

    // Its session went first: the pictures go with it
    deinit {
        picture.removeFromSuperview()
        cover.removeFromSuperview()
    }
}

/// The window UIKit draws the keys in, known by what UIKit says of it rather
/// than by its class: it is the one visible window that no scene lists in its
/// `windows`, from iOS 26.0 to 27.1 (LLP 1021 §5.4). It is heard of as it
/// becomes visible, so the watch starts before any keys show
/// (`KeyboardSampleIOS.install`).
enum KeyboardWindow {
    /// Each window so heard of, while it lives: UIKit may keep more than one.
    nonisolated(unsafe) private static let seen = NSHashTable<UIWindow>.weakObjects()
    nonisolated(unsafe) private static var watching: NSObjectProtocol?

    static func watch() {
        guard watching == nil else { return }
        watching = NotificationCenter.default.addObserver(forName: UIWindow.didBecomeVisibleNotification, object: nil, queue: .main) { note in
            guard let window = note.object as? UIWindow, window.windowScene?.windows.contains(window) != true else { return }
            seen.add(window)
        }
    }

    /// The keys' window while it shows `keys`, a frame in `window`'s points:
    /// the highest such window, visible, that holds them.
    static func showing(_ keys: CGRect, over window: UIWindow) -> UIWindow? {
        seen.allObjects.filter { candidate in
            guard candidate !== window, !candidate.isHidden, candidate.windowScene?.windows.contains(candidate) != true else { return false }
            let shown = keys.offsetBy(dx: window.frame.minX - candidate.frame.minX, dy: window.frame.minY - candidate.frame.minY)
            return candidate.bounds.insetBy(dx: -0.5, dy: -0.5).contains(shown)
        }.max { $0.windowLevel < $1.windowLevel }
    }
}

extension Presenter {
    nonisolated(unsafe) private static var heldKeysKey: UInt8 = 0

    /// The keys a card stood down, until they are back or gone.
    var heldKeys: StoodDownKeys? {
        get { objc_getAssociatedObject(self, &Self.heldKeysKey) as? StoodDownKeys }
        set { objc_setAssociatedObject(self, &Self.heldKeysKey, newValue, .OBJC_ASSOCIATION_RETAIN_NONATOMIC) }
    }
}

extension MenuHost {
    /// Shows `popover` as a card when it opens from a glass control (see the
    /// file's head); false when it lifts into Exact's own layer instead: a
    /// manual popover, which keeps its own dismissal, before iOS 26, or under
    /// the agent's painted presentation.
    func showsCard(_ popover: NodeView, in layer: PopoverLayer, from source: UInt32?) -> Bool {
        if let card = layer.card {
            if let presenter { layer.mount(in: presenter) }
            card.fit(popover)
            return true
        }
        guard #available(iOS 26.0, *), !ExactEnv.agentPaints, popover.props["popover"] != "manual", let presenter,
              let name = popover.props["id"], let opener = source.flatMap({ presenter.views[$0] }), opener.window != nil,
              Materials.glass(opener.materialKind), let glass = opener.materialView,
              var host = presenter.viewport.window?.rootViewController else { return false }
        while let next = host.presentedViewController, !next.isBeingDismissed { host = next }
        let card = PopoverCard()
        card.loadViewIfNeeded()
        layer.card = card
        layer.mount(in: presenter)
        if popover.superview !== layer { layer.addSubview(popover) }
        if popover.isHidden { popover.isHidden = false }
        card.fit(popover)
        card.modalPresentationStyle = .popover
        if let chrome = card.popoverPresentationController {
            chrome.delegate = card
            chrome.sourceView = glass
            chrome.sourceRect = glass.bounds
            chrome.permittedArrowDirections = []
        }
        card.preferredTransition = .zoom { [weak glass] _ in glass }
        // A card opened from inside another shares the keys the first stood down
        if presenter.heldKeys == nil { card.keys = StoodDownKeys(presenter: presenter) }
        card.gone = { [weak self] in self?.drop(name) }
        host.present(card, animated: true)
        return true
    }
}
#endif
