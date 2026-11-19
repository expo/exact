import ExactComposition
import ExactTestsInstall
import XCTest

/// Installs the development composition (`build.mjs --test` names it in
/// `EXACT_APPLE_LINK`) as the bundle loads, from `ExactTestsInstall`'s
/// constructor, and on iOS the observer that takes each test's windows out
/// of their scenes as the test ends (`TestWindows`). A test of a capability
/// development links only by use installs that one itself.
@_cdecl("exact_tests_install")
func installTestComposition() {
    ExactComposition.installCapabilities()
    #if os(iOS)
    TestWindows.install()
    #endif
    exact_tests_installed = 1
}

final class TestsInstallTests: XCTestCase {
    /// The constructor ran before this test, in every test process.
    func testTheCompositionIsInstalledAsTheBundleLoads() {
        XCTAssertEqual(exact_tests_installed, 1)
    }
}
