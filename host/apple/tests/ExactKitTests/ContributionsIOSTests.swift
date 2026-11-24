#if os(iOS)
@testable import ExactKit
import UIKit
import XCTest

/// Exact's writes to a controller's additional insets and their effects as UIKit gives them (LLP
/// 1123 D11, "Provenance"): each measured with the contributions before it, the app's own write
/// kept as the baseline, and Exact's reactions held until the writes settle.
@MainActor
final class ContributionsIOSTests: XCTestCase {
    private var window: UIWindow?

    override func tearDown() {
        window?.isHidden = true
        window = nil
        super.tearDown()
    }

    func testAChildsEffectsAreWhatUIKitGivesEachPrefix() throws {
        let (root, child) = try controllers()
        let inherited = root.view.safeAreaInsets.top
        let ledger = ContributionsIOS.of(child)
        // Plus 20, then minus 30: a child's insets do not fall below what it inherits, so the
        // second takes back only the 20 the first added
        ledger.set(id(1), top(20), generation: 1)
        ledger.set(id(2), top(-30), generation: 1)
        ledger.apply()
        XCTAssertEqual(ledger.entries.map(\.effect), [top(20), top(-20)])
        XCTAssertEqual(child.view.safeAreaInsets.top, inherited)
        XCTAssertEqual(child.additionalSafeAreaInsets, top(-10))
    }

    func testAWindowRootsNegativeEffectIsTheWholeWrite() throws {
        let (root, _) = try controllers()
        // The app's 40 on top, then Exact's −50: UIKit takes a window root's insets below 0, so the
        // effect is the whole −50 whatever the window's own inset
        root.additionalSafeAreaInsets = top(40)
        let baseline = root.view.safeAreaInsets.top
        let ledger = ContributionsIOS.of(root)
        ledger.set(id(1), top(-50), generation: 1)
        ledger.apply()
        XCTAssertEqual(ledger.entries.map(\.effect), [top(-50)])
        XCTAssertEqual(root.view.safeAreaInsets.top, baseline - 50)
        XCTAssertEqual(root.additionalSafeAreaInsets, top(-10))
    }

    func testAWindowRootsInsetsGoNegativePastWhatItHas() throws {
        let (root, _) = try controllers()
        let window = root.view.safeAreaInsets.top
        let ledger = ContributionsIOS.of(root)
        // Exact's −(window + 40) reads −40, the effect the whole write
        ledger.set(id(1), top(-(window + 40)), generation: 1)
        ledger.apply()
        XCTAssertEqual(root.view.safeAreaInsets.top, -40)
        XCTAssertEqual(ledger.entries.map(\.effect), [top(-(window + 40))])
        // The app adds 20: −20, the effect unchanged
        root.additionalSafeAreaInsets.top += 20
        ledger.apply()
        XCTAssertEqual(root.view.safeAreaInsets.top, -20)
        XCTAssertEqual(ledger.entries.map(\.effect), [top(-(window + 40))])
        // The sampler reports the band as UIKit gives it
        let update = OwnerUpdateIOS.sample(controller: root, view: root.view, session: nil)
        XCTAssertEqual(update.band?.top, -20)
    }

    func testTheAppsOwnWriteStaysAndIsNoContribution() throws {
        let (_, child) = try controllers()
        child.additionalSafeAreaInsets = UIEdgeInsets(top: 0, left: 5, bottom: 0, right: 0)
        let ledger = ContributionsIOS.of(child)
        ledger.set(id(1), top(20), generation: 1)
        ledger.apply()
        XCTAssertEqual(child.additionalSafeAreaInsets, UIEdgeInsets(top: 20, left: 5, bottom: 0, right: 0))
        XCTAssertEqual(ledger.entries.map(\.effect), [top(20)])
        // The app adds 4 on the left over Exact's part, then Exact's goes: the app's 9 stays
        child.additionalSafeAreaInsets.left += 4
        ledger.set(id(1), nil, generation: 2)
        ledger.apply()
        XCTAssertEqual(child.additionalSafeAreaInsets, UIEdgeInsets(top: 0, left: 9, bottom: 0, right: 0))
        XCTAssertTrue(ledger.entries.isEmpty)
    }

