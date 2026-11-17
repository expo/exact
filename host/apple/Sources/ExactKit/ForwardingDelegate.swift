// @ref LLP 1075.003 §3.5, §3.7, LLP 1075.003.000 §3.7 — the delegate slots
// Exact keeps on platform objects: a navigation controller's and a tab
// container's on iOS, a window toolbar's on the Mac. Exact's proxy fills each
// one, and the app's delegate, given through the module's hatches, hears what
// Exact does not implement.

import Foundation
#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// The delegate Exact puts in one platform object's slot, from `fill` until
/// `retire`: its subclass implements what Exact handles, and each call it does
/// not implement goes straight to the app's delegate.
///
/// UIKit and AppKit read which optional methods a delegate answers once, when
/// it is set, and later call the ones they read whatever the delegate answers
/// then (LLP 1075.003 §9.16; AppKit's customization palette asks a toolbar's
/// delegate for its immovable items, measured). So the proxy answers for the
/// app's delegate's methods too, is set again whenever the app's delegate
/// changes, and keeps that delegate until the app gives another or nil, or the
/// slot leaves Exact.
///
/// A delegate the proxy lets go goes on a later turn of the main queue: what
/// its `deinit` does (give another delegate, show another container or
/// toolbar) then meets the host's settled state, never a change the host has
/// made halfway, and never runs inside the platform's write.
class ForwardingDelegate: NSObject {
    /// The app's delegate (each subclass reads it as its platform's type).
    private(set) var appDelegate: AnyObject?
    /// The object whose slot the proxy fills, from `fill` until `retire`:
    /// the only slot the proxy ever fills.
    private weak var owner: DelegateSlotOwner?
    private var filled = false, retired = false
    private var journal: ((String) -> Void)?
    /// The last delegate given that the slot has not installed yet.
    private var pending: AnyObject??
    private var settling = false

    /// Whether the slot holds the proxy, rather than a delegate written
    /// outside Exact.
    var fillsSlot: Bool { owner?.slotDelegate === self }
    /// Whether the development check has named the slot as written outside
    /// Exact since it last held the proxy.
    var slotNamed = false

    /// Put the proxy in `owner`'s slot; `journal` hears each delegate the slot
    /// installs. The app has given no delegate yet, so the platform reads only
    /// the proxy's own answers.
    func fill(_ owner: DelegateSlotOwner, journal: @escaping (String) -> Void) {
        guard !filled else { return }
        filled = true
        self.owner = owner
        self.journal = journal
        owner.slotDelegate = self
    }

    /// The app's delegate, or nil to clear it. The slot is emptied and given
    /// the proxy again, so the platform reads its answers anew, and the old
    /// delegate goes. A slot that left Exact takes nothing, and one whose
    /// object went without a retire is retired.
    func give(_ delegate: AnyObject?) {
        guard filled, !retired else { return }
        if case let .some(replaced?) = pending { releaseLater(replaced) }
        pending = .some(delegate)
        settle()
    }

    /// The slot leaves Exact: it is emptied if it holds the proxy, and the
    /// app's delegate goes. A delegate given later is dropped.
    func retire() {
        guard !retired else { return }
        retired = true
        settle()
    }

    /// Bring the slot to what was last asked of it. A delegate given or a
    /// retire asked during a pass, from inside the platform's write (a
    /// delegate's `responds(to:)` as the platform reads it), is left to that
    /// pass, which takes it once the write ends. The platform keeps the
    /// answers its outermost write read (measured on AppKit's toolbar and
    /// UIKit's tab container), so a write nested inside it would leave the
    /// answers of a delegate the slot no longer holds.
    private func settle() {
        guard !settling else { return }
        settling = true
        defer { settling = false }
        while true {
            if retired || owner == nil {
                retired = true
                if case let .some(waiting?) = pending { releaseLater(waiting) }
                releaseLater(appDelegate)
                appDelegate = nil
                pending = nil
                if let owner, owner.slotDelegate === self { owner.slotDelegate = nil }
                owner = nil
                return
            }
            guard case let .some(delegate) = pending, let owner else { return }
            pending = nil
            releaseLater(appDelegate)
            owner.slotDelegate = nil
            appDelegate = delegate
            owner.slotDelegate = self
            journal?("delegate \(delegate.map { "\(type(of: $0))" } ?? "cleared")")
        }
    }

    /// Let `delegate` go on a later turn of the main queue.
    private func releaseLater(_ delegate: AnyObject?) {
        guard let delegate else { return }
        DispatchQueue.main.async { withExtendedLifetime(delegate) {} }
    }

    override func responds(to selector: Selector!) -> Bool {
        super.responds(to: selector) || (appDelegate?.responds(to: selector) ?? false)
    }

    // Send the app's delegate each call the proxy does not implement, whatever the delegate answers now: the platform
    // calls the methods the proxy answered when it was set
    override func forwardingTarget(for selector: Selector!) -> Any? { appDelegate }
}

/// A platform object whose `delegate` slot Exact keeps (`ForwardingDelegate`).
protocol DelegateSlotOwner: NSObject {
    /// The object's own `delegate`.
    var slotDelegate: AnyObject? { get set }
}

#if os(macOS)
extension NSToolbar: DelegateSlotOwner {
    var slotDelegate: AnyObject? {
        get { delegate }
        set { delegate = newValue as? NSToolbarDelegate }
    }
}
#else
extension UINavigationController: DelegateSlotOwner {
    var slotDelegate: AnyObject? {
        get { delegate }
        set { delegate = newValue as? UINavigationControllerDelegate }
    }
}

extension UITabBarController: DelegateSlotOwner {
    var slotDelegate: AnyObject? {
        get { delegate }
        set { delegate = newValue as? UITabBarControllerDelegate }
    }
}
#endif
