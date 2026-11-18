import XCTest
@testable import ExactKit

final class DevelopmentPlanWatchTests: XCTestCase {
    func testEveryOutcomeWaitsForNewBytesRatherThanResubmitting() {
        for result in [ExactApplyResult.applied, .held("composition"), .refused("invalid plan")] {
            var applies = 0
            let apply = { applies += 1; return result }
            var last = ExactDevelopmentPlan.seen([1], last: [], apply: apply)
            last = ExactDevelopmentPlan.seen([1], last: last, apply: apply)
            XCTAssertEqual(applies, 1)
            _ = ExactDevelopmentPlan.seen([2], last: last, apply: apply)
            XCTAssertEqual(applies, 2)
        }
    }
}
