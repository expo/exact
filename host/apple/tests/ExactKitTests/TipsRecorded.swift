import XCTest
@testable import ExactKit

/// A Tips capability that keeps what the core told it: the tips in the order
/// given, each tip's anchor, and the owners withdrawn. The tests of where a
/// tip stands read it, so they wait on nothing TipKit decides.
final class TipsRecorded: TipCapability {
    private(set) var tips: [(content: TipContent, anchor: TipAnchor?)] = []
    private(set) var withdrawn: [ObjectIdentifier] = []

    func update(_ tips: [(content: TipContent, anchor: TipAnchor?)], owner: AnyObject, store: URL?) { self.tips = tips }
    func withdraw(owner: AnyObject) { withdrawn.append(ObjectIdentifier(owner)); tips = [] }
    func state(owner: AnyObject) -> [[String: Any]] { [] }

    /// The tips' ids, in the order the core gave them.
    var ids: [String] { tips.map(\.content.id) }
    /// Whether the tip `id` has somewhere to show.
    func anchored(_ id: String) -> Bool { tips.first { $0.content.id == id }?.anchor != nil }

    /// Install a recorder for this test, and the capability that was there again after it.
    static func install(in test: XCTestCase) -> TipsRecorded {
        let before = TipLink.installed, recorded = TipsRecorded()
        TipLink.installed = recorded
        test.addTeardownBlock { TipLink.installed = before }
        return recorded
    }
}
