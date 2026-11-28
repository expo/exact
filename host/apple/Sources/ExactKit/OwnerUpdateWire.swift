// One content owner's update (LLP 1123 D2) in the kernel's wire form (`OwnerUpdate::encode`), which `exact_host_covers`
// kind 21 carries: each Apple host fills it from its own toolkit (`OwnerUpdateIOS`, `OwnerUpdateMac`) and this encodes it
// once, so the two cannot drift from the kernel's order.
import CoreGraphics
import Foundation

package struct OwnerUpdateWire: Equatable {
    /// Lengths on the four edges, physical.
    package struct Edges: Equatable {
        package var top: CGFloat, right: CGFloat, bottom: CGFloat, left: CGFloat
        package init(top: CGFloat, right: CGFloat, bottom: CGFloat, left: CGFloat) {
            (self.top, self.right, self.bottom, self.left) = (top, right, bottom, left)
        }
        package static let zero = Edges(top: 0, right: 0, bottom: 0, left: 0)
    }

    /// A native view whose raw safe area a box can read (D9): its identity and generation, its insets in its own
    /// coordinates and its frame in its window's, each `nil` while it is out of a window.
    package struct Target: Equatable {
        package var id: UInt64, generation: UInt64, insets: Edges?, frame: CGRect?
        package init(id: UInt64, generation: UInt64 = 1, insets: Edges?, frame: CGRect?) {
            (self.id, self.generation, self.insets, self.frame) = (id, generation, insets, frame)
        }
    }

    /// One of Exact's writes to the owner's view or controller and its effect (D11).
    package struct Contribution: Equatable {
        package var node: UInt32, operation: UInt8, effect: Edges
        package init(node: UInt32, operation: UInt8, effect: Edges) { (self.node, self.operation, self.effect) = (node, operation, effect) }
    }

    /// The owner's margins (D8): its root view's and a plain view's, and the system minimum and the readable width,
    /// each `nil` where the toolkit has none, as AppKit has neither.
    package struct Margins: Equatable {
        package var root: Edges, plain: Edges, minimum: Edges?, readableWidth: CGFloat?
        package init(root: Edges, plain: Edges, minimum: Edges?, readableWidth: CGFloat?) {
            (self.root, self.plain, self.minimum, self.readableWidth) = (root, plain, minimum, readableWidth)
        }
    }

    package enum Keyboard: Equatable {
        case unreported, dismissed
        case shown(frame: CGRect, docked: Bool)
    }

    package struct Fold: Equatable {
        package var cols: UInt8, rows: UInt8, rects: [CGRect]
        package init(cols: UInt8, rows: UInt8, rects: [CGRect]) { (self.cols, self.rows, self.rects) = (cols, rows, rects) }
    }

    package struct BarRegion: Equatable {
        package var distance: CGFloat, start: CGFloat, end: CGFloat
        package init(distance: CGFloat, start: CGFloat, end: CGFloat) { (self.distance, self.start, self.end) = (distance, start, end) }
    }

    /// The owner's chrome (D6): UIKit's vertical bar edge and sidebar availability, and AppKit's auxiliary areas beside
    /// the screen's camera housing in the owner's space, each `nil` where the toolkit has none.
    package struct Chrome: Equatable {
        package var verticalBarEdge: Int?, sidebarAvailable: Bool?, auxiliaryTopLeft: CGRect?, auxiliaryTopRight: CGRect?
        package init(verticalBarEdge: Int?, sidebarAvailable: Bool?, auxiliaryTopLeft: CGRect? = nil, auxiliaryTopRight: CGRect? = nil) {
            (self.verticalBarEdge, self.sidebarAvailable) = (verticalBarEdge, sidebarAvailable)
            (self.auxiliaryTopLeft, self.auxiliaryTopRight) = (auxiliaryTopLeft, auxiliaryTopRight)
        }
    }

    package struct Region: Equatable {
        package var id: UInt64, kind: UInt8, frame: CGRect, margins: Edges, active: Bool
        package init(id: UInt64, kind: UInt8, frame: CGRect, margins: Edges, active: Bool) {
            (self.id, self.kind, self.frame, self.margins, self.active) = (id, kind, frame, margins, active)
        }
    }

    package var band: Edges?
    package var contributions: [Contribution] = []
    /// The owner's containment ancestry, its own controller or view first, each with the kernel's `Kind::bit`s.
    package var chain: [(target: Target, kinds: UInt8)] = []
    /// Relational targets by their index in the kernel's `Relation::ALL`.
    package var relations: [(relation: UInt8, target: Target)] = []
    package var margins: Margins?
    package var keyboard: Keyboard = .unreported
    package var segments: Fold?
    /// Top, right, bottom, left.
    package var barRegions: [BarRegion?] = [nil, nil, nil, nil]
    package var chrome: Chrome?
    package var windowMap: CGAffineTransform?
    package var configuration: UInt64?
    package var regions: [Region]?

    package init() {}

    package static func == (a: Self, b: Self) -> Bool {
        a.band == b.band && a.contributions == b.contributions && a.margins == b.margins && a.keyboard == b.keyboard
            && a.segments == b.segments && a.barRegions == b.barRegions && a.chrome == b.chrome && a.windowMap == b.windowMap
            && a.configuration == b.configuration && a.regions == b.regions
            && a.chain.map(\.target) == b.chain.map(\.target) && a.chain.map(\.kinds) == b.chain.map(\.kinds)
            && a.relations.map(\.relation) == b.relations.map(\.relation) && a.relations.map(\.target) == b.relations.map(\.target)
    }

    /// The kernel's encoding: the band after a presence byte, top, right, bottom, left, as `f32`s; the contributions
    /// after their count, each its node (`u32`), operation and effect; the chain after its count, each its target and
    /// kinds; the relations after theirs, each its relation and target; the margins after a presence byte, the root
    /// view's and a plain view's, then the minimum and the readable width, each after its own. The keyboard is a byte, 0 unreported,
    /// 1 dismissed or 2 shown, and a shown one's frame, x, y, width, height, and a docked byte. Then the segments,
    /// each edge's bar region, the chrome, its vertical bar edge, its sidebar's availability and its auxiliary areas each
    /// optional, the
    /// window map, its six terms in `CGAffineTransform`'s order, the configuration's identity (`u64`) and the reserved
    /// regions after their count, each its identity, kind, frame, margins and an active byte.
    package func encode() -> Data {
        var w = WireOut()
        w.optional(band) { $0.edges($1) }
        w.u32(UInt32(contributions.count))
        for c in contributions {
            w.u32(c.node)
            w.u8(c.operation)
            w.edges(c.effect)
        }
        w.targets(chain: chain, relations: relations)
        w.optional(margins) { w, m in
            w.edges(m.root)
            w.edges(m.plain)
            w.optional(m.minimum) { $0.edges($1) }
            w.optional(m.readableWidth) { $0.f32($1) }
        }
        w.keyboard(keyboard)
        w.optional(segments) { w, fold in
            w.u8(fold.cols)
            w.u8(fold.rows)
            fold.rects.forEach { w.rect($0) }
        }
        for region in barRegions {
            w.optional(region) { w, r in [r.distance, r.start, r.end].forEach { w.f32($0) } }
        }
        w.optional(chrome) { w, c in
            w.optional(c.verticalBarEdge) { $0.u8(UInt8($1)) }
            w.optional(c.sidebarAvailable) { $0.u8($1 ? 1 : 0) }
            w.optional(c.auxiliaryTopLeft) { $0.rect($1) }
            w.optional(c.auxiliaryTopRight) { $0.rect($1) }
        }
        w.windowMap(windowMap)
        w.optional(configuration) { $0.u64($1) }
        w.regions(regions)
        return w.out
    }

    /// A target in the kernel's wire form: its id and generation (`u64` each), its insets after a presence byte,
    /// top, right, bottom, left, and its frame after one, x, y, width, height.
    package static func encode(target t: Target) -> Data {
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

/// The session's update (LLP 1123 D2) in the kernel's wire form (`SessionUpdate::encode`), which `exact_host_covers`
/// kind 29 carries: what the host reports of the session view, in the document's space, which the boxes outside every
/// owner read.
package struct SessionUpdateWire: Equatable {
    package var band: OwnerUpdateWire.Edges = .zero
    package var chain: [(target: OwnerUpdateWire.Target, kinds: UInt8)] = []
    package var relations: [(relation: UInt8, target: OwnerUpdateWire.Target)] = []
    package var keyboard: OwnerUpdateWire.Keyboard = .unreported
    /// The keyboard policy (`interactive-widget`), in the kernel's `KeyboardPolicy::ALL` order; `nil` reports none.
    package enum KeyboardPolicy: UInt8 { case resizesContent, resizesVisual, overlaysContent }
    package var keyboardPolicy: KeyboardPolicy?
    package var windowMap: CGAffineTransform?
    package var configuration: UInt64?
    package var regions: [OwnerUpdateWire.Region]?

    package init() {}

    package static func == (a: Self, b: Self) -> Bool {
        a.band == b.band && a.keyboard == b.keyboard && a.keyboardPolicy == b.keyboardPolicy && a.windowMap == b.windowMap
            && a.configuration == b.configuration && a.regions == b.regions
            && a.chain.map(\.target) == b.chain.map(\.target) && a.chain.map(\.kinds) == b.chain.map(\.kinds)
            && a.relations.map(\.relation) == b.relations.map(\.relation) && a.relations.map(\.target) == b.relations.map(\.target)
    }

    /// The kernel's encoding: the band, top, right, bottom, left; the targets and the keyboard as an owner update's;
    /// the policy, a byte, 0 for none and else 1 more than its index; the window map, the configuration and the
    /// reserved regions as an owner update's.
    package func encode() -> Data {
        var w = WireOut()
        w.edges(band)
        w.targets(chain: chain, relations: relations)
        w.keyboard(keyboard)
        w.u8(keyboardPolicy.map { $0.rawValue + 1 } ?? 0)
        w.windowMap(windowMap)
        w.optional(configuration) { $0.u64($1) }
        w.regions(regions)
        return w.out
    }
}

/// The kernel's wire form's pieces that an owner's update and the session's share, little-endian, lengths as `f32`s.
struct WireOut {
    var out = Data()
    mutating func u8(_ v: UInt8) { out.append(v) }
    mutating func u32(_ v: UInt32) { withUnsafeBytes(of: v.littleEndian) { out.append(contentsOf: $0) } }
    mutating func u64(_ v: UInt64) { withUnsafeBytes(of: v.littleEndian) { out.append(contentsOf: $0) } }
    mutating func f32(_ v: CGFloat) { u32(Float(v).bitPattern) }
    mutating func edges(_ e: OwnerUpdateWire.Edges) { [e.top, e.right, e.bottom, e.left].forEach { f32($0) } }
    mutating func rect(_ r: CGRect) { [r.minX, r.minY, r.width, r.height].forEach { f32($0) } }
    /// A presence byte, then `value` as `each` writes it.
    mutating func optional<T>(_ value: T?, _ each: (inout WireOut, T) -> Void) {
        u8(value == nil ? 0 : 1)
        if let value { each(&self, value) }
    }
    /// The chain after its count, each its target and kinds; the relations after theirs, each its relation and target.
    mutating func targets(chain: [(target: OwnerUpdateWire.Target, kinds: UInt8)], relations: [(relation: UInt8, target: OwnerUpdateWire.Target)]) {
        u32(UInt32(chain.count))
        for entry in chain {
            out.append(OwnerUpdateWire.encode(target: entry.target))
            u8(entry.kinds)
        }
        u32(UInt32(relations.count))
        for r in relations {
            u8(r.relation)
            out.append(OwnerUpdateWire.encode(target: r.target))
        }
    }
    /// A byte, 0 unreported, 1 dismissed or 2 shown, and a shown one's frame and a docked byte.
    mutating func keyboard(_ keyboard: OwnerUpdateWire.Keyboard) {
        switch keyboard {
        case .unreported: u8(0)
        case .dismissed: u8(1)
        case .shown(let frame, let docked):
            u8(2)
            rect(frame)
            u8(docked ? 1 : 0)
        }
    }
    /// Its six terms in `CGAffineTransform`'s order, after a presence byte.
    mutating func windowMap(_ map: CGAffineTransform?) { optional(map) { w, m in [m.a, m.b, m.c, m.d, m.tx, m.ty].forEach { w.f32($0) } } }
    /// After a presence byte and their count, each its identity, kind, frame, margins and an active byte.
    mutating func regions(_ regions: [OwnerUpdateWire.Region]?) {
        optional(regions) { w, regions in
            w.u32(UInt32(regions.count))
            for r in regions {
                w.u64(r.id)
                w.u8(r.kind)
                w.rect(r.frame)
                w.edges(r.margins)
                w.u8(r.active ? 1 : 0)
            }
        }
    }
}