    func testAHostsOwnWriteTakesANewerGenerationAndZeroTakesItAway() throws {
        let (_, child) = try controllers()
        child.additionalSafeAreaInsets = UIEdgeInsets(top: 0, left: 5, bottom: 0, right: 0)
        let ledger = ContributionsIOS.of(child)
        ledger.write(id(1), top(20))
        XCTAssertEqual(child.additionalSafeAreaInsets, UIEdgeInsets(top: 20, left: 5, bottom: 0, right: 0))
        XCTAssertEqual(ledger.insets(of: id(1)), top(20))
        let generation = try XCTUnwrap(ledger.entries.first?.generation)
        // A write as before records nothing; a new one is newer than every one before it
        ledger.write(id(1), top(20))
        XCTAssertEqual(ledger.entries.first?.generation, generation)
        ledger.write(id(1), top(30))
        XCTAssertGreaterThan(try XCTUnwrap(ledger.entries.first?.generation), generation)
        XCTAssertFalse(ledger.set(id(1), top(40), generation: generation), "an older generation is refused")
        // Zero takes it away, the app's part staying
        ledger.write(id(1), .zero)
        XCTAssertTrue(ledger.entries.isEmpty)
        XCTAssertEqual(child.additionalSafeAreaInsets, UIEdgeInsets(top: 0, left: 5, bottom: 0, right: 0))
    }

    func testExactsReactionsReadOnlyTheSettledInsets() throws {
        let (_, child) = try controllers()
        let ledger = ContributionsIOS.of(child)
        for (i, value) in [40, -10, 25].enumerated() { ledger.set(id(UInt32(i)), top(CGFloat(value)), generation: 1) }
        child.reactions = []
        ledger.apply()
        // UIKit called the child at each step; its reaction ran once, after the last
        XCTAssertGreaterThan(child.calls, 1)
        XCTAssertEqual(child.reactions, [child.view.safeAreaInsets])
    }

    func testAReactionThatSamplesTheOwnerFindsTheLedgerSettled() throws {
        let (_, child) = try controllers()
        let ledger = ContributionsIOS.of(child)
        var samples: [OwnerUpdateIOS] = []
        child.react = { samples.append(OwnerUpdateIOS.sample(controller: child, view: child.view, session: nil)) }
        ledger.set(id(1), top(20), generation: 1)
        ledger.apply()
        XCTAssertEqual(samples.map { $0.contributions.map(\.effect) }, [[top(20)]])
        XCTAssertEqual(child.additionalSafeAreaInsets, top(20))
    }

    func testAnApplyDuringItsOwnLayoutPassSettles() throws {
        let (root, child) = try controllers()
        let ledger = ContributionsIOS.of(child)
        child.layout = { ledger.apply() }
        ledger.set(id(1), top(20), generation: 1)
        child.view.setNeedsLayout()
        root.view.layoutIfNeeded()
        root.view.layoutIfNeeded()
        XCTAssertEqual(ledger.entries.map(\.effect), [top(20)])
        XCTAssertEqual(child.additionalSafeAreaInsets, top(20))
    }

    func testNothingIsWrittenAgainWhileNothingItWasMeasuredAgainstChanged() throws {
        let (_, child) = try controllers()
        let ledger = ContributionsIOS.of(child)
        ledger.set(id(1), top(20), generation: 1)
        ledger.apply()
        child.calls = 0
        ledger.apply()
        XCTAssertEqual(child.calls, 0)
        // A stale generation is refused and changes nothing, nor adds back one taken away
        ledger.set(id(1), top(30), generation: 3)
        XCTAssertFalse(ledger.set(id(1), top(99), generation: 2))
        XCTAssertEqual(ledger.entries.map(\.insets), [top(30)])
        ledger.set(id(1), nil, generation: 4)
        XCTAssertFalse(ledger.set(id(1), top(99), generation: 3))
        XCTAssertTrue(ledger.entries.isEmpty)
    }

    func testAnAppsWriteDuringAStepIsMeasuredFromItsBaseline() throws {
        let (_, child) = try controllers()
        let ledger = ContributionsIOS.of(child)
        // The app adds 10 to its own insets from the callback Exact's write raises, once
        var wrote = false
        child.during = {
            guard !wrote else { return }
            wrote = true
            child.additionalSafeAreaInsets.top += 10
        }
        ledger.set(id(1), top(20), generation: 1)
        ledger.apply()
        XCTAssertTrue(wrote)
        XCTAssertEqual(ledger.entries.map(\.effect), [top(20)])
        XCTAssertEqual(child.additionalSafeAreaInsets, top(30))
    }

