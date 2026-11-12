import XCTest
@testable import ExactKit

// The file form of the dev loop, which both adapters run
// (`ExactDevelopmentPlan.watch`): a candidate refused as pending is asked
// again on the next tick until it lands; any other outcome is asked again
// only when the files change.
final class DevelopmentPlanWatchTests: XCTestCase {
    func testAPendingRefusalIsAskedAgainUntilItLands() {
        var applies = 0, pending = true, accepted = false
        let apply = { applies += 1; return accepted }
        var last: [Double] = []
        let revision: [Double] = [1, 2, 3]
        for _ in 0..<3 { last = ExactDevelopmentPlan.seen(revision, last: last, apply: apply, pending: { pending }) }
        XCTAssertEqual(applies, 3, "each tick while pending asks again")
        XCTAssertEqual(last, [])
        accepted = true; pending = false
        last = ExactDevelopmentPlan.seen(revision, last: last, apply: apply, pending: { pending })
        XCTAssertEqual(last, revision)
        last = ExactDevelopmentPlan.seen(revision, last: last, apply: apply, pending: { pending })
        XCTAssertEqual(applies, 4, "an applied revision is not applied again")
    }

    func testARefusalThatIsNotPendingWaitsForTheFilesToChange() {
        var applies = 0
        let apply = { applies += 1; return false }
        var last = ExactDevelopmentPlan.seen([1], last: [], apply: apply, pending: { false })
        last = ExactDevelopmentPlan.seen([1], last: last, apply: apply, pending: { false })
        XCTAssertEqual(applies, 1)
        _ = ExactDevelopmentPlan.seen([2], last: last, apply: apply, pending: { false })
        XCTAssertEqual(applies, 2)
    }
}
