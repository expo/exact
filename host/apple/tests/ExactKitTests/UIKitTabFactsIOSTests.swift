#if os(iOS)
import UIKit
import XCTest

/// What UIKit's own tab container and navigation controllers do, with no
/// Exact, that Exact's tabs follow (LLP 1075.003 §3.7, measured in §9.16):
/// when a pop is committed, what a tab tapped during one leaves, where More's
/// pop to its list puts a tab's root, and which tab UIKit reports selected
/// at More's calls. A change in UIKit fails here first. Six tabs in a scene's
/// window, each a navigation controller over its root: an iPhone's bar shows
/// four and More.
final class UIKitTabFactsIOSTests: XCTestCase {
    private var window: UIWindow?
    private var tabs: UITabBarController!
    private var navs: [UINavigationController] = []
    private let watcher = SelectionWatcher()

    override func tearDown() {
        window?.isHidden = true
        window = nil
        UIView.setAnimationsEnabled(true)
        super.tearDown()
    }

    /// UIKit's Back, an unanimated pop and a tap on the selected tab above
    /// its root are committed when they are asked for: `viewControllers` is
    /// short when the call returns, and the popped controller is still the
    /// stack's child then.
    func testAPopIsCommittedWhenItIsAskedFor() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "UIKit animates a pop as in an app only in a scene's window") }
        build(scene)
        let home = navs[0], root = home.viewControllers[0]
        let shown = UIViewController()
        home.pushViewController(shown, animated: false)
        settle()
        home.popViewController(animated: true)
        XCTAssertEqual(home.viewControllers, [root], "UIKit's Back: short at once")
        XCTAssertEqual(home.transitionCoordinator?.initiallyInteractive, false, "with its transition already")
        XCTAssertTrue(shown.parent === home, "the popped controller is still the stack's child")
        settle()
        XCTAssertNil(shown.parent, "and gone once the pop ends")
        let next = UIViewController()
        home.pushViewController(next, animated: false)
        settle()
        home.popViewController(animated: false)
        XCTAssertEqual(home.viewControllers, [root], "an unanimated pop: short at once")
        home.setViewControllers([root, UIViewController(), UIViewController()], animated: false)
        settle()
        tapTab(tabs, 0)
        XCTAssertEqual(home.viewControllers, [root], "a tap on the selected tab: short by its didSelect")
        XCTAssertNotNil(home.transitionCoordinator)
        settle()
    }

    /// UIKit gives More no delegate of its own: More's slot is empty until
    /// someone sets one, also while More shows its list.
    func testMoreHasNoDelegateOfUIKits() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "More shows its list as in an app only in a scene's window") }
        build(scene, watching: false)
        XCTAssertNil(tabs.moreNavigationController.delegate, "More starts with no delegate")
        tabs.selectedViewController = tabs.moreNavigationController
        waitFor("More shows its list") { tabs.moreNavigationController.viewIfLoaded?.window != nil && tabs.moreNavigationController.transitionCoordinator == nil }
        XCTAssertNil(tabs.moreNavigationController.delegate, "and has none while it shows its list")
    }

    /// A swipe leaves `viewControllers` short at once too, but is committed
    /// only when it completes: a cancelled one restores its source.
    func testASwipeIsCommittedOnlyWhenItCompletes() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "UIKit runs an interactive pop as in an app only in a scene's window") }
        build(scene)
        let home = navs[0], root = home.viewControllers[0]
        let source = UIViewController()
        home.pushViewController(source, animated: false)
        settle()
        watcher.pop.interactive = true
        defer { watcher.pop.interactive = false }
        home.popViewController(animated: true)
        XCTAssertEqual(home.viewControllers, [root], "short at once")
        XCTAssertEqual(home.transitionCoordinator?.initiallyInteractive, true, "in an interactive transition")
        waitForAnimationsToStart(home)
        watcher.pop.interaction.update(0.3)
        watcher.pop.interaction.cancel()
        settle()
        XCTAssertEqual(home.viewControllers, [root, source], "a cancelled swipe restores its source")
        home.popViewController(animated: true)
        waitForAnimationsToStart(home)
        watcher.pop.interaction.update(0.6)
        watcher.pop.interaction.finish()
        settle()
        XCTAssertEqual(home.viewControllers, [root], "a completed swipe keeps it out")
        XCTAssertNil(source.parent)
    }

    /// A tab on the bar tapped in the turn a pop is asked for, before the
    /// pop's `willShow`: UIKit drops the transition, which the stack keeps
    /// off screen until the tab shows again, and the popped controller stays
    /// its child, hears no appearance call and has its view in no view.
    /// Tapped once the pop's animations started, the pop ends off screen and
    /// the controller leaves.
    func testATabTappedDuringAPopLeavesItOffScreen() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "UIKit animates a pop and a tab change as in an app only in a scene's window") }
        build(scene)
        let home = navs[0], root = home.viewControllers[0]
        let popped = AppearanceRecorder()
        home.pushViewController(popped, animated: false)
        settle()
        popped.calls = []
        home.popViewController(animated: true)
        tapTab(tabs, 1)
        pause(1)
        XCTAssertNotNil(home.transitionCoordinator, "in the same turn: the dropped transition stays")
        XCTAssertNil(home.viewIfLoaded?.window, "on a stack off screen")
        XCTAssertEqual(home.viewControllers, [root])
        XCTAssertTrue(popped.parent === home, "and the popped controller stays its child")
        let afterTap = popped.calls, inWindowAfterTap = popped.viewIfLoaded?.window != nil
        tapTab(tabs, 0)
        settle()
        XCTAssertNil(home.transitionCoordinator, "until the tab shows again")
        XCTAssertEqual(afterTap, [], "the popped controller hears no appearance call")
        XCTAssertFalse(inWindowAfterTap, "and its view is in no window")
        XCTAssertEqual(popped.calls, [], "nor once the tab shows again")
        XCTAssertNil(popped.viewIfLoaded?.superview, "when its view is in no view")
        let later = UIViewController()
        home.pushViewController(later, animated: false)
        settle()
        home.popViewController(animated: true)
        waitForAnimationsToStart(home)
        tapTab(tabs, 1)
        settle()
        XCTAssertNil(home.transitionCoordinator, "once its animations started: the pop ends off screen")
        XCTAssertNil(later.parent)
        XCTAssertEqual(home.viewControllers, [root])
    }

    /// More tapped again above a tab's routes pops More to its list: the
    /// routes leave More at once, the tab's root is in no controller while
    /// the pop runs, and as it ends UIKit puts the root back in the tab's own
    /// controller and drops the routes above it, also when a tab on the bar
    /// is tapped during the pop.
    func testMoresPopToItsListPutsTheRootBackInItsTabsOwnController() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "UIKit animates More's pop as in an app only in a scene's window") }
        for tapsHome in [false, true] {
            build(scene)
            let more = tabs.moreNavigationController, five = navs[4], root = five.viewControllers[0]
            let deeper = deeperUnderMore()
            tapMoreAgain()
            XCTAssertEqual(more.viewControllers.count, 1, "the routes leave More at once")
            XCTAssertNil(root.parent, "the root is in no controller")
            if tapsHome {
                waitForAnimationsToStart(more)
                tapTab(tabs, 0)
            }
            settle()
            XCTAssertEqual(five.viewControllers, [root], "the root is back in its tab's own controller")
            XCTAssertNil(deeper.parent, "and the route above it is gone")
        }
    }

    /// Which tab UIKit reports selected at More's `willShow`: the row's for a
    /// row picked; for a tap on More from another tab, that tab until
    /// `didSelect`; for a tab under More selected in code, the tab selected
    /// before at the `willShow` within the setter.
    func testTheSelectionMoresWillShowSees() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "UIKit shows More's tabs as in an app only in a scene's window") }
        build(scene)
        let more = tabs.moreNavigationController
        _ = deeperUnderMore()
        XCTAssertTrue(watcher.selectedAtWillShow.contains { $0 === navs[4] }, "a row picked: the row's tab")
        tapTab(tabs, 0)
        settle()
        watcher.selectedAtWillShow = []
        watcher.selectedAtDidSelect = []
        tapTab(tabs, of: more)
        settle()
        XCTAssertTrue((watcher.selectedAtWillShow.first ?? nil) === navs[0], "a tap on More: the tab tapped from at More's willShow")
        XCTAssertTrue((watcher.selectedAtDidSelect.first ?? nil) === navs[4], "and More's tab at didSelect")
        tapTab(tabs, 0)
        settle()
        watcher.selectedAtWillShow = []
        tabs.selectedViewController = navs[4]
        XCTAssertTrue((watcher.selectedAtWillShow.first ?? nil) === navs[0], "a selection in code: the tab before, within the setter")
        settle()
    }

    /// A tab under More whose root and a route above it More holds, selected
    /// again while another tab shows. More selected, as the tap helper selects
    /// it, keeps More's stack. The tab's own controller selected in code keeps
    /// it too, unless the route hides the tab bar: then More drops the route
    /// within the setter, shows its list and the tab's root again, and the
    /// route is in no controller.
    func testSelectingATabWhoseRoutesMoreHolds() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "UIKit shows More's tabs as in an app only in a scene's window") }
        for (hides, viaMore) in [(false, false), (true, false), (false, true), (true, true)] {
            build(scene)
            let more = tabs.moreNavigationController, five = navs[4], root = five.viewControllers[0], list = more.viewControllers[0]
            let deeper = UIViewController()
            deeper.hidesBottomBarWhenPushed = hides
            tapTab(tabs, of: more)
            settle()
            tabs.selectedViewController = five
            settle()
            more.pushViewController(deeper, animated: false)
            settle()
            tapTab(tabs, 0)
            settle()
            watcher.shownAtWillShow = []
            if viaMore { tapTab(tabs, of: more) } else { tabs.selectedViewController = five }
            settle()
            let what = "\(viaMore ? "More" : "the tab's controller") selected, the route \(hides ? "hiding" : "keeping") the tab bar"
            if hides && !viaMore {
                XCTAssertTrue(more.viewControllers.elementsEqual([list, root], by: ===), "\(what): More drops the route")
                XCTAssertTrue(watcher.shownAtWillShow.elementsEqual([deeper, list, root], by: ===), "\(what): More shows its list, then the tab's root")
                XCTAssertNil(deeper.parent, "\(what): the route is in no controller")
            } else {
                XCTAssertTrue(more.viewControllers.elementsEqual([list, root, deeper], by: ===), "\(what): More keeps its stack")
                XCTAssertTrue(deeper.parent === more, "\(what): the route stays More's")
            }
        }
    }

    /// Five's Deeper over Five's root in More, Five picked from More's list.
    @discardableResult
    private func deeperUnderMore() -> UIViewController {
        let more = tabs.moreNavigationController
        tapTab(tabs, of: more)
        settle()
        tabs.selectedViewController = navs[4]
        settle()
        let deeper = UIViewController()
        more.pushViewController(deeper, animated: false)
        settle()
        return deeper
    }

    /// More's item tapped while More shows a tab's routes, as UIKit performs it (§9.16): `shouldSelect(More)`,
    /// `didSelect` of the tab it shows, then More's animated pop to its list.
    private func tapMoreAgain() {
        let more = tabs.moreNavigationController
        _ = tabs.delegate?.tabBarController?(tabs, shouldSelect: more)
        tabs.delegate?.tabBarController?(tabs, didSelect: tabs.selectedViewController!)
        more.popToRootViewController(animated: true)
    }

    /// UIKit reads which optional methods a navigation controller's delegate
    /// answers once, when the delegate is set: a method the delegate answered
    /// then is called though the delegate no longer answers it, and one it did
    /// not answer is never asked for though it answers it now. A delegate whose
    /// answers change is set again (LLP 1075.003 §9.16).
    func testADelegatesAnswersAreReadOnceWhenItIsSet() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "UIKit asks for a push's animator as in an app only in a scene's window") }
        build(scene)
        let home = navs[0]
        let answered = Answering()
        home.delegate = answered
        answered.answers = false
        home.pushViewController(UIViewController(), animated: true)
        settle()
        XCTAssertEqual(answered.queries, 1, "asked once, when it was set")
        XCTAssertEqual(answered.calls, 1, "and its animator called though it no longer answers for it")
        let unanswered = Answering()
        unanswered.answers = false
        home.delegate = unanswered
        unanswered.answers = true
        home.popViewController(animated: true)
        settle()
        XCTAssertEqual(unanswered.queries, 1, "asked once, when it was set")
        XCTAssertEqual(unanswered.calls, 0, "and never called though it answers for it now")
    }

    /// A tab container reads its delegate's answers once too: an orientation
    /// method the delegate answered when it was set is called though the
    /// delegate no longer answers it.
    func testATabDelegatesAnswersAreReadOnceWhenItIsSet() throws {
        let tabs = UITabBarController()
        let answered = TabAnswering()
        tabs.delegate = answered
        answered.answers = false
        XCTAssertEqual(tabs.supportedInterfaceOrientations, .portrait, "its answer, though it no longer answers for it")
        XCTAssertEqual(answered.queries, 1, "asked once, when it was set")
        XCTAssertEqual(answered.calls, 1)
    }

    /// Enough tabs that UIKit shows More, in a window; with `watching`, every
    /// delegate slot, More's included, is the watcher's.
    private func build(_ scene: UIWindowScene, watching: Bool = true) {
        window?.isHidden = true
        let window = UIWindow(windowScene: scene)
        window.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        tabs = UITabBarController()
        navs = (0..<6).map { index in
            let nav = UINavigationController(rootViewController: UIViewController())
            nav.tabBarItem = UITabBarItem(title: "Tab \(index)", image: UIImage(systemName: "circle"), tag: index)
            if watching { nav.delegate = watcher }
            return nav
        }
        if watching {
            watcher.tabs = tabs
            tabs.delegate = watcher
        }
        tabs.setViewControllers(navs, animated: false)
        window.rootViewController = tabs
        window.makeKeyAndVisible()
        self.window = window
        if watching { tabs.moreNavigationController.delegate = watcher }
        UIView.setAnimationsEnabled(true)
        waitFor("the container is in the window") { tabs.selectedViewController?.viewIfLoaded?.window != nil }
        settle()
    }

    /// Waits until no stack shown runs a transition, then a moment more.
    private func settle() {
        waitFor("no transition runs", cap: 10) {
            ([tabs.moreNavigationController] + navs.filter { $0.viewIfLoaded?.window != nil }).allSatisfy { $0.transitionCoordinator == nil }
        }
        pause(0.6)
    }

    private func pause(_ seconds: TimeInterval) {
        let until = Date().addingTimeInterval(seconds)
        waitFor("\(seconds) s pass") { Date() >= until }
    }
}