    func testACallTheReleaseMakesRunsOnceMore() throws {
        let (_, child) = try controllers()
        let ledger = ContributionsIOS.of(child)
        // A reaction the first measurement releases records a second contribution and applies it
        var asked = false
        child.react = {
            guard !asked else { return }
            asked = true
            ledger.set(self.id(2), self.top(10), generation: 1)
            ledger.apply()
        }
        ledger.set(id(1), top(20), generation: 1)
        ledger.apply()
        XCTAssertEqual(ledger.entries.map(\.effect), [top(20), top(10)])
        XCTAssertEqual(child.additionalSafeAreaInsets, top(30))
    }

    func testAFlooredWriteAppliedFromEachOfItsLayoutPassesWritesOnce() throws {
        let (root, child) = try controllers()
        let ledger = ContributionsIOS.of(child)
        // −50 under the app's 20, so the floor holds part of it, applied at each of the child's
        // layout passes: it measures, then stops writing
        child.additionalSafeAreaInsets = top(20)
        child.layout = { ledger.apply() }
        ledger.set(id(1), top(-50), generation: 1)
        child.view.setNeedsLayout()
        root.view.layoutIfNeeded()
        XCTAssertEqual(ledger.entries.map(\.effect), [top(-20)])
        let calls = child.calls
        for _ in 0..<3 {
            child.view.setNeedsLayout()
            root.view.layoutIfNeeded()
        }
        XCTAssertEqual(child.calls, calls)
    }

    func testAContributionTakenAwayDuringItsOwnWriteIsMeasuredWhenTheWritesEnd() throws {
        let (_, child) = try controllers()
        let ledger = ContributionsIOS.of(child)
        // The callback Exact's write raises takes that very contribution away
        var once = false
        child.during = {
            guard !once else { return }
            once = true
            ledger.set(self.id(1), nil, generation: 2)
        }
        ledger.set(id(1), top(20), generation: 1)
        ledger.apply()
        XCTAssertEqual(ledger.entries, [])
        XCTAssertEqual(child.additionalSafeAreaInsets, .zero)
    }

    func testAContributionRecordedDuringTheWritesIsWrittenWhenTheyEnd() throws {
        let (_, child) = try controllers()
        let ledger = ContributionsIOS.of(child)
        // The callback Exact's write raises records a second contribution and applies it, which
        // the writes under way owe a pass
        var once = false
        child.during = {
            guard !once else { return }
            once = true
            ledger.set(self.id(2), self.top(10), generation: 1)
            ledger.apply()
        }
        ledger.set(id(1), top(20), generation: 1)
        ledger.apply()
        XCTAssertEqual(ledger.entries.map(\.effect), [top(20), top(10)])
        XCTAssertEqual(child.additionalSafeAreaInsets, top(30))
    }

    func testAnAppThatAnswersEveryWriteLeavesTheLedgerUnsettledAndStill() throws {
        let (_, child) = try controllers()
        let ledger = ContributionsIOS.of(child)
        // The app puts back its 30 in every callback, and a held reaction applies the ledger, as
        // a sample does: the measurement ends, unsettled, and writes nothing more until an input
        // changes
        child.during = {
            if child.additionalSafeAreaInsets != self.top(30) { child.additionalSafeAreaInsets = self.top(30) }
        }
        child.react = { ledger.apply() }
        ledger.set(id(1), top(20), generation: 1)
        ledger.apply()
        XCTAssertTrue(ledger.unsettled)
        let calls = child.calls
        ledger.apply()
        XCTAssertEqual(child.calls, calls)
    }

    func testAReactionThatChangesAndSamplesTheLedgerReadsItApplied() throws {
        let (_, child) = try controllers()
        let ledger = ContributionsIOS.of(child)
        // Plus 20, then minus 30; the held reaction takes the 20 away and samples: the sample
        // carries the −30 alone as UIKit now gives it
        var samples: [OwnerUpdateIOS] = []
        var once = false
        child.react = {
            guard !once else { return }
            once = true
            ledger.set(self.id(1), nil, generation: 2)
            samples.append(OwnerUpdateIOS.sample(controller: child, view: child.view, session: nil))
        }
        ledger.set(id(1), top(20), generation: 1)
        ledger.set(id(2), top(-30), generation: 1)
        ledger.apply()
        XCTAssertEqual(child.additionalSafeAreaInsets, top(-30))
        XCTAssertEqual(samples.map { $0.contributions.map(\.effect) }, [[direct(child, without: top(-30))]])
    }

