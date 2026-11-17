import XCTest
@testable import ExactKit

/// One wake offered to several packages ends once, when the last package
/// that took it is done, whether each finishes during its offer or later,
/// and whatever the others declined (LLP 1133.017 D8).
final class WakeOfferTests: XCTestCase {
    private enum Answer { case now, later, decline }
    private var observer: NSObjectProtocol?

    override func setUp() { super.setUp(); WakeStore.install() }
    override func tearDown() { observer.map(NotificationCenter.default.removeObserver); observer = nil; super.tearDown() }

    /// A transfer wake for one package per answer, each answering in the
    /// order it is offered: done at once, held for `release`, or declined.
    /// Returns the wake's completions so far and the held packages' dones.
    private func offer(_ answers: [Answer]) -> (completed: () -> [Bool], release: [() -> Void]) {
        let packages = answers.indices.map { "wake-offer-\(UUID().uuidString.prefix(6))-\($0)" }
        var offered = 0, held: [() -> Void] = [], completed: [Bool] = []
        observer = NotificationCenter.default.addObserver(forName: Notification.Name("exact.wakes.due"), object: nil, queue: nil) { note in
            guard let body = note.object as? NSMutableDictionary, let token = body["token"] as? String,
                  let package = body["package"] as? String, packages.contains(package) else { return }
            let answer = answers[offered]
            offered += 1
            guard answer != .decline else { return }
            body["taken"] = true
            let done = {
                let result: [String: Any] = ["token": token, "package": package, "success": true]
                NotificationCenter.default.post(name: Notification.Name("exact.wakes.done"), object: result as NSDictionary)
            }
            if answer == .now { done() } else { held.append(done) }
        }
        WakeStore.transfer(packages.map { ($0, "key") }) { completed.append($0) }
        XCTAssertEqual(offered, answers.count, "every package is offered its work")
        return ({ completed }, held)
    }

    func testAPackageDoneDuringItsOfferDoesNotEndTheWakeBeforeTheOthers() {
        for answers: [Answer] in [[.now, .later], [.later, .now], [.later, .decline, .now], [.now, .decline, .later]] {
            let wake = offer(answers)
            XCTAssertEqual(wake.completed(), [], "\(answers): ended while a package still runs")
            wake.release.forEach { $0() }
            XCTAssertEqual(wake.completed(), [true], "\(answers): ends once its last package is done")
        }
    }

    func testAWakeEveryPackageFinishesDuringItsOfferEndsOnce() {
        for answers: [Answer] in [[.now], [.now, .now], [.decline, .now], [.now, .decline]] {
            XCTAssertEqual(offer(answers).completed(), [true], "\(answers)")
        }
    }

    func testAWakeEveryPackageDeclinesEndsUntaken() {
        XCTAssertEqual(offer([.decline, .decline]).completed(), [false])
    }
}
