#if os(iOS)
import ObjectiveC
import UIKit

/// One content owner's update (LLP 1123 D2) as UIKit gives it at one layout pass, sampled
/// from the owner's controller and its view, and its encoding, the kernel's
/// (`OwnerUpdate::encode`), which `exact_host_covers` kind 21 carries. The OS decides every
/// inset, margin and width, each read from UIKit as it gives it; the host derives only the
/// column kind (a split's child), the system minimum's sides from its view's direction, and the
/// segments from the division regions, as the document's are split.
package struct OwnerUpdateIOS: Equatable {
    /// A native view whose raw `safeAreaInsets` a box can read (D9), by an identity the
    /// host gives it once and never reuses.
    package struct Target: Equatable {
        package var id: UInt64
        package var generation: UInt64 = 1
        /// In the view's own coordinates; `nil` while it is out of a window.
        package var insets: UIEdgeInsets?
    }

    /// An entry of the native chain: a controller's view and the kinds it is.
    package struct ChainEntry: Equatable {
        package var target: Target
        package var kinds: UInt8
    }

    /// A relational target (D9), by its index in the kernel's `Relation::ALL`.
    package struct Related: Equatable {
        package var relation: UInt8
        package var target: Target
    }

    /// The margins UIKit gives the owner (D8), each physical.
    package struct Margins: Equatable {
        /// The owner view's `layoutMargins`.
        package var root: UIEdgeInsets
        /// A plain view's own `layoutMargins`, which UIKit gives each view by default.
        package var plain: UIEdgeInsets
        /// The controller's `systemMinimumLayoutMargins`, by the view's direction.
        package var minimum: UIEdgeInsets
        /// The width UIKit keeps text to, before any view's margins clamp it (`ReadableWidthIOS`).
        package var readableWidth: CGFloat
    }

    /// The kernel's `Kind::bit`s.
    package enum Kind {
        package static let navigation: UInt8 = 1 << 2
        package static let tab: UInt8 = 1 << 3
        package static let split: UInt8 = 1 << 4
        package static let column: UInt8 = 1 << 5
    }

    /// The kernel's `Relation::ALL` indices this host reports.
    package enum Relation {
        package static let presenter: UInt8 = 0
        package static let session: UInt8 = 1
        package static let root: UInt8 = 2
        package static let window: UInt8 = 3
    }

    /// The owner view's raw `safeAreaInsets`; `nil` while it is out of a window (D3, D9).
    package var band: UIEdgeInsets?
    /// Exact's writes to the owner's controller and their effects, in recorded order (D11).
    package var contributions: [ContributionsIOS.Entry] = []
    /// The owner's containment ancestry, its controller first (D9, "The native chain").
    package var chain: [ChainEntry] = []
    package var relations: [Related] = []
    package var margins: Margins?
    /// The owner's segments (D16): its active division regions split as the document's
    /// are (`Segments.split`); `nil` where it lies on one.
    var segments: ViewportFold?

    /// The update of the owner whose controller is `controller` and whose view is `view` (its
    /// surface), in a session shown by `session`. `controller` manages `view`: for a route shown
    /// in a boot surface, the controller that surface belongs to. Exact's writes to the controller
    /// are applied first, so the band and their effects describe the same native layout (D11); it
    /// is not to be called from a callback those writes raise: each trigger of a sample, D2's
    /// layout, trait, keyboard and hinge triggers among them, reaches it through
    /// `SafeAreaSteps.react`, so one raised during the writes runs after them.
    package static func sample(controller: UIViewController, view: UIView, session: UIView?) -> OwnerUpdateIOS {
        var update = OwnerUpdateIOS()
        // The reactions the ledger's writes raise wait until the band is read, so none runs between
        // measuring the effects and reading what they describe
        SafeAreaSteps.stepping { update = read(controller: controller, view: view, session: session) }
        return update
    }

    private static func read(controller: UIViewController, view: UIView, session: UIView?) -> OwnerUpdateIOS {
        var update = OwnerUpdateIOS()
        if let ledger = ContributionsIOS.existing(on: controller) {
            // Assert, in debug builds, that no sample comes from a callback the ledger's writes raise,
            // which would read its effects half measured; a reaction they held reads them settled
            assert(!ledger.writing, "an owner sampled during its ledger's writes")
            ledger.apply()
            update.contributions = ledger.entries
        }
        update.band = view.window == nil ? nil : view.safeAreaInsets
        for c in sequence(first: controller, next: \.parent) {
            var kinds: UInt8 = 0
            if c is UINavigationController { kinds |= Kind.navigation }
            if c is UITabBarController { kinds |= Kind.tab }
            if c is UISplitViewController { kinds |= Kind.split }
            if c.parent is UISplitViewController { kinds |= Kind.column }
            let shown = c === controller ? view : c.viewIfLoaded
            update.chain.append(ChainEntry(target: target(shown), kinds: kinds))
        }
        // A presented controller's chain ends at it, and its presenter is a relation: UIKit gives a
        // contained controller its presented ancestor's presenter
        let related: [(UInt8, UIView?)] = [
            (Relation.presenter, controller.presentingViewController?.viewIfLoaded),
            (Relation.session, session),
            (Relation.root, view.window?.rootViewController?.viewIfLoaded),
            (Relation.window, view.window),
        ]
        update.relations = related.compactMap { r, v in v.map { Related(relation: r, target: target($0)) } }
        // Out of a window the guides were never laid out: no margins, as there is no band
        guard view.window != nil else { return update }
        let minimum = controller.systemMinimumLayoutMargins
        let rtl = view.effectiveUserInterfaceLayoutDirection == .rightToLeft
        update.margins = Margins(
            root: view.layoutMargins, plain: UIView().layoutMargins,
            minimum: UIEdgeInsets(top: minimum.top, left: rtl ? minimum.trailing : minimum.leading,
                                  bottom: minimum.bottom, right: rtl ? minimum.leading : minimum.trailing),
            readableWidth: ReadableWidthIOS.of(in: session.flatMap { $0.window === view.window ? $0 : nil } ?? view))
        // The regions are in the view's bounds, the segments from its origin
        let origin = view.bounds.origin
        let active = (ReservedRegions.divisions(of: view) ?? []).filter(\.active).map { $0.frame.offsetBy(dx: -origin.x, dy: -origin.y) }
        let fold = Segments.split(viewport: view.bounds.size, dividers: active)
        update.segments = fold.cols * fold.rows > 1 ? fold : nil
        return update
    }

    /// `view` as a target, unavailable while it is out of a window; a window is in itself,
    /// though its `window` is `nil`.
    package static func target(_ view: UIView?) -> Target {
        guard let view else { return Target(id: 0, insets: nil) }
        let attached = view is UIWindow || view.window != nil
        return Target(id: TargetIds.id(of: view), insets: attached ? view.safeAreaInsets : nil)
    }

    /// The update in the kernel's wire form (`OwnerUpdate::encode`): each field in order, an
    /// optional one after a presence byte, a list after its `u32` length, little-endian;
    /// insets top, right, bottom, left. A contribution is its node, its operation and its
    /// effect. The keyboard is absent: its executor samples it.
    package func encode() -> Data {
        var out = Data()
        func u8(_ v: UInt8) { out.append(v) }
        func u32(_ v: UInt32) { withUnsafeBytes(of: v.littleEndian) { out.append(contentsOf: $0) } }
        func u64(_ v: UInt64) { withUnsafeBytes(of: v.littleEndian) { out.append(contentsOf: $0) } }
        func f32(_ v: CGFloat) { u32(Float(v).bitPattern) }
        func edges(_ i: UIEdgeInsets) { [i.top, i.right, i.bottom, i.left].forEach(f32) }
        func optional<T>(_ value: T?, _ each: (T) -> Void) {
            u8(value == nil ? 0 : 1)
            if let value { each(value) }
        }
        func target(_ t: Target) { out.append(Self.encode(target: t)) }
        optional(band, edges)
        u32(UInt32(contributions.count))
        for c in contributions {
            u32(c.identity.node)
            u8(c.identity.operation.rawValue)
            edges(c.effect)
        }
        u32(UInt32(chain.count))
        for entry in chain {
            target(entry.target)
            u8(entry.kinds)
        }
        u32(UInt32(relations.count))
        for r in relations {
            u8(r.relation)
            target(r.target)
        }
        optional(margins) { m in
            edges(m.root)
            edges(m.plain)
            edges(m.minimum)
            f32(m.readableWidth)
        }
        u8(0)
        optional(segments) { fold in
            u8(UInt8(fold.cols))
            u8(UInt8(fold.rows))
            for r in fold.rects { [r.minX, r.minY, r.width, r.height].forEach(f32) }
        }
        return out
    }
}

