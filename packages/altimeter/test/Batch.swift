// Compiled with Altimeter.swift by batch.test.mjs: a poll's samples and dropped count after the
// runner's cursor. The answer's sequence, the newest, becomes the next cursor, so each sample after
// the cursor is either answered or counted dropped, shown or hidden.
import ExactPackage
import Foundation

@main
private struct BatchTests {
    static func main() {
        var failures: [String] = []
        let ring = (3...7).map { (sequence: $0, fields: ["relativeAltitude": Double($0)] as [String: Any]) }
        let cases: [(after: Int, history: Int, shown: Bool, samples: [Double], dropped: Int)] = [
            (2, 10, true, [3, 4, 5, 6, 7], 0),
            (4, 10, true, [5, 6, 7], 0),
            (0, 10, true, [3, 4, 5, 6, 7], 2),
            (2, 2, true, [6, 7], 3),
            (2, 10, false, [], 5),
            (4, 2, false, [], 3),
            (7, 10, false, [], 0),
            (2, 0, true, [], 0),
        ]
        for c in cases {
            let got = AltimeterModule.batch(ring, after: c.after, history: c.history, shown: c.shown)
            let samples = got.samples.compactMap { $0["relativeAltitude"] as? Double }
            if samples != c.samples || got.dropped != c.dropped {
                failures.append("after \(c.after), history \(c.history), shown \(c.shown): \(samples) and \(got.dropped) dropped, not \(c.samples) and \(c.dropped)")
            }
        }
        for failure in failures { print("FAIL: \(failure)") }
        print("altimeter batches: \(failures.count) failures")
        exit(failures.isEmpty ? 0 : 1)
    }
}
