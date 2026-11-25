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
        /// Its view's bounds converted into its window's coordinates, a bounding rectangle under a transform
        /// (D9), `nil` out of a window; its insets stay in the view's own coordinates.
        package var frame: CGRect?
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
        package static let tabContent: UInt8 = 4
    }

    /// The owner view's raw `safeAreaInsets`; `nil` while it is out of a window (D3, D9).
    package var band: UIEdgeInsets?
    /// Exact's writes to the owner's controller and their effects, in recorded order (D11).
    package var contributions: [ContributionsIOS.Entry] = []
    /// The owner's containment ancestry, its controller first (D9, "The native chain").
    package var chain: [ChainEntry] = []
    package var relations: [Related] = []
    package var margins: Margins?
    /// What the host knows of the keyboard (D7), the kernel's `KeyboardReport`.
    package var keyboard: KeyboardReport = .unreported

    package enum KeyboardReport: Equatable {
        /// The owner's window has no keyboard sampler, which comes with its first keyboard editor,
        /// or the owner is out of a window.
        case unreported
        case dismissed
        /// The keyboard in the owner's view, from its origin, and whether it is docked: its window's
        /// sample (`KeyboardSampleIOS`) converted into the view as it is now.
        case shown(KeyboardSampleIOS.Sample)

        package var shown: KeyboardSampleIOS.Sample? {
            if case .shown(let sample) = self { return sample }
            return nil
        }
    }
    /// The owner's segments (D16): its active division regions split as the document's
    /// are (`Segments.split`); `nil` where it lies on one.
    var segments: ViewportFold?
    /// UIKit's bar layout region on each edge of the owner's view, top, right, bottom, left (D12,
    /// "Region"): `nil` before iOS 27.1, which has none, and out of a window.
    package var barRegions: [BarRegion?] = [nil, nil, nil, nil]
    /// The owner's chrome (D6), `nil` out of a window.
    package var chrome: Chrome?
    /// The owner view's map into its window from its bounds' origin, where its block's space starts
    /// (D6's `"window"` space): UIKit's conversion of that origin and the points a point right of
    /// and below it, so a sheet UIKit scales back maps scaled; `nil` out of a window.
    package var windowMap: CGAffineTransform?

    /// What the owner's chrome is: its view's vertical bar edge, as the kernel's
    /// `VerticalBarEdge` takes it, `nil` before iOS 27.1; and whether the sidebar of the tab
    /// controller holding it is available, which is not whether it shows, `nil` before iOS 27 and
    /// where no tab controller holds it.
    package struct Chrome: Equatable {
        package var verticalBarEdge: Int?
        package var sidebarAvailable: Bool?
    }

    /// A bar layout region on one edge: how far in from that edge it begins, and how far in from
    /// the two edges beside it it runs, left and right for the top or bottom, top and bottom for a side.
    package struct BarRegion: Equatable {
        package var distance: CGFloat
        package var sides: (CGFloat, CGFloat)
        package static func == (a: Self, b: Self) -> Bool { a.distance == b.distance && a.sides == b.sides }
    }

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
        // The tab controller containing the owner, whose guide clears its tab bar or sidebar
        let tabs = sequence(first: controller, next: \.parent).lazy.compactMap { $0 as? UITabBarController }.first
        if #available(iOS 26.0, *), let tabs {
            update.relations.append(Related(relation: Relation.tabContent, target: target(tabs.contentLayoutGuide)))
        }
        // Out of a window the guides were never laid out: no margins, as there is no band
        guard view.window != nil else { return update }
        let minimum = controller.systemMinimumLayoutMargins
        let rtl = view.effectiveUserInterfaceLayoutDirection == .rightToLeft
        update.margins = Margins(
            root: view.layoutMargins, plain: UIView().layoutMargins,
            minimum: UIEdgeInsets(top: minimum.top, left: rtl ? minimum.trailing : minimum.leading,
                                  bottom: minimum.bottom, right: rtl ? minimum.leading : minimum.trailing),
            readableWidth: ReadableWidthIOS.of(in: session.flatMap { $0.window === view.window ? $0 : nil } ?? view))
        // The regions are in the view's bounds; the segments and the keyboard are reported from its origin
        let origin = view.bounds.origin
        if let window = view.window, let sampler = KeyboardSampleIOS.existing(in: window) {
            update.keyboard = .dismissed
            if var keys = sampler.sample {
                keys.frame = view.convert(keys.frame, from: window).offsetBy(dx: -origin.x, dy: -origin.y)
                update.keyboard = .shown(keys)
            }
        }
        let active = (ReservedRegions.divisions(of: view) ?? []).filter(\.active).map { $0.frame.offsetBy(dx: -origin.x, dy: -origin.y) }
        let fold = Segments.split(viewport: view.bounds.size, dividers: active)
        update.segments = fold.cols * fold.rows > 1 ? fold : nil
        if #available(iOS 27.1, *) { update.barRegions = barRegions(of: view) }
        var sidebar: Bool?
        if #available(iOS 27.0, *), let tabs { sidebar = tabs.sidebar.isAvailable }
        update.chrome = Chrome(verticalBarEdge: view.traitCollection.verticalBarEdgeRaw, sidebarAvailable: sidebar)
        let at = view.convert(origin, to: nil)
        let right = view.convert(CGPoint(x: origin.x + 1, y: origin.y), to: nil)
        let below = view.convert(CGPoint(x: origin.x, y: origin.y + 1), to: nil)
        update.windowMap = CGAffineTransform(a: right.x - at.x, b: right.y - at.y, c: below.x - at.x, d: below.y - at.y, tx: at.x, ty: at.y)
        return update
    }

    /// `view`'s bar layout regions, read after the ledger's writes, so each lies past the owner's own
    /// additional insets and safe-area padding as UIKit places it on the safe area they make (measured
    /// on an iPhone Duo, iOS 27.1). The extent changes only the region's depth, which a bar's slot does
    /// not take from it: UIKit gives a 50- and a 120-point bottom region the same insets.
    @available(iOS 27.1, *)
    private static func barRegions(of view: UIView) -> [BarRegion?] {
        let edges: [UIRectEdge] = [.top, .right, .bottom, .left]
        return edges.map { edge in
            let i = view.edgeInsets(for: .bar(onEdge: edge, extent: 50))
            switch edge {
            case .top: return BarRegion(distance: i.top, sides: (i.left, i.right))
            case .right: return BarRegion(distance: i.right, sides: (i.top, i.bottom))
            case .bottom: return BarRegion(distance: i.bottom, sides: (i.left, i.right))
            default: return BarRegion(distance: i.left, sides: (i.top, i.bottom))
            }
        }
    }

    /// `guide` as a target (D9's `tab-content`): its edges' distances from its owning view's
    /// bounds, in that view's coordinates, with that view's frame in its window; unavailable while
    /// that view is out of a window.
    static func target(_ guide: UILayoutGuide) -> Target {
        guard let view = guide.owningView, view.window != nil else { return Target(id: TargetIds.id(of: guide), insets: nil) }
        let (b, g) = (view.bounds, guide.layoutFrame)
        let insets = UIEdgeInsets(top: g.minY - b.minY, left: g.minX - b.minX, bottom: b.maxY - g.maxY, right: b.maxX - g.maxX)
        return Target(id: TargetIds.id(of: guide), insets: insets, frame: view.convert(b, to: nil))
    }

    /// `view` as a target, unavailable while it is out of a window; a window is in itself,
    /// though its `window` is `nil`.
    package static func target(_ view: UIView?) -> Target {
        guard let view else { return Target(id: 0, insets: nil) }
        let attached = view is UIWindow || view.window != nil
        return Target(id: TargetIds.id(of: view), insets: attached ? view.safeAreaInsets : nil,
                      frame: attached ? view.convert(view.bounds, to: nil) : nil)
    }

    /// The update in the kernel's wire form (`OwnerUpdate::encode`): each field in order, an
    /// optional one after a presence byte, a list after its `u32` length, little-endian;
    /// insets top, right, bottom, left. A contribution is its node, its operation and its
    /// effect. The keyboard is a byte, 0 unreported, 1 dismissed or 2 shown, and a shown one's frame,
    /// x, y, width, height, and a docked byte. Each edge's bar
    /// region, top, right, bottom, left, follows the segments, and the chrome, its vertical bar
    /// edge and its sidebar's availability each optional, follows them, and then the window map, its
    /// six terms in `CGAffineTransform`'s order.
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
        switch keyboard {
        case .unreported: u8(0)
        case .dismissed: u8(1)
        case .shown(let k):
            u8(2)
            [k.frame.minX, k.frame.minY, k.frame.width, k.frame.height].forEach(f32)
            u8(k.docked ? 1 : 0)
        }
        optional(segments) { fold in
            u8(UInt8(fold.cols))
            u8(UInt8(fold.rows))
            for r in fold.rects { [r.minX, r.minY, r.width, r.height].forEach(f32) }
        }
        for region in barRegions {
            optional(region) { r in [r.distance, r.sides.0, r.sides.1].forEach(f32) }
        }
        optional(chrome) { c in
            optional(c.verticalBarEdge) { u8(UInt8($0)) }
            optional(c.sidebarAvailable) { u8($0 ? 1 : 0) }
        }
        optional(windowMap) { m in [m.a, m.b, m.c, m.d, m.tx, m.ty].forEach(f32) }
        return out
    }
}

