import ExactCore
import XCTest
@testable import ExactKit

/// A batch that says the navigation roots' tabs (`Batch.rootTabs`, LLP
/// 1115.001 D11) is news: no shortcut that skips a batch or applies only its
/// snapshots passes it by, and a reset forgets the tabs held, which the next
/// host's first batch says again.
final class RootTabsBatchTests: XCTestCase {
    private let tabs = [RootTabs(root: 1, tablist: 2, tabs: [3], panels: [4], unsaid: false)]

    /// The tabs can change the screen with no op (the placement the views do not carry), so the
    /// session neither skips the batch nor counts it as showing nothing, which launch tracking reads.
    func testABatchThatSaysTheRootTabsChangesTheScreen() throws {
        let session = ExactApp.shared.makeSession(label: "root-tabs")
        defer { session.destroy() }
        var said = wireBatch([])
        XCTAssertTrue(session.changesNothing(said))
        said.rootTabs = tabs
        XCTAssertTrue(said.changesScreen, "the tabs are news")
        XCTAssertFalse(session.showsNothing(said))
        XCTAssertFalse(session.changesNothing(said))
    }

    /// A batch of list snapshots that says the tabs too: the presenter holds them.
    func testASnapshotBatchThatSaysTheRootTabsIsTaken() {
        let p = Presenter()
        var batch = wireBatch([["op": "collections", "items": []]])
        batch.rootTabs = tabs
        #if os(macOS)
        XCTAssertFalse(Presenter.onlySnapshots(batch), "the tabs are more than snapshots")
        #endif
        p.apply(batch)
        XCTAssertEqual(p.rootTabs[1]?.tablist, 2)
    }

    func testAResetForgetsTheRootTabs() {
        let p = Presenter()
        var batch = wireBatch([])
        batch.rootTabs = tabs
        p.apply(batch)
        XCTAssertEqual(p.rootTabs[1]?.tablist, 2)
        p.reset()
        XCTAssertTrue(p.rootTabs.isEmpty, "the next host's first batch says them again")
    }
}
