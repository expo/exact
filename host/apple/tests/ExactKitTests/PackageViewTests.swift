#if os(iOS) || os(tvOS)
import UIKit
import XCTest
import ExactPackage

/// A package's view is plain unless it names the controller whose view it
/// is (#1609): only then does the host contain a controller for its node.
final class PackageViewTests: XCTestCase {
    private final class Plain: ExactPackageView {
        let view = UIView()
        func setProps(_ props: [String: String]) throws {}
    }

    private final class Held: ExactPackageView {
        let held = UIViewController()
        var view: UIView { held.view }
        var controller: UIViewController? { held }
        func setProps(_ props: [String: String]) throws {}
    }

    func testAViewHasAControllerOnlyWhenItNamesOne() {
        XCTAssertNil((Plain() as any ExactPackageView).controller)
        let held = Held()
        XCTAssertTrue((held as any ExactPackageView).controller === held.held)
    }
}
#endif
