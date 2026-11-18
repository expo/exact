// @ref LLP 1055 D7 — an `svg` op's scene prepared on the owner, as it is
// decoded: each shape's path made and each animation read once
// (`SvgLink.prepare`).
import Foundation
import ExactKit
import ExactEffects

enum SvgPrepare {
    /// A scene's element tree, in place: each shape's path, each animation.
    static func scene(_ scene: inout [String: Any]) {
        if let elements = scene["els"] as? [Any] { scene["els"] = elements.map(element) }
    }

    static func element(_ value: Any) -> Any {
        guard var e = value as? [String: Any] else { return value }
        if let p = e["p"], !(p is PreparedPath) { e["p"] = PreparedPath(p) }
        if let children = e["c"] as? [Any] { e["c"] = children.map(element) }
        if let list = e["a"] as? [[String: Any]] { e["a"] = list.map(spec) }
        if var tf = e["tf"] as? [String: Any], let list = tf["a"] as? [[String: Any]] {
            tf["a"] = list.map(spec)
            e["tf"] = tf
        }
        return e
    }

    /// One CSS animation spec with its reading under "#": the scene's
    /// readers (`SvgFilterLive`) look at the element's dictionaries.
    static func spec(_ spec: [String: Any]) -> [String: Any] {
        guard spec["#"] == nil, let read = CssAnimation(spec) else { return spec }
        var out = spec
        out["#"] = read
        return out
    }
}
