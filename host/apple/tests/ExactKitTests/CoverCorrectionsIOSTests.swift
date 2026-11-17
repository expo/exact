#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// A chain of covers corrections, a change no answer causes and what the runtime's answers to its deliveries make in
/// turn, has eight passes, however its answers branch and whether they apply at once or wait behind a slice; a change no
/// answer causes starts a chain of its own.
final class CoverCorrectionsIOSTests: XCTestCase {
    func testCoversThatNeverSettleBehindASliceStopAfterEightPasses() {
        let session = ExactApp.shared.makeSession(label: "covers-oscillation-behind-a-slice")
        defer { session.destroy() }
        let navigation = session.presenter.navigation
        var told = 0
        session.presenter.onCovers = { _ in
            told += 1
            guard told < 40 else { return }
            session.whenIdle(bound: true) { session.apply(wireBatch([])) }
        }
        // The runtime's answer moves the scroller, which changes its cover again
        session.onApplyForTests = { _ in
            guard session.coversCause != nil else { return }
            navigation.covers = [1: told % 2 == 0 ? .whole : edge(1)]
            navigation.countReport()
            navigation.deliverCovers()
        }
        session.holdFillForTests()
        navigation.covers = [1: .whole]
        navigation.countReport()
        navigation.deliverCovers()
        session.publishFillForTests(1, wireBatch([]), lease: session.runtime.completionLease)
        session.landFill()
        XCTAssertEqual(told, 8)
        navigation.deliverCovers()
        navigation.deliverCovers()
        XCTAssertEqual(told, 8, "a delivery asked again tells nothing in the spent chain")
    }

    func testAnswersThatBranchShareTheirChainsEightPasses() {
        let session = ExactApp.shared.makeSession(label: "covers-branching-answers")
        defer { session.destroy() }
        let navigation = session.presenter.navigation
        var told = 0
        // Two answers to each delivery, each moving its own scroller, which changes that scroller's cover again
        session.presenter.onCovers = { _ in
            told += 1
            guard told < 300 else { return }
            for id: UInt32 in [1, 2] {
                session.whenIdle(bound: true) {
                    navigation.covers[id] = navigation.covers[id] == .whole ? edge(1) : .whole
                    navigation.countReport()
                    navigation.deliverCovers()
                }
            }
        }
        session.holdFillForTests()
        navigation.covers = [1: .whole, 2: .whole]
        navigation.countReport()
        navigation.deliverCovers()
        session.publishFillForTests(1, wireBatch([]), lease: session.runtime.completionLease)
        session.landFill()
        XCTAssertEqual(told, 8)
    }

    func testAReportOutsideTheChainDuringItsEighthAnswerIsDeliveredInAChainOfItsOwn() {
        let session = ExactApp.shared.makeSession(label: "covers-change-in-the-eighth-answer")
        defer { session.destroy() }
        let navigation = session.presenter.navigation
        var told: [UInt32] = []
        session.presenter.onCovers = { changes in
            told += changes.map(\.0)
            session.whenIdle(bound: true) { session.apply(wireBatch([])) }
        }
        // Each answer moves scroller 1, which changes its cover again; during the eighth, a report outside the chain, as a
        // slice landed then makes, changes scroller 2's
        session.onApplyForTests = { _ in
            guard let chain = session.coversCause as? CoversChain else { return }
            navigation.covers[1] = navigation.covers[1] == .whole ? edge(1) : .whole
            navigation.countReport()
            if chain.passes == 8, navigation.covers[2] == nil {
                session.inCoversChain(nil) {
                    navigation.covers[2] = .whole
                    navigation.countReport()
                }
            }
            navigation.deliverCovers()
        }
        navigation.covers = [1: .whole]
        navigation.countReport()
        navigation.deliverCovers()
        XCTAssertFalse(told.contains(2), "the call that ran eight passes leaves that change for the next turn")
        waitForMainQueue("the next turn's delivery")
        XCTAssertTrue(told.contains(2), "which delivers it")
        XCTAssertFalse(navigation.coversAnswerOwed, "and leaves no report it counted owed")
    }

    func testALiftAnAnswerMakesHoldsUntilItsOwnDeliverysAnswer() {
        let session = ExactApp.shared.makeSession(label: "covers-lift-in-an-answer")
        defer { session.destroy() }
        let navigation = session.presenter.navigation
        var heldBetween: Bool?, lifted = false
        session.presenter.onCovers = { _ in session.whenIdle(bound: true) { session.apply(wireBatch([])) } }
        // The first delivery's answer lifts another header, as `coverLifted` does inside a batch
        session.onApplyForTests = { _ in
            guard session.coversCause != nil, !lifted else { return }
            lifted = true
            navigation.covers[2] = .whole
            navigation.countReport()
            navigation.liftReport = navigation.coversReported
            session.whenIdle { heldBetween = navigation.liftHeld }
            navigation.deliverCovers()
        }
        session.holdFillForTests()
        navigation.covers[1] = .whole
        navigation.countReport()
        navigation.liftReport = navigation.coversReported
        navigation.deliverCovers()
        session.publishFillForTests(1, wireBatch([]), lease: session.runtime.completionLease)
        session.landFill()
        XCTAssertEqual(heldBetween, true, "the first delivery's end leaves the second lift held")
        XCTAssertFalse(navigation.liftHeld, "and the second delivery releases it, just before its answer")
    }

