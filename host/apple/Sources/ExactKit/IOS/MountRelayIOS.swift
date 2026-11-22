// @ref LLP 1123 D2 — a presented route's logical ancestors hear its touches.
#if os(iOS)
import UIKit

/// What a presented route's or a modal dialog's authored ancestors listen
/// for, carried by the view its sheet mounts it in. UIKit gives a touch to
/// the recognizers of the views it falls in, and a sheet's content is
/// mounted out of the document it is declared in, so an ancestor's own
/// recognizers no longer see it. As a DOM event bubbles to an ancestor,
/// the mount's recognizers give a touch to the nearest ancestor that listens
/// — a pointer observer, a long press (`contextmenu`), a double tap
/// (`dblclick`) — unless a nearer listener in the sheet takes it with its
/// own recognizer. Recognition and arbitration stay UIKit's.
/// A view that holds authored content out of the tree it is declared in (a sheet's mount, the tab bar's accessory)
/// and relays its touches to the content's logical ancestors, a menu one of them gives included (`ContextMenuHost`).
package protocol RelayMount: UIView {
    var relay: MountRelay? { get }
}

extension PresentationMount: RelayMount {}

package final class MountRelay: NSObject, UIGestureRecognizerDelegate {
    private weak var presenter: Presenter?
    unowned let mount: UIView
    private let pointer = PointerRecognizer(target: nil, action: nil)
    private let press = UILongPressGestureRecognizer()
    private let double = UITapGestureRecognizer()
    private weak var pressed, doubled: NodeView?

    package init(_ presenter: Presenter, mount: UIView) {
        self.presenter = presenter
        self.mount = mount
        super.init()
        pointer.relay = self
        press.addTarget(self, action: #selector(longPressed(_:)))
        double.addTarget(self, action: #selector(doubleTapped(_:)))
        double.numberOfTapsRequired = 2
        for g in [pointer, press, double] as [UIGestureRecognizer] {
            // A recognized long press takes its touch, as a node's own does:
            // no press follows a context menu
            g.cancelsTouchesInView = g === press; g.delaysTouchesBegan = false; g.delaysTouchesEnded = false
            if g !== pointer { g.delegate = self }
            mount.addGestureRecognizer(g)
        }
    }

    /// The nearest node above `touched`, through logical parents, that
    /// `listens`, when it is outside the mount: nil when none does or when
    /// one in the sheet is nearer, whose own recognizer takes the touch.
    /// With `holders`, a node that holds the mount counts too, as its own
    /// menu leaves a press on the mount to the mount's (`ContextMenuHost`).
    func listener(_ touched: UIView?, contextMenu: Bool = false, holders: Bool = false, _ listens: (NodeView) -> Bool) -> NodeView? {
        var view = touched
        while let v = view {
            // One inside the mount, or one that holds it (the root over a layer of lifted nodes), hears the touch itself
            if let n = v as? NodeView, listens(n), !(contextMenu ? n.formDisabled : n.disabled) {
                return n.isDescendant(of: mount) || !holders && mount.isDescendant(of: n) ? nil : n
            }
            view = presenter?.pointerParent(of: v) ?? v.superview
        }
        return nil
    }

    /// The node above the mount whose `contextmenu` a long press on `touched` at `location` (in the mount) relays,
    /// decided once by the nearest node or run that hears a long press or gives a menu: none where that is a run
    /// (`runAnswers`) or a node in the mount (its own recognizer or menu takes the press), or a node that gives a
    /// menu the mount's interaction opens (`ContextMenuHost.syncMounts`), which delivers the event itself, as a
    /// node's long press yields to its own menu. Where the mount opens no menu (a lifted node's stand), a menu above
    /// it cannot show, and the nearest listener above hears the press.
    func longPressListener(_ touched: UIView?, at location: CGPoint) -> NodeView? {
        if runAnswers(touched, at: location) { return nil }
        let gives = { (n: NodeView) in n.props["contextPopover"]?.isEmpty == false }
        guard let nearest = listener(touched, contextMenu: true, { $0.handlers.contains("contextmenu") || gives($0) }) else { return nil }
        guard gives(nearest) else { return nearest }
        let opens = mount.interactions.contains { $0 is UIContextMenuInteraction }
        return opens ? nil : listener(touched, contextMenu: true) { $0.handlers.contains("contextmenu") }
    }

    /// Whether a run under the touch at `location` (in the mount) hears a long press or gives a menu: it is nearer
    /// than every node, and its paragraph's own recognizer or interaction takes the press
    func runAnswers(_ touched: UIView?, at location: CGPoint) -> Bool {
        guard let paragraph = touched.flatMap(NodeView.innermost) else { return false }
        return paragraph.inlineContextTarget(at: mount.convert(location, to: paragraph)) != nil
    }

    package func gestureRecognizer(_ g: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
        // What a node's own long press and double tap leave to native input
        // (`NodeView.gestureRecognizer(_:shouldReceive:)`)
        if g !== pointer, NodeView.nativeInputOwns(touch.view, below: mount) { return false }
        if g === press { pressed = longPressListener(touch.view, at: touch.location(in: mount)); return pressed != nil }
        if g === double { doubled = listener(touch.view) { $0.handlers.contains("dblclick") }; return doubled != nil }
        return true
    }
    @objc private func longPressed(_ g: UILongPressGestureRecognizer) { pressed?.openContext(g) }
    @objc private func doubleTapped(_ g: UITapGestureRecognizer) { doubled?.doubleClicked(g) }
}
#endif