    func testANewerGenerationOfTheSameWriteKeepsTheMeasurement() throws {
        let (_, child) = try controllers()
        let ledger = ContributionsIOS.of(child)
        ledger.set(id(1), top(20), generation: 1)
        ledger.apply()
        let calls = child.calls
        // The same insets at generation 2 write nothing; other insets at generation 3 do
        XCTAssertTrue(ledger.set(id(1), top(20), generation: 2))
        ledger.apply()
        XCTAssertEqual(child.calls, calls)
        ledger.set(id(1), top(30), generation: 3)
        ledger.apply()
        XCTAssertGreaterThan(child.calls, calls)
        XCTAssertEqual(ledger.entries.map(\.effect), [top(30)])
    }

    func testAContributionReplacedByOneOfEqualInsetsIsMeasured() throws {
        let (_, child) = try controllers()
        let ledger = ContributionsIOS.of(child)
        ledger.set(id(1), top(20), generation: 1)
        ledger.apply()
        // Another node's 20 in its place: the same total, measured for the new entry
        ledger.set(id(1), nil, generation: 2)
        ledger.set(id(2), top(20), generation: 1)
        ledger.apply()
        XCTAssertEqual(ledger.entries.map(\.identity), [id(2)])
        XCTAssertEqual(ledger.entries.map(\.effect), [top(20)])
    }

    func testOneNodesTwoOperationsStayApart() throws {
        let (_, child) = try controllers()
        let ledger = ContributionsIOS.of(child)
        let bar = ContributionsIOS.Identity(node: 1, operation: .barReservation)
        ledger.set(id(1), top(20), generation: 1)
        ledger.set(bar, top(10), generation: 1)
        ledger.apply()
        XCTAssertEqual(ledger.entries.map(\.effect), [top(20), top(10)])
        ledger.set(bar, nil, generation: 2)
        ledger.apply()
        XCTAssertEqual(ledger.entries.map(\.identity), [id(1)])
        XCTAssertEqual(child.additionalSafeAreaInsets, top(20))
    }

    func testABaselineChangeUnderAFlooredWriteIsMeasuredAgain() throws {
        let (root, child) = try controllers()
        let inherited = root.view.safeAreaInsets.top
        let ledger = ContributionsIOS.of(child)
        // Minus 50 on a child reads what it inherits before and after the app adds 20, so only the
        // app's part tells the effect moved
        ledger.set(id(1), top(-50), generation: 1)
        ledger.apply()
        XCTAssertEqual(ledger.entries.map(\.effect), [top(0)])
        child.additionalSafeAreaInsets.top += 20
        XCTAssertEqual(child.view.safeAreaInsets.top, inherited)
        ledger.apply()
        XCTAssertEqual(child.view.safeAreaInsets.top, inherited)
        XCTAssertEqual(ledger.entries.map(\.effect), [top(-20)])
    }

    func testABarShownUnderAFlooredWriteIsMeasuredAgain() throws {
        // A navigation controller's child under Exact's −60 on top, its bar hidden, then shown
        let scene = hostedWindowScene()
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        window.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        let child = Child()
        let navigation = UINavigationController(rootViewController: child)
        navigation.setNavigationBarHidden(true, animated: false)
        window.rootViewController = navigation
        window.makeKeyAndVisible()
        self.window = window
        window.layoutIfNeeded()
        let ledger = ContributionsIOS.of(child)
        ledger.set(id(1), top(-60), generation: 1)
        ledger.apply()
        XCTAssertEqual(ledger.entries.map(\.effect), [direct(child, without: top(-60))])
        let hidden = ledger.entries.map(\.effect)
        // The bar shows in the same turn as Exact's writes: the effect moves, with no other input
        navigation.setNavigationBarHidden(false, animated: false)
        window.layoutIfNeeded()
        ledger.apply()
        XCTAssertEqual(ledger.entries.map(\.effect), [direct(child, without: top(-60))])
        XCTAssertNotEqual(ledger.entries.map(\.effect), hidden)
    }