    func testACarriedPositionWaitsWhenAnEarlierWriteReportsCovers() throws {
        let session = ExactApp.shared.makeSession(label: "covers-carried-writes-recheck")
        defer { session.destroy() }
        let p = session.presenter, navigation = p.navigation
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        defer { window.isHidden = true }
        var ops: [[String: Any]] = [["op": "create", "id": 1, "kind": "view"]]
        for id in [2, 3] {
            ops += [["op": "create", "id": id, "kind": "view"], ["op": "style", "id": id, "style": ["overflow_y": "scroll"]],
                    ["op": "frame", "id": id, "x": 0.0, "y": Double(id - 2) * 200, "w": 402.0, "h": 180.0],
                    ["op": "content", "id": id, "w": 402.0, "h": 9000.0], ["op": "props", "id": id, "set": ["scrollTop": "140"]]]
        }
        ops += [["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 402.0, "h": 400.0], ["op": "children", "id": 1, "ids": [2, 3]], ["op": "roots", "ids": [1]]]
        p.apply(wireBatch(ops))
        let owners = try [UInt32(2), 3].map { try XCTUnwrap(p.views[$0]) }
        let scrolls = try owners.map { try XCTUnwrap($0.scrollView) }
        p.onCovers = { _ in }
        // The first write reports covers whose answer is still to come
        var armed = false
        let observations = scrolls.map { $0.observe(\.contentOffset) { _, _ in
            guard armed, !navigation.coversAnswerOwed else { return }
            navigation.covers[9] = .whole
            navigation.countReport()
        } }
        defer { observations.forEach { $0.invalidate() } }
        let writes: [(String, () -> Void)] = [("outside a batch", { navigation.writeCarriedScrollsWhenAnswered() }),
                                              ("in a batch's restore", { p.finishScrollPositions(restoring: false) })]
        for (place, write) in writes {
            armed = false
            navigation.coversAnswered = navigation.coversReported
            for (owner, scroll) in zip(owners, scrolls) {
                scroll.contentOffset.y = 140 - owner.scrollTopInset(scroll)
                owner.retainScrollPosition(from: scroll)
                scroll.contentOffset.y = 0
            }
            armed = true
            write()
            XCTAssertEqual(owners.filter { $0.pendingScrollTop != nil }.count, 1, "\(place), the position after the write that reported waits for its answer")
        }
    }

    func testChangesWaitingBehindASliceAreEachDelivered() {
        let session = ExactApp.shared.makeSession(label: "covers-changes-behind-a-slice")
        defer { session.destroy() }
        let navigation = session.presenter.navigation
        var told: [HostCover] = []
        // The runtime's answers change nothing
        session.presenter.onCovers = { changes in
            told += changes.compactMap(\.1)
            session.whenIdle(bound: true) { session.apply(wireBatch([])) }
        }
        session.holdFillForTests()
        for top in 1...12 {
            session.whenIdle(bound: true) {
                navigation.covers = [1: edge(CGFloat(top))]
                navigation.countReport()
                navigation.deliverCovers()
            }
        }
        session.publishFillForTests(1, wireBatch([]), lease: session.runtime.completionLease)
        session.landFill()
        XCTAssertEqual(told, (1...12).map { edge(CGFloat($0)) })
    }

    func testAChainKeepsItsPassesAmongChangesThatWaitBetweenItsAnswers() {
        let session = ExactApp.shared.makeSession(label: "covers-chain-among-changes")
        defer { session.destroy() }
        let navigation = session.presenter.navigation
        var corrections = 0, others: [HostCover] = []
        // The runtime's answer to scroller 1's cover moves it, which changes its cover again; scroller 2's settles
        session.presenter.onCovers = { changes in
            let flips = changes.contains { $0.0 == 1 }
            if flips { corrections += 1 }
            others += changes.compactMap { $0.0 == 2 ? $0.1 : nil }
            session.whenIdle(bound: true) {
                guard flips, corrections < 40 else { return }
                navigation.covers[1] = navigation.covers[1] == .whole ? edge(1) : .whole
                navigation.countReport()
                navigation.deliverCovers()
            }
        }
        session.holdFillForTests()
        navigation.covers = [1: .whole]
        navigation.countReport()
        navigation.deliverCovers()
        for top in 1...6 {
            session.whenIdle(bound: true) {
                navigation.covers[2] = edge(CGFloat(top))
                navigation.countReport()
                navigation.deliverCovers()
            }
        }
        session.publishFillForTests(1, wireBatch([]), lease: session.runtime.completionLease)
        session.landFill()
        XCTAssertEqual(corrections, 8, "scroller 1's chain stops after eight passes, whatever runs between them")
        XCTAssertEqual(others, (1...6).map { edge(CGFloat($0)) }, "and each change of scroller 2 is delivered")
    }
}

private func edge(_ top: CGFloat) -> HostCover { .edges(.init(top: top, right: 0, bottom: 0, left: 0)) }
#endif
