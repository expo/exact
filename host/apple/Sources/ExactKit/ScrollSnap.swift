// CSS Scroll Snap 1 on Apple's hosts (LLP 1008 §9, "Scroll snap"): along
// one axis, the offsets a scroller's snap areas rest at, and which of them a
// scroll that ends rests at. UIKit asks when a drag ends
// (`IOS/ScrollSnapIOS.swift`), AppKit when a trackpad lifts or a wheel stops
// (`Mac/ScrollSnapMac.swift`). The Linux family's is
// `host/painted/src/presenter/snap.rs`, Android's `Scrolling.kt`'s `snapRest`.
import CoreGraphics

enum ScrollSnap {
    /// `scroll-snap-type`, as the row spells it: which axes snap, and whether
    /// every rest is a snap position (`mandatory`) or only one near it.
    struct Kind: Equatable {
        var x: Bool, y: Bool, mandatory: Bool
        init?(_ value: String?) {
            let words = (value ?? "none").split(separator: " ")
            switch words.first {
            case "x": (x, y) = (true, false)
            case "y": (x, y) = (false, true)
            case "both": (x, y) = (true, true)
            default: return nil
            }
            mandatory = words.last == "mandatory"
        }
    }

    /// An area's `scroll-snap-align` along one axis: one keyword is both
    /// axes'; of two, the first is the block axis's (y), the second the
    /// inline axis's (x). `nil` for `none`.
    static func align(_ value: String?, vertical: Bool) -> String? {
        let words = (value ?? "none").split(separator: " ").map(String.init)
        let word = words.count == 2 ? words[vertical ? 0 : 1] : words.first ?? "none"
        return word == "none" ? nil : word
    }

    /// Whether a box with these `overflow-x` and `overflow-y` is a scroll
    /// container, which captures the snap areas inside it: `hidden` is one,
    /// `clip` is not (CSS Overflow 3 §3.1).
    static func ownsAreas(_ x: String?, _ y: String?) -> Bool {
        [x, y].contains { ["auto", "scroll", "hidden"].contains($0 ?? "visible") }
    }

    /// The offsets one area rests at, `from` through `to`, whether a snap may
    /// not pass over them (`scroll-snap-stop: always`), and its node (0 for a
    /// list's row that is not mounted).
    struct Area: Equatable {
        var from: CGFloat, to: CGFloat, stop: Bool, id: UInt32 = 0
    }

    /// An area spanning `lo..<hi` along the axis, in the coordinates of the
    /// snapport's start, in a snapport `port` long whose start may rest in
    /// `range`. One no longer than the snapport rests at one offset, its start,
    /// centre or end at the snapport's; a longer one at any offset while it
    /// covers the snapport (§5.2.2's "snap area larger than the snapport").
    static func area(lo: CGFloat, hi: CGFloat, align: String, stop: Bool, port: CGFloat, range: ClosedRange<CGFloat>, id: UInt32 = 0) -> Area {
        let wide = hi - lo > port
        let at = wide ? lo : align == "end" ? hi - port : align == "center" ? (lo + hi - port) / 2 : lo
        let from = min(range.upperBound, max(range.lowerBound, at))
        let to = wide ? min(range.upperBound, max(from, hi - port)) : from
        return Area(from: from, to: to, stop: stop, id: id)
    }

    /// Where a scroller last came to rest along one axis: the node of the
    /// area it rested on (0 for none, or a list's row that is not mounted)
    /// and the position.
    struct Rested: Equatable {
        var id: UInt32 = 0, at: CGFloat = 0
        init(id: UInt32 = 0, at: CGFloat = 0) { self.id = id; self.at = at }
        /// Resting at `position` among `areas`.
        init(_ position: CGFloat, in areas: [Area]) {
            self.init(id: areas.first { position >= $0.from - 0.5 && position <= $0.to + 0.5 }?.id ?? 0, at: position)
        }
    }

    /// CSS Scroll Snap 1 §5.5, a re-snap after layout changes: a scroller
    /// that `rested` on an area and is now at `current` goes where that area
    /// now rests, when layout moved it; when the area went and the scroller is
    /// still where it rested, to the position nearest. `nil`: it stays.
    static func resnap(_ areas: [Area], rested: Rested, current: CGFloat, mandatory: Bool, port: CGFloat) -> CGFloat? {
        guard rested.id != 0 else { return nil }
        let kept = areas.filter { $0.id == rested.id }
        guard !kept.isEmpty else {
            return abs(current - rested.at) < 0.5 ? rest(areas, projected: current, release: current, mandatory: mandatory, port: port) : nil
        }
        guard let at = rest(kept, projected: current, release: current, mandatory: true, port: port), abs(at - rested.at) > 0.5 else { return nil }
        return at
    }

    /// How near a `proximity` snap's position must be to where the scroll
    /// would rest: a third of the snapport.
    static let proximity: CGFloat = 1 / 3

    /// The speed from which a release is a flick, in points per millisecond:
    /// Android's minimum fling velocity, 50 dp a second
    /// (`ViewConfiguration.getScaledMinimumFlingVelocity`).
    static let flick: CGFloat = 0.05

    /// The way a release at `velocity` (points per millisecond, positive as
    /// the offset grows) flicks: 1, −1, or 0 for none.
    static func way(_ velocity: CGFloat) -> CGFloat {
        abs(velocity) < flick || !velocity.isFinite ? 0 : velocity > 0 ? 1 : -1
    }

    /// Where a scroll released at `release` and projected to rest at
    /// `projected` comes to rest among `areas`, or `nil` to rest where it
    /// would. The position nearest the projection (the smaller of two as
    /// near); for a flick `way` (1 as the offset grows, −1 back), at least the
    /// first position past `start`, where the drag began, that way; then
    /// short of the first `always` position past the release in the way it
    /// goes (§6.2); and for `proximity`, only within a third of the snapport
    /// of the projection.
    static func rest(_ areas: [Area], projected: CGFloat, release: CGFloat, start: CGFloat? = nil, way: CGFloat = 0,
                     mandatory: Bool, port: CGFloat) -> CGFloat? {
        func at(_ a: Area) -> CGFloat { min(a.to, max(a.from, projected)) }
        guard var rest = areas.map(at).min(by: { a, b in
            let da = abs(a - projected), db = abs(b - projected)
            return da < db || (da == db && a < b)
        }) else { return nil }
        // A move under half a point is none: positions from layout are fractional.
        let e: CGFloat = 0.5
        // A flick that would rest back where its drag began, or behind, goes
        // on to the next position its way, as a UIKit pager's does.
        let from = start ?? release
        if way != 0, way * (rest - from) <= e, let next = areas.compactMap({ a -> CGFloat? in
            let p = min(a.to, max(a.from, from + way * 2 * e))
            return way * (p - from) > e ? p : nil
        }).min(by: { way * $0 < way * $1 }) {
            rest = next
        }
        if rest > release + e, let stop = areas.filter(\.stop).compactMap({ $0.from > release + e ? $0.from : $0.to > release + e ? $0.to : nil }).min() {
            rest = min(rest, stop)
        }
        if rest < release - e, let stop = areas.filter(\.stop).compactMap({ $0.to < release - e ? $0.to : $0.from < release - e ? $0.from : nil }).max() {
            rest = max(rest, stop)
        }
        if !mandatory, abs(rest - projected) > port * proximity { return nil }
        return rest
    }
}