extension OwnerUpdateIOS {
    /// A target in the kernel's wire form: its id and generation (`u64` each) and its
    /// insets after a presence byte, top, right, bottom, left.
    static func encode(target t: Target) -> Data {
        var out = Data()
        func u64(_ v: UInt64) { withUnsafeBytes(of: v.littleEndian) { out.append(contentsOf: $0) } }
        u64(t.id)
        u64(t.generation)
        out.append(t.insets == nil ? 0 : 1)
        if let i = t.insets {
            for v in [i.top, i.right, i.bottom, i.left] {
                withUnsafeBytes(of: Float(v).bitPattern.littleEndian) { out.append(contentsOf: $0) }
            }
        }
        return out
    }
}

/// The width UIKit keeps text to (LLP 1123 D8), read from a hidden view that no margins clamp:
/// wider than any readable width, with no margins of its own and none from the safe area, a
/// subview of a view Exact owns in the owner's window (the session view, else the owner view; a
/// view in no window reads 0), and never the presenter's root. An owner view's own
/// `readableContentGuide` is that width clamped to the owner's margins already, and the width
/// follows the system's text size, not a trait override (LLP 1123's evidence row "Readable
/// width"). It is laid out where UIKit marked it before each read. Whether the guide follows a
/// text size change while the app runs is not measured, the probe's readings having stopped when
/// the size changed; nor is the cost of the layout engine a guide gives its window.
enum ReadableWidthIOS {
    nonisolated(unsafe) private static var key: UInt8 = 0

