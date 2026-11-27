import ExactCore
import ExactKit
import Foundation
import QuartzCore

private func num(_ v: Any?) -> Double { (v as? NSNumber)?.doubleValue ?? 0 }

/// A CSS animation spec as `CssAnimations` plays it (LLP 1055 D7), read from
/// the batch's dictionary once, on the owner as the batch is decoded
/// (`SvgPrepare`), and kept for the agent clock's re-seek: kept as the
/// dictionary, each animated box held about 1 KB.
package struct CssAnimation {
    let id: String
    /// The key path's name: a box's (`opacity`, `transform.scale`, …) or a
    /// shape's (`fillColor`, `path`, `r`).
    let key: String
    /// What `CssAnimations.apply` keeps an unchanged one by.
    let signature: Int
    /// The keyframe animation's values, key times and timing functions.
    let track: LoweredTrack
    var values: NSArray { track.values }
    var keyTimes: [NSNumber] { track.keyTimes }
    var timing: [CAMediaTimingFunction]? { track.timing }
    let duration: Double, repeatCount: Double, fill: Int, start: Double, delay: Double
    /// Each key holds to the next (`kCAAnimationDiscrete`): a `steps()` a
    /// path plays as whole paths (LLP 1003.003 D7).
    let discrete: Bool
    /// The local time an authored pause holds it at, in seconds.
    let held: Double?

    /// An SVG element's spec carries its reading under "#" (`SvgPrepare.spec`).
    package init?(_ spec: [String: Any]) {
        if let read = spec["#"] as? CssAnimation { self = read; return }
        guard let id = spec["id"] as? String else { return nil }
        self.id = id
        key = spec["k"] as? String ?? ""
        var h = Hasher()
        CssAnimations.digest(spec, into: &h)
        signature = h.finalize()
        track = LoweredTrack.shared(spec)
        duration = num(spec["d"]); repeatCount = num(spec["n"]); fill = Int(num(spec["fill"]))
        start = num(spec["s"]); delay = num(spec["dl"]); discrete = num(spec["m"]) == 1
        held = (spec["h"] as? NSNumber)?.doubleValue
    }

    /// A list as an op or an element carries it: read, or dictionaries.
    package static func list(_ value: Any?) -> [CssAnimation] {
        value as? [CssAnimation] ?? (value as? [[String: Any]] ?? []).compactMap(CssAnimation.init)
    }
}

/// A keyframe track lowered for Core Animation, shared by every animation
/// that plays it: boxes that play one rule from different delays (a step
/// row's lights) lowered it to equal arrays each, about 0.7 KB a box. The
/// values are an `NSArray`, which each `CAKeyframeAnimation` keeps as it is;
/// a Swift `[Any]` is bridged to a new array for each.
final class LoweredTrack {
    let values: NSArray, keyTimes: [NSNumber], timing: [CAMediaTimingFunction]?

    private init(_ spec: [String: Any]) {
        let (values, keyTimes, timing) = CssAnimations.lowered(spec)
        (self.values, self.keyTimes, self.timing) = (NSArray(array: values), keyTimes, timing)
    }

    /// What lowering reads: the key path, values, key times and curves.
    private struct Key: Hashable {
        let parts: [Any?]
        let hash: Int
        init(_ spec: [String: Any]) {
            parts = [spec["k"], spec["v"], spec["t"], spec["c"]]
            var h = Hasher()
            for part in parts { CssAnimations.digest(part, into: &h) }
            hash = h.finalize()
        }
        func hash(into h: inout Hasher) { h.combine(hash) }
        static func == (a: Key, b: Key) -> Bool { a.hash == b.hash && zip(a.parts, b.parts).allSatisfy(same) }
    }
    private struct Held { weak var track: LoweredTrack? }
    private static let lock = NSLock()
    nonisolated(unsafe) private static var held: [Key: Held] = [:]
    /// Entries after the last sweep; the next comes at twice this.
    nonisolated(unsafe) private static var swept = 0

    /// The track `spec` lowers to: the one a live animation holds, or new.
    static func shared(_ spec: [String: Any]) -> LoweredTrack {
        let key = Key(spec)
        lock.lock(); defer { lock.unlock() }
        if let track = held[key]?.track { return track }
        let track = LoweredTrack(spec)
        held[key] = Held(track: track)
        if held.count > 2 * swept + 64 {
            held = held.filter { $0.value.track != nil }
            swept = held.count
        }
        return track
    }

    /// Equal as lowering reads them: numbers by value, strings, lists of them.
    private static func same(_ a: Any?, _ b: Any?) -> Bool {
        switch (a, b) {
        case (.none, .none): return true
        case let (x as NSNumber, y as NSNumber): return x.doubleValue.bitPattern == y.doubleValue.bitPattern
        case let (x as String, y as String): return x == y
        case let (x as [Any], y as [Any]): return x.count == y.count && zip(x, y).allSatisfy(same)
        default: return false
        }
    }
}