extension OwnerUpdateIOS {
    /// A target in the kernel's wire form: its id and generation (`u64` each), its insets after
    /// a presence byte, top, right, bottom, left, and its frame after one, x, y, width, height.
    static func encode(target t: Target) -> Data {
        var out = Data()
        func u64(_ v: UInt64) { withUnsafeBytes(of: v.littleEndian) { out.append(contentsOf: $0) } }
        func f32s(_ values: [CGFloat]) {
            for v in values { withUnsafeBytes(of: Float(v).bitPattern.littleEndian) { out.append(contentsOf: $0) } }
        }
        u64(t.id)
        u64(t.generation)
        out.append(t.insets == nil ? 0 : 1)
        if let i = t.insets { f32s([i.top, i.right, i.bottom, i.left]) }
        out.append(t.frame == nil ? 0 : 1)
        if let f = t.frame { f32s([f.minX, f.minY, f.width, f.height]) }
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

/// The host's identities for native targets: one per view or layout guide for its life, never
/// reused, so a new one at a freed one's address is a new target (D9).
package enum TargetIds {
    nonisolated(unsafe) private static var next: UInt64 = 1
    nonisolated(unsafe) private static var key: UInt8 = 0

    package static func id(of object: NSObject) -> UInt64 {
        if let id = objc_getAssociatedObject(object, &key) as? NSNumber { return id.uint64Value }
        let id = next
        next += 1
        objc_setAssociatedObject(object, &key, NSNumber(value: id), .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        return id
    }
}
#endif
