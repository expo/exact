#if os(iOS)
@testable import ExactKit
import UIKit
import XCTest

/// An arranged pane's safe area (LLP 1123 D11): the band nested containment withholds from it is
/// Exact's contribution on its holder's ledger, beside what the app adds there.
extension ContentOwnersIOSTests {
    func testAnAppsOwnInsetOnAnArrangedPaneStaysThroughItsSafeAreaPasses() throws {
        guard #available(iOS 27.1, *) else { throw XCTSkip("arrangements require iOS 27.1") }
        guard let scene = hostedWindowScene() else { throw missing("scene", "arranged panes take their safe area in a window") }
        let (session, _, _, _, _, _) = try arrangedPeers(in: scene)
        let p = session.presenter
        let holder = try XCTUnwrap(p.arrangements.entries.values.first?.holders.first)
        holder.additionalSafeAreaInsets.left += 6
        for _ in 0..<2 { p.arrangements.sync() }
        XCTAssertEqual(holder.additionalSafeAreaInsets.left, 6, accuracy: 0.5, "an app's own inset on a pane stays")
        holder.additionalSafeAreaInsets.left -= 6
    }
}
#endif
