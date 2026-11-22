#if os(iOS)
import UIKit
import XCTest

/// What UIKit's tab bar accessory (`UITabBarController.bottomAccessory`, iOS
/// 26) does with no Exact, which the tab accessory follows (LLP 1075.003
/// §9.17): UIKit decides the content view's box whatever the content asks
/// for, counts it in every tab's safe area, and keeps it, inline, while the
/// bar hides. A change in UIKit fails here first. Three tabs in a scene's
/// window, each a navigation controller over a root whose scroll view is its
/// content scroll view.
final class UIKitTabAccessoryFactsIOSTests: XCTestCase {
    private var window: UIWindow?
    private var tabs: UITabBarController!

    override func tearDown() {
        window?.isHidden = true
        window = nil
        super.tearDown()
    }

    func testUIKitSizesTheAccessoryAndCountsItInEveryTabsSafeArea() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("bottomAccessory is iOS 26") }
        guard let scene = hostedWindowScene() else { throw missing("scene", "UIKit lays the accessory out as in an app only in a scene's window") }
        build(scene)
        let root = try XCTUnwrap((tabs.selectedViewController as? UINavigationController)?.topViewController)
        let without = root.view.safeAreaInsets.bottom
        var boxes: [CGRect] = []
        for height in [60, 30, nil] as [CGFloat?] {
            let content = Sized(height: height)
            tabs.setBottomAccessory(UITabAccessory(contentView: content), animated: false)
            settle("the accessory is laid out") { content.window != nil && content.bounds.height > 0 && root.view.safeAreaInsets.bottom > without }
            boxes.append(content.convert(content.bounds, to: nil))
            XCTAssertEqual(content.traitCollection.tabAccessoryEnvironment, .regular)
            let gained = root.view.safeAreaInsets.bottom - without
            XCTAssertGreaterThanOrEqual(gained, content.bounds.height, "the routes' safe area holds the accessory")
            let scroll = try XCTUnwrap(root.view.subviews.first as? UIScrollView)
            XCTAssertEqual(scroll.adjustedContentInset.bottom, root.view.safeAreaInsets.bottom, "and the scroller's inset follows it")
        }
        XCTAssertEqual(Set(boxes.map(\.size)).count, 1, "one box whatever the content asks for: \(boxes)")
        XCTAssertGreaterThan(boxes[0].height, 0)
        if UIDevice.current.userInterfaceIdiom == .phone {
            XCTAssertLessThanOrEqual(boxes[0].maxY, tabs.tabBar.frame.minY, "above the bar on the iPhone")
        }
        tabs.selectedIndex = 1
        settle("the second tab shows") { (self.tabs.selectedViewController as? UINavigationController)?.topViewController?.viewIfLoaded?.window != nil }
        let second = try XCTUnwrap((tabs.selectedViewController as? UINavigationController)?.topViewController)
        XCTAssertEqual(second.view.safeAreaInsets.bottom, root.view.safeAreaInsets.bottom, "every tab clears it")
    }

    func testTheAccessoryStaysInlineWhereTheBarWasWhileTheBarHides() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("bottomAccessory is iOS 26") }
        guard UIDevice.current.userInterfaceIdiom == .phone else { throw XCTSkip("an iPhone's bottom tab bar") }
        guard let scene = hostedWindowScene() else { throw missing("scene", "UIKit lays the accessory out as in an app only in a scene's window") }
        build(scene)
        let content = Sized(height: nil)
        tabs.setBottomAccessory(UITabAccessory(contentView: content), animated: false)
        settle("the accessory is laid out") { content.window != nil && content.bounds.height > 0 }
        let above = content.convert(content.bounds, to: nil)
        tabs.setTabBarHidden(true, animated: false)
        settle("the accessory goes inline") { content.traitCollection.tabAccessoryEnvironment == .inline && content.convert(content.bounds, to: nil) != above }
        let inline = content.convert(content.bounds, to: nil)
        XCTAssertEqual(content.traitCollection.tabAccessoryEnvironment, .inline)
        XCTAssertGreaterThan(inline.minY, above.minY, "lower, where the bar was")
        XCTAssertLessThan(inline.width, above.width, "and narrower")
        let nav = try XCTUnwrap(tabs.selectedViewController as? UINavigationController)
        tabs.setTabBarHidden(false, animated: false)
        let pushed = UIViewController()
        pushed.hidesBottomBarWhenPushed = true
        nav.pushViewController(pushed, animated: false)
        settle("the route that hides the bar shows") { pushed.viewIfLoaded?.window != nil && nav.transitionCoordinator == nil && content.convert(content.bounds, to: nil) == inline }
        XCTAssertEqual(content.convert(content.bounds, to: nil), inline, "a route that hides the bar keeps it inline too")
    }

    private func build(_ scene: UIWindowScene) {
        let window = UIWindow(windowScene: scene)
        window.frame = scene.coordinateSpace.bounds
        tabs = UITabBarController()
        let navs = (0..<3).map { index in
            let root = UIViewController()
            root.view.backgroundColor = .systemBackground
            let scroll = UIScrollView(frame: root.view.bounds)
            scroll.autoresizingMask = [.flexibleWidth, .flexibleHeight]
            scroll.contentSize = CGSize(width: 10, height: 4000)
            root.view.addSubview(scroll)
            root.setContentScrollView(scroll, for: .bottom)
            let nav = UINavigationController(rootViewController: root)
            nav.tabBarItem = UITabBarItem(title: "Tab \(index)", image: UIImage(systemName: "circle"), tag: index)
            return nav
        }
        tabs.setViewControllers(navs, animated: false)
        window.rootViewController = tabs
        window.makeKeyAndVisible()
        self.window = window
        waitFor("the container is in the window") { tabs.selectedViewController?.viewIfLoaded?.window != nil }
    }

    /// Lays the container out and waits for `done`, which UIKit's own passes bring.
    private func settle(_ what: String, _ done: () -> Bool) {
        tabs.view.layoutIfNeeded()
        waitFor(what) { tabs.view.layoutIfNeeded(); return done() }
    }
}

/// A content view with an optional intrinsic height.
private final class Sized: UIView {
    let height: CGFloat?
    init(height: CGFloat?) {
        self.height = height
        super.init(frame: .zero)
    }
    required init?(coder: NSCoder) { nil }
    override var intrinsicContentSize: CGSize { CGSize(width: UIView.noIntrinsicMetric, height: height ?? UIView.noIntrinsicMetric) }
}
#endif
