// Compiled with Gamepad.swift by indices.test.mjs: an index that names no slot, from a request
// or from a drive's pad, answers or falls back rather than trapping. The registry is the
// drive's (`ExactPads.driven`), so no pad attached to the machine is read or moved.
import ExactPackage
import Foundation

@main
private struct IndexTests {
    static func main() {
        var failures: [String] = []
        func check(_ what: String, _ ok: Bool, _ got: Any) { if !ok { failures.append("\(what): got \(got)") } }

        // A slot outside the table is not taken; the pad gets the lowest free one.
        let slots = ExactPads()
        let a = NSObject(), b = NSObject(), c = NSObject()
        check("a negative slot", slots.take(a, at: -1) == 0, slots.index(of: a) as Any)
        check("a slot past the most", slots.take(b, at: ExactPads.most) == 1, slots.index(of: b) as Any)
        check("a free slot", slots.take(c, at: 5) == 5, slots.index(of: c) as Any)
        for pad in [a, b, c] { slots.free(pad) }

        let pads = ExactPads(driven: true, injected: [
            ["index": -2.0, "id": "negative"],
            ["index": 1e20, "id": "huge"],
            ["index": 1.5, "id": "fraction"],
            ["id": "pressed", "pressed": [1e20, 0.0]],
        ])
        let context = ExactPackageContext(agent: false, session: "indices", changed: { _ in }, log: { _ in }, now: { 0 }, pads: pads)
        let module = GamepadModule(context: context)
        func ask(_ name: String, _ args: [String: Any]) -> [String: Any] {
            var answer: [String: Any] = ["error": "no answer"]
            module.later(name, args, preferred: nil) { answer = $0 }
            return answer
        }
        let list = ask("getGamepads", [:])
        let listed = (list["gamepads"] as? [[String: Any]] ?? []).map { "\($0["index"] ?? "") \($0["id"] ?? "")" }
        check("the drive's pads, each in the lowest free slot", listed == ["0 negative", "1 huge", "2 fraction", "3 pressed"], listed)
        check("the pressed pad is current", list["current"] as? Int == 3, list["current"] as Any)

        for index in [1e20, -1, 1.5, 7] as [Double] {
            for name in ["playEffect", "resetEffects", "setLight"] {
                var args: [String: Any] = ["index": index, "type": "dual-rumble", "duration": 10.0]
                if name == "setLight" { args["color"] = ["r": 255.0, "g": 0.0, "b": 0.0] }
                let answer = ask(name, args)
                check("\(name) at \(index)", answer["error"] as? String == "NotFoundError", answer)
            }
        }
        check("a reset without an index is the current pad's", ask("resetEffects", [:])["result"] as? String == "complete", "")
        check("a reset at a pad", ask("resetEffects", ["index": 2.0])["result"] as? String == "complete", "")

        for failure in failures { print("FAIL: \(failure)") }
        print("gamepad indices: \(failures.count) failures")
        exit(failures.isEmpty ? 0 : 1)
    }
}
