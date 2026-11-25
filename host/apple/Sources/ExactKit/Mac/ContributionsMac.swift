#if os(macOS)
import AppKit
import ObjectiveC

/// Exact's own writes to one view's `additionalSafeAreaInsets` on AppKit (LLP 1123 D11,
/// "Provenance"), each with its identity; what the app wrote there stays, as the baseline the
/// contributions add to. AppKit adds a view's additional insets to what the view inherits, of any
/// sign and with no floor (the evidence row "AppKit's additional insets"), so each contribution's
/// effect on the view is what it adds, and the ledger writes their sum beside the app's part with
/// nothing to measure, where UIKit's ledger measures each prefix (`ContributionsIOS`).
package final class ContributionsMac {
    package typealias Identity = ContributionIdentity

    package struct Entry {
        package var identity: Identity
        package var generation: UInt64
        /// What it adds to the view's additional insets, which is its effect on the view.
        package var insets: NSEdgeInsets
    }

    private weak var view: NSView?
    /// The contributions, in recorded order.
    package private(set) var entries: [Entry] = []
    /// Exact's part of the view's additional insets as last written; the rest is the app's, which is
    /// taken to add to what it reads.
    private var written = NSEdgeInsetsZero
    /// The generation of each contribution taken away, so a late stale write cannot add it back;
    /// kept for the ledger's life, so a node id used again takes a newer generation.
    private var retired: [Identity: UInt64] = [:]

    private init(view: NSView) {
        self.view = view
    }

    nonisolated(unsafe) private static var key: UInt8 = 0

    /// `view`'s ledger, made on first use.
    package static func of(_ view: NSView) -> ContributionsMac {
        if let ledger = existing(on: view) { return ledger }
        let ledger = ContributionsMac(view: view)
        objc_setAssociatedObject(view, &key, ledger, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        return ledger
    }

    /// `view`'s ledger, if Exact has written to it.
    package static func existing(on view: NSView) -> ContributionsMac? {
        objc_getAssociatedObject(view, &key) as? ContributionsMac
    }

    /// What `identity` adds, as last recorded; zero where it adds nothing.
    package func insets(of identity: Identity) -> NSEdgeInsets {
        entries.first { $0.identity == identity }?.insets ?? NSEdgeInsetsZero
    }

    /// Record that `identity` adds `insets`, none where they are zero, at a generation newer than its
    /// last, and write the contributions, unless that is what it adds already: a write the host makes
    /// of its own accord.
    package func write(_ identity: Identity, _ insets: NSEdgeInsets) {
        guard !Self.same(insets, self.insets(of: identity)) else { return }
        let last = entries.first { $0.identity == identity }?.generation ?? retired[identity] ?? 0
        set(identity, Self.same(insets, NSEdgeInsetsZero) ? nil : insets, generation: last + 1)
        apply()
    }

    /// Record that `identity` adds `insets` at `generation`, after the contributions recorded before
    /// it, or take it away (`nil`); `apply()` writes it. A generation older than the one recorded is
    /// refused (`false`), as a stale write is (D11).
    @discardableResult
    package func set(_ identity: Identity, _ insets: NSEdgeInsets?, generation: UInt64) -> Bool {
        let index = entries.firstIndex { $0.identity == identity }
        if let last = index.map({ entries[$0].generation }) ?? retired[identity], generation < last { return false }
        switch (index, insets) {
        case let (index?, insets?):
            entries[index].insets = insets
            entries[index].generation = generation
        case let (index?, nil):
            retired[identity] = generation
            entries.remove(at: index)
        case let (nil, insets?):
            retired[identity] = nil
            entries.append(Entry(identity: identity, generation: generation, insets: insets))
        case (nil, nil):
            retired[identity] = generation
        }
        return true
    }

    /// Write the app's part and the contributions' sum to the view, skipped where that is what it has.
    package func apply() {
        guard let view else { return }
        let app = Self.minus(view.additionalSafeAreaInsets, written)
        let ours = entries.reduce(NSEdgeInsetsZero) { Self.plus($0, $1.insets) }
        written = ours
        let next = Self.plus(app, ours)
        if !Self.same(view.additionalSafeAreaInsets, next) { view.additionalSafeAreaInsets = next }
    }

    private static func same(_ a: NSEdgeInsets, _ b: NSEdgeInsets) -> Bool {
        a.top == b.top && a.left == b.left && a.bottom == b.bottom && a.right == b.right
    }

    private static func plus(_ a: NSEdgeInsets, _ b: NSEdgeInsets) -> NSEdgeInsets {
        NSEdgeInsets(top: a.top + b.top, left: a.left + b.left, bottom: a.bottom + b.bottom, right: a.right + b.right)
    }

    private static func minus(_ a: NSEdgeInsets, _ b: NSEdgeInsets) -> NSEdgeInsets {
        NSEdgeInsets(top: a.top - b.top, left: a.left - b.left, bottom: a.bottom - b.bottom, right: a.right - b.right)
    }
}
#endif
