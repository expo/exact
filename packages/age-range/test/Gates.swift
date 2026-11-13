// Compiled with AgeRange.swift and AgentAgeRange.swift by gates.test.mjs. Under the agent a
// request asks no sheet, so a gate reaches the request path and its conversion with no window.
import ExactPackage
import Foundation

@main
private struct GateTests {
    static func main() {
        let context = ExactPackageContext(agent: true, session: "gates", changed: { _ in }, log: { _ in }, now: { 0 }, emit: { _, _ in })
        let module = AgeRangeModule(context: context)
        let cases: [([Double], String?)] = [
            ([13], nil),
            ([13, 16, 18], nil),
            ([150], nil),
            ([18.5], "TypeError"),
            ([-1], "TypeError"),
            ([151], "TypeError"),
            ([1e20], "TypeError"),
            ([13, 14], "TypeError"),
        ]
        var failures = 0
        for (gates, expected) in cases {
            var answer: [String: Any]?
            module.later("requestAgeRange", ["ageGates": gates.map { NSNumber(value: $0) }], preferred: nil) { answer = $0 }
            let error = answer?["error"] as? String
            let refused = error == "TypeError" ? "TypeError" : nil
            if answer == nil || refused != expected {
                failures += 1
                print("FAIL: \(gates): expected \(expected ?? "no TypeError"), got \(answer.map { "\($0)" } ?? "no answer")")
            }
        }
        print("age gates: \(failures) failures")
        exit(failures == 0 ? 0 : 1)
    }
}
