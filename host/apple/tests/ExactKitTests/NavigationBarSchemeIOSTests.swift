#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

extension NavigationBarIOSTests {
    /// A readout of a text in an authored size and ink is hosted, its own views in the bar,
    /// which take the window's scheme as the page's do, though the app sends no batch (LLP
    /// 1141.004 D1).
    func testAHostedFaceFollowsTheSchemeAsThePageDoes() throws {
        let session = try fixture("scheme-face", module: false, plan: "BAR_FACE_SCHEME")
        let nav = try XCTUnwrap(session.presenter.navigationHost.primaryNavigation)
        let navigationItem = try XCTUnwrap(nav.topViewController?.navigationItem)
        let item = try XCTUnwrap(((navigationItem.leftBarButtonItems ?? []) + (navigationItem.rightBarButtonItems ?? [])).first { $0.accessibilityIdentifier == "title-home" })
        let view = try XCTUnwrap(item.customView as? HostedItemView, "hosted, drawn as written")
        let window = try XCTUnwrap(self.window)
        window.overrideUserInterfaceStyle = .light
        until("its views are light") { view.node.traitCollection.userInterfaceStyle == .light }
        window.overrideUserInterfaceStyle = .dark
        defer { window.overrideUserInterfaceStyle = .unspecified }
        until("its views are dark with the window") { view.node.traitCollection.userInterfaceStyle == .dark }
    }
}
#endif
