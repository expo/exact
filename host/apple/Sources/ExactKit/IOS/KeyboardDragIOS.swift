#if os(iOS) || os(tvOS)
import ObjectiveC
import UIKit

extension Presenter {
    /// A finger drags a scroller in an interactive dismiss mode, the keyboard following it: the session's page, a
    /// node's scroller or a primary route's page.
    var interactiveKeyboardDrag: Bool {
        let drags = { (sv: UIScrollView) in
            [.interactive, .interactiveWithAccessory].contains(sv.keyboardDismissMode) && (sv.isTracking || sv.isDragging)
        }
        return drags(viewport) || scrollers.contains { views[$0]?.scrollView.map(drags) ?? false }
            || navigation.primaryRouteSurfaces.contains { $0.contentViewport.map(drags) ?? false }
    }
}
#endif

#if os(iOS)

/// A scroller's keyboard drag (LLP 1123 D7). While a finger drags the scroll view in an interactive dismiss mode, the
/// drag holds its window's dismiss padding (`KeyboardSampleIOS.hold`): its node's `-exact-keyboard-dismiss-padding` in
/// the window's points, read again whenever the window settles its padding, or, while the window's first responder is
/// a route bar's search field UIKit carried onto the keys, at least the field's top, taken as the window's drags begin.
/// It lets the padding go as the drag ends, is cancelled or fails, which UIKit reports too for a scroll view taken out
/// of its window mid-drag. The scroll view's own pan target
/// was added first and hears the end first, so the release leaves UIKit's choice to dismiss the keyboard or bring it
/// back as it was (§"Evidence: measured on expo/exact", "Keyboard dismissal").
package final class KeyboardDrag: NSObject {
    private weak var scroll: UIScrollView?
    private var node: () -> NodeView?
    private weak var holding: KeyboardSampleIOS?
    /// The bar's search field UIKit carried onto the keys as the window's drags began, and how far its top was above
    /// theirs then, at rest: the field is part of the keys for the whole drag while it still edits (`carriedOnKeys`).
    /// The window's sampler holds the one copy for its run of drags (`KeyboardSampleIOS.carried`).
    package struct Carried {
        weak var field: UIView?
        let offset: CGFloat
    }

    nonisolated(unsafe) private static var key: UInt8 = 0

    private init(_ scroll: UIScrollView, node: @escaping () -> NodeView?) {
        self.scroll = scroll
        self.node = node
        super.init()
    }

    /// Watches `scroll`'s drags, whose padding is `node`'s: once per scroll view, a later call naming its node again.
    @discardableResult
    package static func watch(_ scroll: UIScrollView, node: @escaping () -> NodeView?) -> KeyboardDrag {
        if let drag = objc_getAssociatedObject(scroll, &key) as? KeyboardDrag {
            drag.node = node
            return drag
        }
        let drag = KeyboardDrag(scroll, node: node)
        objc_setAssociatedObject(scroll, &key, drag, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        scroll.panGestureRecognizer.addTarget(drag, action: #selector(panned(_:)))
        return drag
    }

    @objc private func panned(_ pan: UIPanGestureRecognizer) { track(pan.state) }

    /// Follows the scroll view's pan through `state`.
    package func track(_ state: UIGestureRecognizer.State) {
        guard let scroll else { return }
        switch state {
        case .began, .changed:
            let interactive = scroll.keyboardDismissMode == .interactive || scroll.keyboardDismissMode == .interactiveWithAccessory
            // A drag that holds the padding keeps its place among the window's drags while its scroller's dismissal
            // changes, the window passing over it while that is not interactive (`KeyboardSampleIOS.settlePadding`)
            guard interactive || holding != nil, let window = scroll.window, let sampler = KeyboardSampleIOS.existing(in: window) else { return release() }
            if holding !== sampler { release() }
            // Whether the keys carry the field is decided once for the window's overlapping drags, by the first,
            // from the keys at rest; a later one takes that, as the keys may be following a finger then, and the
            // field moves with them a little behind. A first drag that begins while a scroller Exact does not watch
            // moves the keys decides from the frames then: its run keeps that distance if they qualify, and gives
            // its own padding alone if not (LLP 1123 D7)
            if holding == nil, !sampler.holdsNow() { sampler.carried = Self.carried(in: window, keys: sampler.sample?.frame) }
            holding = sampler
            sampler.hold(for: scroll) { [weak self] in self?.padding() ?? 0 }
        default:
            release()
            node()?.presenter?.keyboardDragEnded()
        }
    }

    /// The padding this drag holds now, in its window's points: its node's, or at least the top of the field the keys
    /// carried as the window's drags began, while that field still edits in this window as the search of its item,
    /// outside this scroller and every navigation bar (a field inside the scroller moves with it). The window asks
    /// it of its newest drag whenever it settles its padding, so a drag that takes it back from a newer one gives it
    /// as things are then, not as they were.
    private func padding() -> CGFloat {
        guard let scroll, let window = scroll.window else { return 0 }
        let authored = max(0, CGFloat(node()?.style["keyboard_dismiss_padding"]?.number ?? 0))
        let held = scroll.convert(CGRect(x: 0, y: 0, width: 0, height: authored), to: window).height
        // Still the search of its item, editing in this window, outside this scroller and every navigation bar, where
        // UIKit moves a field it no longer integrates
        guard let carried = holding?.carried, let field = carried.field as? UITextField, field.isFirstResponder, field.window === window,
              (field.delegate as? any BarSearchOwner)?.searchItem != nil,
              !field.isDescendant(of: scroll), !Self.inNavigationBar(field) else { return held }
        return max(held, carried.offset)
    }

    /// The bar's search field UIKit carried onto `keys`, if the window's first responder is one, whichever Exact
    /// session's bar it is in, and how far its top is above theirs.
    static func carried(in window: UIWindow, keys: CGRect?) -> Carried? {
        guard let keys, let editor = FirstResponder.current as? UIView, editor.window === window,
              carriedOnKeys(editor, keys: keys, in: window) else { return nil }
        return Carried(field: editor, offset: keys.minY - editor.convert(editor.bounds, to: window).minY)
    }

    /// Whether `editor` is the search field of a route's bar that UIKit carried onto the keys (LLP 1123 D7), its
    /// owner found as the bar set it up: the field's delegate, which knows the navigation item the search is on. The
    /// delegate, not the field, names the search: on the folded iPhone Duo the search bar takes a new field as its
    /// search opens in the toolbar, outside the search bar's own views, and gives it the delegate.
    static func carriedOnKeys(_ editor: UIView, keys: CGRect, in window: UIWindow) -> Bool {
        guard let field = editor as? UITextField, let item = (field.delegate as? any BarSearchOwner)?.searchItem else { return false }
        return carriedOnKeys(field, item: item, keys: keys, in: window)
    }

    /// The test itself: the item's realized placement is integrated (on an iPhone, the toolbar's, which UIKit lays on
    /// the keys while the field edits); the field is in no navigation bar, its own route's or another column's, into
    /// which UIKit can integrate a split sidebar's search at the top; and it rests on the keys: over them across, its
    /// bottom at their top and less than its own height above it. A field UIKit keeps at the top meets none of these,
    /// and a floating keyboard beside the field does not hold it.
    static func carriedOnKeys(_ field: UIView, item: UINavigationItem, keys: CGRect, in window: UIWindow) -> Bool {
        guard #available(iOS 26.0, *), item.searchController != nil, field.window === window else { return false }
        let placement = item.searchBarPlacement
        guard placement == .integrated || placement == .integratedButton || placement == .integratedCentered else { return false }
        guard !inNavigationBar(field) else { return false }
        let shown = field.convert(field.bounds, to: window)
        guard shown.minX < keys.maxX, keys.minX < shown.maxX else { return false }
        return shown.maxY <= keys.minY + 0.5 && keys.minY - shown.maxY < shown.height
    }

    private static func inNavigationBar(_ view: UIView) -> Bool {
        sequence(first: view, next: { $0.superview }).contains { $0 is UINavigationBar }
    }

    private func release() {
        holding?.release(scroll)
        holding = nil
    }
}
/// The owner of a route bar's search field, which the bar makes the field's delegate: the navigation item the search
/// is on, while that item still holds the search's controller (LLP 1123 D7).
package protocol BarSearchOwner: AnyObject {
    var searchItem: UINavigationItem? { get }
}
#endif