    /// `host`'s measuring view, if one was made.
    static func view(in host: UIView) -> UIView? {
        (objc_getAssociatedObject(host, &key) as? UIView).flatMap { $0.superview === host ? $0 : nil }
    }

    /// The width as `host`'s measuring view gives it, the view made on first use.
    static func of(in host: UIView) -> CGFloat {
        let measure: UIView
        if let held = view(in: host) {
            measure = held
        } else {
            measure = UIView(frame: CGRect(x: 0, y: 0, width: 10_000, height: 10))
            measure.isHidden = true
            measure.isUserInteractionEnabled = false
            measure.insetsLayoutMarginsFromSafeArea = false
            measure.preservesSuperviewLayoutMargins = false
            measure.layoutMargins = .zero
            host.insertSubview(measure, at: 0)
            objc_setAssociatedObject(host, &key, measure, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        }
        measure.layoutIfNeeded()
        return measure.readableContentGuide.layoutFrame.width
    }
}

/// The host's identities for native targets: one per view for its life, never reused, so a
/// new view at a freed view's address is a new target (D9).
package enum TargetIds {
    nonisolated(unsafe) private static var next: UInt64 = 1
    nonisolated(unsafe) private static var key: UInt8 = 0

    package static func id(of view: UIView) -> UInt64 {
        if let id = objc_getAssociatedObject(view, &key) as? NSNumber { return id.uint64Value }
        let id = next
        next += 1
        objc_setAssociatedObject(view, &key, NSNumber(value: id), .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        return id
    }
}
#endif