    @available(iOS 18, *)
    func testATabBarShownUnderAFlooredWriteIsMeasuredAgain() throws {
        // A tab bar controller's child under Exact's −60 on the bottom, its tab bar hidden with
        // `setTabBarHidden`, then shown
        let scene = hostedWindowScene()
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        window.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        let child = Child(), tabs = UITabBarController()
        tabs.viewControllers = [child]
        tabs.setTabBarHidden(true, animated: false)
        window.rootViewController = tabs
        window.makeKeyAndVisible()
        self.window = window
        window.layoutIfNeeded()
        let ledger = ContributionsIOS.of(child), bottom = UIEdgeInsets(top: 0, left: 0, bottom: -60, right: 0)
        ledger.set(id(1), bottom, generation: 1)
        ledger.apply()
        let hidden = ledger.entries.map(\.effect)
        XCTAssertEqual(hidden, [direct(child, without: bottom)])
        tabs.setTabBarHidden(false, animated: false)
        window.layoutIfNeeded()
        ledger.apply()
        XCTAssertEqual(ledger.entries.map(\.effect), [direct(child, without: bottom)])
        XCTAssertNotEqual(ledger.entries.map(\.effect), hidden)
    }

    func testAnOwnersUpdateCarriesItsContributionsAfterItsBand() throws {
        let (root, child) = try controllers()
        let ledger = ContributionsIOS.of(child)
        ledger.set(ContributionsIOS.Identity(node: 7, operation: .barReservation), top(20), generation: 1)
        let update = OwnerUpdateIOS.sample(controller: child, view: child.view, session: root.view)
        XCTAssertEqual(update.contributions.map(\.effect), [top(20)])
        XCTAssertEqual(update.band, child.view.safeAreaInsets)
        // The encoding as the kernel reads it: the contribution after the band, node, operation, effect
        let bytes = [UInt8](update.encode())
        XCTAssertEqual(Array(bytes[17..<21]), [1, 0, 0, 0])
        XCTAssertEqual(Array(bytes[21..<26]), [7, 0, 0, 0, ContributionsIOS.Operation.barReservation.rawValue])
        XCTAssertEqual(Array(bytes[26..<30]), Array(withUnsafeBytes(of: Float(20).bitPattern.littleEndian, Array.init)))
    }

    /// The effect of `child`'s last contribution measured directly: its insets with every write
    /// less with that one taken away.
    private func direct(_ child: UIViewController, without last: UIEdgeInsets) -> UIEdgeInsets {
        let full = child.additionalSafeAreaInsets, with = child.view.safeAreaInsets
        child.additionalSafeAreaInsets = UIEdgeInsets(top: full.top - last.top, left: full.left - last.left,
                                                      bottom: full.bottom - last.bottom, right: full.right - last.right)
        let without = child.view.safeAreaInsets
        child.additionalSafeAreaInsets = full
        return UIEdgeInsets(top: with.top - without.top, left: with.left - without.left,
                            bottom: with.bottom - without.bottom, right: with.right - without.right)
    }

    private func id(_ node: UInt32) -> ContributionsIOS.Identity {
        ContributionsIOS.Identity(node: node, operation: .additionalInsets)
    }

    private func top(_ value: CGFloat) -> UIEdgeInsets {
        UIEdgeInsets(top: value, left: 0, bottom: 0, right: 0)
    }

    /// A window root filling a window of the test host's scene, or of no scene where none has
    /// connected, and a child filling the root.
    private func controllers() throws -> (UIViewController, Child) {
        let scene = hostedWindowScene()
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        window.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        let root = UIViewController(), child = Child()
        window.rootViewController = root
        window.makeKeyAndVisible()
        root.addChild(child)
        child.view.frame = root.view.bounds
        root.view.addSubview(child.view)
        child.didMove(toParent: root)
        root.view.layoutIfNeeded()
        self.window = window
        return (root, child)
    }

    /// A child counting UIKit's safe-area calls, recording what its held reaction reads, and running
    /// what a test gives it there and at its layout.
    private final class Child: UIViewController {
        var calls = 0
        var reactions: [UIEdgeInsets] = []
        var react: (() -> Void)?
        var layout: (() -> Void)?
        /// Run inside UIKit's callback, at each step, as an app's own code would.
        var during: (() -> Void)?
        override func viewSafeAreaInsetsDidChange() {
            super.viewSafeAreaInsetsDidChange()
            calls += 1
            during?()
            SafeAreaSteps.react(self) { [weak self] in
                guard let self else { return }
                reactions.append(view.safeAreaInsets)
                react?()
            }
        }
        override func viewDidLayoutSubviews() {
            super.viewDidLayoutSubviews()
            layout?()
        }
    }
}
#endif