/// The tab and navigation delegate: the tab UIKit reports selected at More's
/// `willShow` and at `didSelect`, and an interactive pop when asked for.
private final class SelectionWatcher: NSObject, UINavigationControllerDelegate, UITabBarControllerDelegate {
    weak var tabs: UITabBarController?
    var selectedAtWillShow: [UIViewController?] = []
    var selectedAtDidSelect: [UIViewController?] = []
    var shownAtWillShow: [UIViewController] = []
    let pop = InteractivePop()

    func navigationController(_ nav: UINavigationController, willShow vc: UIViewController, animated: Bool) {
        if nav === tabs?.moreNavigationController { selectedAtWillShow.append(tabs?.selectedViewController); shownAtWillShow.append(vc) }
    }

    func navigationController(_ nav: UINavigationController, animationControllerFor operation: UINavigationController.Operation,
                              from: UIViewController, to: UIViewController) -> UIViewControllerAnimatedTransitioning? {
        pop.interactive ? pop : nil
    }

    func navigationController(_ nav: UINavigationController,
                              interactionControllerFor animator: UIViewControllerAnimatedTransitioning) -> UIViewControllerInteractiveTransitioning? {
        pop.interactive ? pop.interaction : nil
    }

    func tabBarController(_ tabs: UITabBarController, didSelect vc: UIViewController) {
        selectedAtDidSelect.append(tabs.selectedViewController)
    }
}
/// A controller that records the appearance calls UIKit sends it.
private final class AppearanceRecorder: UIViewController {
    var calls: [String] = []
    override func viewWillDisappear(_ animated: Bool) { super.viewWillDisappear(animated); calls.append("willDisappear") }
    override func viewDidDisappear(_ animated: Bool) { super.viewDidDisappear(animated); calls.append("didDisappear") }
    override func viewWillAppear(_ animated: Bool) { super.viewWillAppear(animated); calls.append("willAppear") }
    override func viewDidAppear(_ animated: Bool) { super.viewDidAppear(animated); calls.append("didAppear") }
}

