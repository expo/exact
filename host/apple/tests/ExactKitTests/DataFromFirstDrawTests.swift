#if !os(tvOS)
import ExactCore
import UserNotifications
import XCTest
@testable import ExactKit

/// LLP 1027.008 Part 1: an owner-placed TypeScript module's evaluation is
/// posted to the owner from the first draw's own callback, and the rest of
/// what follows first pixel runs, in its order, when the evaluation has
/// ended. The tests link a Rust source, so a session is told to take the
/// module's path (`Runtime.evaluatesFromDraw`, which the boot sets from the
/// source); `prepareData` is then the source's activation on the owner.
final class DataFromFirstDrawTests: XCTestCase {
    private var sessions: [ExactSession] = []
    private var restoreCategories: (() -> Void)?

    override func setUp() {
        super.setUp()
        // An XCTest process has no notification centre: the categories are held here.
        let presenter = NotificationPresenter.shared, categories = presenter.categories
        restoreCategories = { presenter.categories = categories }
        var registered: Set<UNNotificationCategory> = []
        presenter.categories = (read: { $0(registered) }, write: { registered = $0 })
    }

    override func tearDown() {
        for session in sessions { session.destroy() }
        sessions = []
        restoreCategories?()
        super.tearDown()
    }

    private let size = CGSize(width: 390, height: 844)

    private func booted(evaluatesFromDraw: Bool = true) throws -> ExactSession {
        let session = ExactApp.shared.makeSession(label: "from-draw-\(UUID().uuidString)")
        sessions.append(session)
        XCTAssertNil(session.boot(plan: try fixturePlan("NOTIFICATION_NO_ACTIONS"), size: size).error)
        session.runtime.evaluatesFromDraw = evaluatesFromDraw
        return session
    }

    /// D1 and D2: the draw's callback posts the evaluation, and while it
    /// runs no turn of the main thread waits for the owner and nothing of
    /// the rest has run. The agent's settle waits from the draw on (D3).
    func testTheDrawPostsTheEvaluationAndNoTurnWaitsForIt() throws {
        let session = try booted()
        // The owner busy, as it is through a long evaluation.
        let held = DispatchSemaphore(value: 0)
        var released = false
        defer { if !released { held.signal() } }
        Owner.shared.post { _ = held.wait(timeout: .now() + 10) }
        session.drawReceipt().drawn()
        XCTAssertEqual(session.preparedGeneration, session.generation, "the evaluation is posted from the draw itself")
        XCTAssertTrue(session.activationInFlight, "the agent's settle waits from the draw on")
        let started = Date()
        for _ in 0..<3 { waitForMainQueue() }
        XCTAssertLessThan(Date().timeIntervalSince(started), 2, "no turn waited for the owner")
        XCTAssertFalse(session.dataActivated)
        XCTAssertTrue(session.activationInFlight)
        held.signal()
        released = true
        waitFor("the data's activation") { session.dataActivated }
        XCTAssertFalse(session.dataPreparing)
    }

    /// D2: when the evaluation has ended, the categories, the app's native
    /// module and the activation run in the main thread's next turn.
    func testTheRestRunsInTheTurnAfterTheEvaluation() throws {
        let session = try booted()
        session.drawReceipt().drawn()
        Owner.shared.sync {} // after the evaluation, which has posted the rest to this thread
        XCTAssertFalse(session.dataActivated)
        waitForMainQueue("the turn queued after the rest")
        XCTAssertTrue(session.dataActivated, "activated in that turn")
        XCTAssertFalse(session.dataPreparing)
    }

    /// D3: the rest, posted for a generation replaced during the evaluation,
    /// applies nothing; the new generation activates on its own draw.
    func testAGenerationReplacedDuringTheEvaluationAppliesNothing() throws {
        let session = try booted()
        session.drawReceipt().drawn()
        let drawn = session.generation
        Owner.shared.sync {}
        let candidate = try XCTUnwrap(session.prepare(try fixturePlan("NOTIFICATION_NO_ACTIONS"), resolver: ExactApp.shared.resolver, size: size))
        let batch = try XCTUnwrap(session.commit(candidate))
        session.presentCommitted(batch, label: "replaced during the evaluation")
        XCTAssertNotEqual(session.generation, drawn)
        waitForMainQueue("the rest, for the replaced generation")
        XCTAssertNil(session.dataGeneration, "nothing activated for the replaced generation")
        XCTAssertFalse(session.activationInFlight)
        session.runtime.evaluatesFromDraw = true
        session.drawReceipt().drawn()
        waitForActivation(session)
        XCTAssertEqual(session.dataGeneration, session.generation)
    }

    /// D3: a source the draw does not evaluate (a Rust source, a module on a
    /// worker, a Mac module that keeps documents) is prepared after the main
    /// thread's first turn, as before.
    func testASourceTheDrawDoesNotEvaluateWaitsForTheFirstTurn() throws {
        let session = try booted(evaluatesFromDraw: false)
        session.drawReceipt().drawn()
        XCTAssertNil(session.preparedGeneration, "nothing is posted from the draw")
        waitForActivation(session)
        XCTAssertEqual(session.preparedGeneration, session.generation)
        XCTAssertTrue(session.dataActivated)
    }
}
#endif
