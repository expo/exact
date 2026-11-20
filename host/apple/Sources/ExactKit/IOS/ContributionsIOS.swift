#if os(iOS) || os(tvOS)
import ObjectiveC
import UIKit

/// Exact's own writes to one controller's `additionalSafeAreaInsets` (LLP 1123 D11, "Provenance"),
/// each with its identity and its marginal effect on the controller's view, which the owner update
/// reports so the owner's incoming band leaves them out. What the app wrote there stays, as the
/// baseline the contributions are measured against.
///
/// An effect is the view's insets with the contribution less without it, both with the
/// contributions recorded before it, as UIKit gives them. UIKit decides how a write combines with
/// what the view inherits: a negative one reduces a window root's insets, below 0 past them,
/// while a plain child filling its parent never reads less than its superview gives it, a floor
/// that can hold all or part of a write. So the ledger measures by writing each prefix in order,
/// in one call, before control returns to the run loop, ending at the full write. UIKit's
/// scrollers end at rest where that last write puts them (LLP 1123's evidence row "Stepwise
/// insets"), and Exact's own reactions wait for the settled result (`SafeAreaSteps`); an app's own
/// callbacks hear each step.
package final class ContributionsIOS {
    /// A contribution's identity on its controller: the declaring node and what it writes, by its
    /// index in the kernel's `Operation::ALL`.
    package struct Identity: Hashable {
        package var node: UInt32
        package var operation: Operation

        package init(node: UInt32, operation: Operation) {
            self.node = node
            self.operation = operation
        }
    }

    /// What a contribution writes, by its index in the kernel's `Operation::ALL`.
    package enum Operation: UInt8 {
        case additionalInsets = 0
        case safeAreaPadding = 1
        case barReservation = 2
        case keyboardInsets = 3
    }

    package struct Entry: Equatable {
        package var identity: Identity
        package var generation: UInt64
        /// What it adds to the controller's additional insets.
        package var insets: UIEdgeInsets
        /// The view's insets with it less without it, as last measured.
        package var effect: UIEdgeInsets = .zero
    }

    /// What the effects were measured against; they stand while it does. The parent container's
    /// bars are among them because under a write UIKit's floor holds, a bar shown or hidden moves
    /// what the child is given with no other input changing.
    private struct Inputs: Equatable {
        var frame: CGRect
        var inherited: UIEdgeInsets
        var raw: UIEdgeInsets
        var app: UIEdgeInsets
        var chrome: [CGRect]
    }

    private weak var controller: UIViewController?
    /// The contributions, in recorded order.
    package private(set) var entries: [Entry] = []
    /// Exact's part of the controller's additional insets as last written; the rest is the app's,
    /// which is taken to add to what it reads.
    private var written: UIEdgeInsets = .zero
    /// What the effects were measured against, `nil` once the contributions change.
    private var measured: Inputs?
    /// Counts the changes to the contributions, so a pass can tell one made during its writes.
    private var revision = 0
    /// Whether the last measurement ended with an app still writing in a callback each write
    /// raised: its effects are the last pass's, kept until an input changes.
    package private(set) var unsettled = false
    /// The generation of each contribution taken away, so a late stale write cannot add it back;
    /// kept for the ledger's life, so a node id used again takes a newer generation.
    private var retired: [Identity: UInt64] = [:]
    /// Whether the writes are under way, the effects half measured, and whether a call during them
    /// owes another pass; the reactions they hold are released after, onto a measured ledger.
    package private(set) var writing = false
    private var owed = false

    private init(controller: UIViewController) {
        self.controller = controller
    }

    nonisolated(unsafe) private static var key: UInt8 = 0

    /// `controller`'s ledger, made on first use.
    package static func of(_ controller: UIViewController) -> ContributionsIOS {
        if let ledger = existing(on: controller) { return ledger }
        let ledger = ContributionsIOS(controller: controller)
        objc_setAssociatedObject(controller, &key, ledger, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        return ledger
    }

    /// `controller`'s ledger, if Exact has written to it.
    package static func existing(on controller: UIViewController) -> ContributionsIOS? {
        objc_getAssociatedObject(controller, &key) as? ContributionsIOS
    }

    /// Record that `identity` adds `insets` at `generation`, after the contributions recorded before
    /// it, or take it away (`nil`); `apply()` writes it. A generation older than the one recorded is
    /// refused (`false`), as a stale write is (D11).
    @discardableResult
    package func set(_ identity: Identity, _ insets: UIEdgeInsets?, generation: UInt64) -> Bool {
        let index = entries.firstIndex { $0.identity == identity }
        let last = index.map { entries[$0].generation } ?? retired[identity]
        if let last, generation < last { return false }
        switch (index, insets) {
        case let (index?, insets?):
            let same = entries[index].insets == insets
            entries[index].insets = insets
            entries[index].generation = generation
            // A newer generation of the same write is the same measurement
            if same { return true }
        case let (index?, nil):
            retired[identity] = generation
            entries.remove(at: index)
        case let (nil, insets?):
            retired[identity] = nil
            entries.append(Entry(identity: identity, generation: generation, insets: insets))
        case (nil, nil):
            retired[identity] = generation
            return true
        }
        measured = nil
        revision += 1
        return true
    }

    /// Write the contributions to the controller and measure each one's effect, unless the
    /// contributions are as they were and nothing the effects were measured against changed: the
    /// view's frame in its window, the insets its superview gives it, its raw insets, the app's own
    /// part and the parent container's bars. The app's part is among them because on a child a
    /// write UIKit's floor holds can leave the raw insets unchanged while its effect moves with
    /// that part. Each prefix is written in recorded order, the app's part first, ending at the
    /// full write. Chrome other than a navigation controller's bars and a tab bar is not seen under
    /// such a write, and an effect measured under it stands until one of the inputs changes.
    package func apply() {
        // A call from a callback a write raises is owed, not dropped: the pass runs again once the
        // writes end, from what the call recorded
        guard !writing else { return owed = true }
        repeat {
            owed = false
            measure()
        } while owed
    }

    private func measure() {
        guard let controller, let view = controller.viewIfLoaded else { return }
        let app = Self.minus(controller.additionalSafeAreaInsets, written)
        guard inputs(controller, view, app: app) != measured else { return }
        guard !entries.isEmpty || written != .zero else {
            measured = inputs(controller, view, app: app)
            return
        }
        let began = revision
        SafeAreaSteps.stepping {
            writing = true
            defer { writing = false }
            // An app's write in a callback a write raises moves the baseline every effect is
            // measured against, so the pass is made again from it, once; an app that writes again
            // leaves the last pass's effects, unsettled, which nothing measures again until an
            // input changes
            for pass in 0..<2 {
                let (settled, app) = steps(controller, view)
                written = Self.minus(controller.additionalSafeAreaInsets, app)
                // Measure again once the writes end where a callback changed the contributions
                guard revision == began else {
                    measured = nil
                    owed = true
                    return
                }
                if settled || pass == 1 {
                    measured = inputs(controller, view, app: app)
                    unsettled = !settled
                    return
                }
            }
        }
    }

    /// One pass: the app's part, then each prefix, each effect read after its write. Whether no
    /// app wrote during it, and the app's part it ended with.
    private func steps(_ controller: UIViewController, _ view: UIView) -> (Bool, UIEdgeInsets) {
        var app = Self.minus(controller.additionalSafeAreaInsets, written)
        var total = app, settled = true
        let write = { (value: UIEdgeInsets) in
            controller.additionalSafeAreaInsets = value
            let now = controller.additionalSafeAreaInsets
            if now != value {
                app = Self.plus(app, Self.minus(now, value))
                settled = false
            }
            total = now
        }
        write(app)
        // Walk the contributions as the pass began, as a callback may change them; `measure()`
        // measures again then
        var before = view.safeAreaInsets, effects: [Identity: UIEdgeInsets] = [:]
        for entry in entries {
            write(Self.plus(total, entry.insets))
            let after = view.safeAreaInsets
            effects[entry.identity] = Self.minus(after, before)
            before = after
        }
        for i in entries.indices {
            entries[i].effect = effects[entries[i].identity] ?? entries[i].effect
        }
        return (settled, app)
    }

    private func inputs(_ controller: UIViewController, _ view: UIView, app: UIEdgeInsets) -> Inputs {
        let frame = { (bar: UIView?) in bar.map { $0.convert($0.bounds, to: nil) } ?? .null }
        let navigation = controller.navigationController
        var chrome = [frame(navigation.flatMap { $0.isNavigationBarHidden ? nil : $0.navigationBar })]
        #if os(iOS)
        // Read the toolbar only where it shows, as UIKit makes it on first read
        chrome.append(frame(navigation.flatMap { $0.isToolbarHidden ? nil : $0.toolbar }))
        #endif
        chrome.append(frame(controller.tabBarController.flatMap { Self.tabBarHidden($0) ? nil : $0.tabBar }))
        return Inputs(frame: view.convert(view.bounds, to: nil), inherited: view.superview?.safeAreaInsets ?? .zero,
                      raw: view.safeAreaInsets, app: app, chrome: chrome)
    }

    /// Whether `tabs` hides its tab bar, as `setTabBarHidden` does on iOS 18 and tvOS 18 or as
    /// the bar's own `isHidden` does.
    private static func tabBarHidden(_ tabs: UITabBarController) -> Bool {
        if #available(iOS 18, tvOS 18, *), tabs.isTabBarHidden { return true }
        return tabs.tabBar.isHidden
    }

    private static func plus(_ a: UIEdgeInsets, _ b: UIEdgeInsets) -> UIEdgeInsets {
        UIEdgeInsets(top: a.top + b.top, left: a.left + b.left, bottom: a.bottom + b.bottom, right: a.right + b.right)
    }

    private static func minus(_ a: UIEdgeInsets, _ b: UIEdgeInsets) -> UIEdgeInsets {
        UIEdgeInsets(top: a.top - b.top, left: a.left - b.left, bottom: a.bottom - b.bottom, right: a.right - b.right)
    }
}

/// Exact's reactions to a safe-area change, held while a ledger writes in steps (LLP 1123 D11), so
/// each reads only the settled result. The stepping's end releases them: each held reaction runs
/// once, in the order first held, with what its last call carried.
package enum SafeAreaSteps {
    nonisolated(unsafe) private static var depth = 0
    nonisolated(unsafe) private static var held: [(ObjectIdentifier, () -> Void)] = []

    /// Run `reaction` for `owner` now, or once when the stepping under way ends.
    package static func react(_ owner: AnyObject, _ reaction: @escaping () -> Void) {
        guard depth > 0 else { return reaction() }
        let id = ObjectIdentifier(owner)
        if let i = held.firstIndex(where: { $0.0 == id }) {
            held[i].1 = reaction
        } else {
            held.append((id, reaction))
        }
    }

    /// Run `steps`, holding the reactions they cause until the outermost stepping ends.
    package static func stepping(_ steps: () -> Void) {
        depth += 1
        steps()
        depth -= 1
        guard depth == 0 else { return }
        // The release runs each held reaction once, one at a time, the depth raised, so a reaction
        // that steps again holds what its steps raise for this same loop
        depth += 1
        defer { depth -= 1 }
        while !held.isEmpty { held.removeFirst().1() }
    }
}
#endif