/// A navigation delegate whose answer for the animator method can change, counting UIKit's questions and calls.
private final class Answering: NSObject, UINavigationControllerDelegate {
    var answers = true, queries = 0, calls = 0
    override func responds(to selector: Selector!) -> Bool {
        if selector == #selector(UINavigationControllerDelegate.navigationController(_:animationControllerFor:from:to:)) { queries += 1; return answers }
        return super.responds(to: selector)
    }
    func navigationController(_ navigationController: UINavigationController, animationControllerFor operation: UINavigationController.Operation,
                              from fromVC: UIViewController, to toVC: UIViewController) -> UIViewControllerAnimatedTransitioning? {
        calls += 1
        return nil
    }
}

/// A tab delegate whose answer for the orientation method can change, counting UIKit's questions and calls.
final class TabAnswering: NSObject, UITabBarControllerDelegate {
    var answers = true, queries = 0, calls = 0
    /// Run once, the first time UIKit asks whether it answers for the orientations
    var asked: (() -> Void)?
    override func responds(to selector: Selector!) -> Bool {
        guard selector == #selector(UITabBarControllerDelegate.tabBarControllerSupportedInterfaceOrientations(_:)) else { return super.responds(to: selector) }
        queries += 1
        if let asked { self.asked = nil; asked() }
        return answers
    }
    func tabBarControllerSupportedInterfaceOrientations(_ tabBarController: UITabBarController) -> UIInterfaceOrientationMask {
        calls += 1
        return .portrait
    }
}
#endif
