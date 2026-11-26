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

    package struct Margins: Equatable {
        package var root: Edges, plain: Edges, minimum: Edges, readableWidth: CGFloat
        package init(root: Edges, plain: Edges, minimum: Edges, readableWidth: CGFloat) {
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

    package struct Chrome: Equatable {
        package var verticalBarEdge: Int?, sidebarAvailable: Bool?
        package init(verticalBarEdge: Int?, sidebarAvailable: Bool?) { (self.verticalBarEdge, self.sidebarAvailable) = (verticalBarEdge, sidebarAvailable) }
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
    /// view's, a plain view's and the minimum's, then the readable width. The keyboard is a byte, 0 unreported,
    /// 1 dismissed or 2 shown, and a shown one's frame, x, y, width, height, and a docked byte. Then the segments,
    /// each edge's bar region, the chrome, its vertical bar edge and its sidebar's availability each optional, the
    /// window map, its six terms in `CGAffineTransform`'s order, the configuration's identity (`u64`) and the reserved
    /// regions after their count, each its identity, kind, frame, margins and an active byte.
    package func encode() -> Data {
        var out = Data()
        func u8(_ v: UInt8) { out.append(v) }
        func u32(_ v: UInt32) { withUnsafeBytes(of: v.littleEndian) { out.append(contentsOf: $0) } }
        func u64(_ v: UInt64) { withUnsafeBytes(of: v.littleEndian) { out.append(contentsOf: $0) } }
        func f32(_ v: CGFloat) { u32(Float(v).bitPattern) }
        func edges(_ e: Edges) { [e.top, e.right, e.bottom, e.left].forEach(f32) }
        func rect(_ r: CGRect) { [r.minX, r.minY, r.width, r.height].forEach(f32) }
        func optional<T>(_ value: T?, _ each: (T) -> Void) {
            u8(value == nil ? 0 : 1)
            if let value { each(value) }
        }
        optional(band, edges)
        u32(UInt32(contributions.count))
        for c in contributions {
            u32(c.node)
            u8(c.operation)
            edges(c.effect)
        }
        u32(UInt32(chain.count))
        for entry in chain {
            out.append(Self.encode(target: entry.target))
            u8(entry.kinds)
        }
        u32(UInt32(relations.count))
        for r in relations {
            u8(r.relation)
            out.append(Self.encode(target: r.target))
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
        case .shown(let frame, let docked):
            u8(2)
            rect(frame)
            u8(docked ? 1 : 0)
        }
        optional(segments) { fold in
            u8(fold.cols)
            u8(fold.rows)
            fold.rects.forEach(rect)
        }
        for region in barRegions {
            optional(region) { r in [r.distance, r.start, r.end].forEach(f32) }
        }
        optional(chrome) { c in
            optional(c.verticalBarEdge) { u8(UInt8($0)) }
            optional(c.sidebarAvailable) { u8($0 ? 1 : 0) }
        }
        optional(windowMap) { m in [m.a, m.b, m.c, m.d, m.tx, m.ty].forEach(f32) }
        optional(configuration, u64)
        optional(regions) { regions in
            u32(UInt32(regions.count))
            for r in regions {
                u64(r.id)
                u8(r.kind)
                rect(r.frame)
                edges(r.margins)
                u8(r.active ? 1 : 0)
            }
        }
        return out
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
