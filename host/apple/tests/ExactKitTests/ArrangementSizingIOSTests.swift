#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

final class ArrangementSizingIOSTests: XCTestCase {
    private var session: ExactSession?
    private var window: UIWindow?

    override func tearDown() {
        session?.destroy(); session = nil
        window?.isHidden = true; window = nil
        super.tearDown()
    }

    private func booted(_ size: CGSize = CGSize(width: 600, height: 800)) throws -> ExactSession {
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_ARRANGEMENT_SIZING_PLAN"])
        let session = ExactApp.shared.makeSession(label: "arrangement-sizing")
        self.session = session
        let host = UIViewController(), view = ExactView(session: session)
        let window = UIWindow(frame: CGRect(origin: .zero, size: size))
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        self.window = window
        XCTAssertNil(session.boot(plan: try Data(contentsOf: URL(fileURLWithPath: path)), size: window.bounds.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        return session
    }

    @available(iOS 27.1, *)
    private func fixture(_ size: CGSize = CGSize(width: 600, height: 800)) throws -> (ExactSession, UIArrangementViewController, UIView) {
        let session = try booted(size)
        let controller = UIArrangementViewController()
        controller.updateArrangement(split(.intrinsic))
        XCTAssertTrue(session.presenter.arrangements.set(try node("arrangement").id, controller: controller))
        let pane = try XCTUnwrap(controller.viewController(for: .primary)?.view)
        return (session, controller, pane)
    }

    private func node(_ name: String) throws -> NodeView {
        try XCTUnwrap(session?.presenter.views.values.first { $0.props["testId"] == name }, name)
    }

    @available(iOS 27.1, *)
    private func split(_ dimension: UISplitArrangement.Dimension) -> UISplitArrangement {
        var split = UISplitArrangement().axes(.vertical)
        var properties = split.defaultViewProperties
        properties.height.minimum = .absolute(0)
        properties.height.preferred = dimension
        properties.height.maximum = .fractional(1)
        split.setViewProperties(properties, for: .primary)
        return split
    }

    /// Panes side by side, the primary's width `dimension`.
    @available(iOS 27.1, *)
    private func across(_ dimension: UISplitArrangement.Dimension) -> UISplitArrangement {
        var split = UISplitArrangement().axes(.horizontal)
        var properties = split.defaultViewProperties
        properties.width.minimum = .absolute(0)
        properties.width.preferred = dimension
        properties.width.maximum = .fractional(1)
        split.setViewProperties(properties, for: .primary)
        return split
    }

    private func fitted(_ pane: UIView, width: CGFloat) -> CGSize {
        pane.systemLayoutSizeFitting(CGSize(width: width, height: 800),
                                    withHorizontalFittingPriority: .required, verticalFittingPriority: .fittingSizeLevel)
    }

    func testWithoutNavigationAPaneTakenOutIsCoveredWholeUntilItIsPutBack() throws {
        guard #available(iOS 27.1, *) else { throw XCTSkip("arrangements require iOS 27.1") }
        let make = NavigationLink.make, makeModals = NavigationLink.makeModals
        NavigationLink.make = nil; NavigationLink.makeModals = nil
        defer { NavigationLink.make = make; NavigationLink.makeModals = makeModals }
        try takeOutAndPutBack(navigation: false)
    }

    func testAPaneTakenOutStaysCoveredWholeThroughANavigationReset() throws {
        guard #available(iOS 27.1, *) else { throw XCTSkip("arrangements require iOS 27.1") }
        try takeOutAndPutBack(navigation: true)
    }

    @available(iOS 27.1, *)
    private func takeOutAndPutBack(navigation linked: Bool) throws {
        let (session, controller, _) = try fixture()
        let p = session.presenter
        XCTAssertEqual(p.navigation is NoNavigation, !linked)
        XCTAssertEqual(p.modals is NoModals, !linked)
        let holder = try XCTUnwrap(controller.viewController(for: .secondary) as? ArrangementPaneController), id = holder.node.id
        waitForMainQueue("the arrangement's setup pass")
        var told: [HostCover?] = [], tell = p.onCovers
        p.onCovers = { changes in told += changes.filter { $0.0 == id }.map(\.1); tell?(changes) }
        controller.setViewController(nil, for: .secondary)
        waitFor("the pane taken out is told covered whole") { told.last == .whole && holder.node.bounds.isEmpty }
        if linked {
            // A batch on a root without `navigationBack` resets the navigation host, which tells nothing of the pane
            p.apply(facesBatch())
            waitForMainQueue("the batch's work")
            XCTAssertEqual(told, [.whole])
        }
        controller.setViewController(holder, for: .secondary)
        waitFor("the pane put back is told uncovered and laid out") { told.last == .some(nil) && !holder.node.bounds.isEmpty }
        waitForMainQueue("a pass still queued")
        XCTAssertEqual(told, [.whole, nil], "each change is told once, and a pane shown is never told covered")
    }

    /// A batch can draw a pane whose arrangement just went as a bare layer, as it draws any inert box: a cover its
    /// arrangement left it goes all the same.
    func testAPaneDrawnAsALayerIsUncovered() throws {
        guard #available(iOS 27.1, *) else { throw XCTSkip("arrangements require iOS 27.1") }
        let p = Presenter(), host = UIViewController()
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 600, height: 800))
        window.rootViewController = host
        window.makeKeyAndVisible()
        self.window = window
        p.viewport.frame = host.view.bounds
        host.view.addSubview(p.viewport)
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "props": ["hatch": "arrangement"]],
            ["op": "create", "id": 2, "kind": "view", "style": ["background_color": [255, 0, 0, 255]]],
            ["op": "create", "id": 3, "kind": "view", "style": ["background_color": [0, 0, 255, 255]]],
            ["op": "children", "id": 1, "ids": [2, 3]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 600.0, "h": 800.0],
            ["op": "frame", "id": 2, "x": 0.0, "y": 0.0, "w": 600.0, "h": 400.0],
            ["op": "frame", "id": 3, "x": 0.0, "y": 400.0, "w": 600.0, "h": 400.0],
        ]))
        let controller = UIArrangementViewController()
        XCTAssertTrue(p.arrangements.set(1, controller: controller))
        let holder = try XCTUnwrap(controller.viewController(for: .secondary) as? ArrangementPaneController), id = holder.node.id
        waitForMainQueue("the arrangement's setup pass")
        var told: [HostCover?] = []
        p.onCovers = { changes in told += changes.filter { $0.0 == id }.map(\.1) }
        controller.setViewController(nil, for: .secondary)
        waitFor("the pane taken out is told covered whole") { told == [.whole] }
        XCTAssertTrue(p.arrangements.set(1, controller: nil))
        p.apply(wireBatch([["op": "style", "id": Int(id), "style": ["background_color": [0, 255, 0, 255]]]]))
        XCTAssertTrue(p.flats.isFlat(id), "the pane is drawn as a layer before the next pass")
        waitFor("and is told uncovered") { told == [.whole, nil] }
    }

    func testProposalsMeasureAuthoredTextWithoutChangingItsAllocation() throws {
        let session = try booted()
        let primary = try node("primary")
        session.apply(session.runtime.blocks([(primary.id, .size(CGSize(width: 600, height: 300)))]))
        let holder = ArrangementPaneController(node: primary, room: { CGSize(width: 600, height: 800) }, changed: {})
        let pane = try XCTUnwrap(holder.view)
        let frame = primary.frame, batches = session.appliedBatches
        XCTAssertEqual(fitted(pane, width: 240), CGSize(width: 240, height: 60))
        XCTAssertEqual(pane.sizeThatFits(CGSize(width: 240, height: 800)).height, 60)
        XCTAssertEqual(pane.systemLayoutSizeFitting(CGSize(width: 240, height: 0),
                       withHorizontalFittingPriority: .required, verticalFittingPriority: .fittingSizeLevel),
                       CGSize(width: 240, height: 60), "a fitting height is content sized even under a zero proposal")
        XCTAssertEqual(primary.frame, frame, "trial measurement preserves committed geometry")
        XCTAssertEqual(session.appliedBatches, batches, "measurement sends no allocation back")
        session.presenter.press(try node("change-text").id)
        let wide = fitted(pane, width: 240), narrow = fitted(pane, width: 120)
        XCTAssertGreaterThan(wide.height, 60)
        XCTAssertGreaterThan(narrow.height, wide.height, "the proposal determines text wrapping")
        let before = primary.frame
        for _ in 0..<10 { XCTAssertEqual(fitted(pane, width: 120), narrow) }
        XCTAssertEqual(primary.frame, before)
    }

    func testContentChangesInvalidateTheHoldersIntrinsicSizeAndAllocationDoesNot() throws {
        let session = try booted(), primary = try node("primary")
        let holder = ArrangementPaneController(node: primary, room: { CGSize(width: 600, height: 800) }, changed: {})
        let pane = try XCTUnwrap(holder.view)
        XCTAssertEqual(pane.intrinsicContentSize.width, UIView.noIntrinsicMetric, "the initial block has not been installed")
        session.apply(session.runtime.blocks([(primary.id, .size(.zero))]))
        holder.contentChanged()
        XCTAssertEqual(pane.intrinsicContentSize.height, 60, "a refused measurement is retried after allocation")
        let stack = UIStackView(arrangedSubviews: [pane, UIView()])
        stack.axis = .vertical
        stack.frame = CGRect(x: 0, y: 0, width: 600, height: 800)
        window?.rootViewController?.view.addSubview(stack)
        stack.layoutIfNeeded()
        XCTAssertEqual(pane.bounds.height, 60)
        session.presenter.press(try node("add-row").id)
        holder.contentChanged()
        stack.layoutIfNeeded()
        XCTAssertEqual(pane.bounds.height, 80)
        session.presenter.press(try node("change-text").id)
        holder.contentChanged()
        stack.layoutIfNeeded()
        XCTAssertGreaterThan(pane.bounds.height, 80)
        let expected = pane.bounds.size
        let before = session.appliedBatches
        session.apply(session.runtime.blocks([(primary.id, .size(expected))]))
        holder.contentChanged()
        stack.layoutIfNeeded()
        XCTAssertEqual(pane.bounds.size, expected, "the allocation does not feed back into the content measure")
        XCTAssertEqual(session.appliedBatches, before + 1, "measurement emits no further batch")
    }

    func testPaneMarginsAreIncludedAndMarginChangesInvalidateItsSize() throws {
        let session = try booted(), primary = try node("primary")
        session.presenter.press(try node("change-margin").id)
        session.apply(session.runtime.blocks([(primary.id, .size(CGSize(width: 600, height: 300)))]))
        let holder = ArrangementPaneController(node: primary, room: { CGSize(width: 600, height: 800) }, changed: {})
        let pane = try XCTUnwrap(holder.view)
        XCTAssertEqual(fitted(pane, width: 240), CGSize(width: 240, height: 80))
        XCTAssertEqual(pane.sizeThatFits(CGSize(width: 240, height: 800)).height, 80)
        XCTAssertEqual(pane.systemLayoutSizeFitting(CGSize(width: 240, height: 40),
                       withHorizontalFittingPriority: .required, verticalFittingPriority: .required),
                       CGSize(width: 240, height: 40), "required axes keep the proposed allocation")
        XCTAssertEqual(pane.intrinsicContentSize.height, 80)
        let stack = UIStackView(arrangedSubviews: [pane, UIView()])
        stack.axis = .vertical
        stack.frame = CGRect(x: 0, y: 0, width: 600, height: 800)
        window?.rootViewController?.view.addSubview(stack)
        stack.layoutIfNeeded()
        XCTAssertEqual(pane.bounds.height, 80)
        session.apply(session.runtime.blocks([(primary.id, .size(pane.bounds.size))]))
        XCTAssertEqual(primary.frame.minY, 10)
        XCTAssertEqual(primary.frame.maxY, 70, "the allocation holds the border box and both margins")
        session.presenter.press(try node("change-margin").id)
        holder.contentChanged()
        stack.layoutIfNeeded()
        XCTAssertEqual(pane.bounds.height, 100, "a margin-only change invalidates the intrinsic size")
        XCTAssertEqual(fitted(pane, width: 240), CGSize(width: 240, height: 100))
        let before = session.appliedBatches
        session.apply(session.runtime.blocks([(primary.id, .size(pane.bounds.size))]))
        holder.contentChanged()
        stack.layoutIfNeeded()
        XCTAssertEqual(primary.frame.minY, 20)
        XCTAssertEqual(primary.frame.maxY, 80)
        XCTAssertEqual(pane.bounds.height, 100)
        XCTAssertEqual(session.appliedBatches, before + 1, "allocation and measurement do not feed back")
    }

    func testAPanesReachOfTheBottomEdgeIsTheSameWhetherTheRoutePadsItOrNot() throws {
        let session = try booted(), primary = try node("primary")
        session.presenter.press(try node("cap").id)
        func told(_ height: CGFloat) -> HostBlock {
            let block = HostBlock.size(CGSize(width: 600, height: height))
            session.apply(session.runtime.blocks([(primary.id, block)]))
            return block
        }
        var held: BottomReach?
        func reaches(_ block: HostBlock, below slack: CGFloat) -> Bool {
            BottomReach.reaches(primary, end: primary.frame.maxY, told: block, slack: slack, held: &held)
        }
        // A route padding 25 points for a bar tells the pane 266, which it fills; unpadded it is told 291 and stops at its
        // 280-point maximum. Read from the padded layout, the pane would take the edge there and give it back unpadded.
        let padded = told(266)
        XCTAssertEqual(primary.frame.maxY, 266)
        let batches = session.appliedBatches
        XCTAssertFalse(reaches(padded, below: 25), "the pane is measured in the block it would be told unpadded")
        XCTAssertEqual(primary.frame.maxY, 266, "measuring leaves the committed layout")
        XCTAssertEqual(session.appliedBatches, batches, "measuring sends nothing")
        XCTAssertFalse(reaches(padded, below: 25), "a held answer stands while the layout does")
        XCTAssertFalse(reaches(told(291), below: 0))
        XCTAssertEqual(primary.frame.maxY, 280)
        // A pane that fills its unpadded block reaches the edge in either state
        XCTAssertTrue(reaches(told(230), below: 25))
        XCTAssertTrue(reaches(told(255), below: 0))
        XCTAssertFalse(reaches(told(230), below: 60), "nor in a block past its maximum")
    }

    /// A percentage height on a pane its arrangement sizes to its content is cyclic (LLP 1123 D7): the pane takes its
    /// content's height, not the room's; given an absolute allocation, the percentage has it as its basis, within the
    /// pane's maximum.
    func testAPercentageHeightIsCyclicUnderIntrinsicSizingAndHasTheAllocationOtherwise() throws {
        guard #available(iOS 27.1, *) else { throw XCTSkip("arrangements require iOS 27.1") }
        // `height: 100%`, at most 280, landed before the arrangement takes the pane
        let session = try booted(), primary = try node("primary")
        session.presenter.press(try node("cap").id)
        waitForActivation(session)
        waitFor("the cap lands") { !session.ownerInFlight && !session.hasPublished() }
        let controller = UIArrangementViewController()
        controller.updateArrangement(split(.intrinsic))
        XCTAssertTrue(session.presenter.arrangements.set(try node("arrangement").id, controller: controller))
        let pane = try XCTUnwrap(controller.viewController(for: .primary)?.view)
        func layout() { window?.layoutIfNeeded(); controller.view.layoutIfNeeded() }
        // UIKit settles the pane at its content's height after it attaches, not the room's, though a region gives it its maximum
        waitFor("the pane takes its content's 60 points", cap: 5, facts: { "pane \(pane.bounds.height), primary \(primary.frame.height)" }) {
            layout(); return abs(pane.bounds.height - 60) < 1 && abs(primary.frame.height - 60) < 1
        }
        XCTAssertEqual(session.runtime.measureBlock(primary.id, in: .region(CGSize(width: 600, height: 800)))?.used.height, 280, "the cap is in")
        for (allocation, height) in [(CGFloat(200), CGFloat(200)), (400, 280)] {
            controller.updateArrangement(split(.absolute(allocation)))
            waitFor("100% of \(allocation), at most 280", cap: 5, facts: { "pane \(pane.bounds.height), primary \(primary.frame.height)" }) {
                layout(); return abs(pane.bounds.height - allocation) < 1 && abs(primary.frame.height - height) < 1
            }
        }
    }

    /// Side by side, in a window wide enough for UIKit to put them so, the axis the split sizes is the width: a
    /// `width: 100%` pane's intrinsic and fitted widths are its content's, while its `height: 50%` keeps the room's height,
    /// or a fitting target's, as its basis; and an overlay that stacks its panes, which sizes no axis, gives the width its basis back.
    func testOnlyTheAxisTheSplitSizesTakesThePanesPercentagesAsCyclic() throws {
        guard #available(iOS 27.1, *) else { throw XCTSkip("arrangements require iOS 27.1") }
        let (session, controller, pane) = try fixture(CGSize(width: 1000, height: 600))
        let primary = try node("primary")
        controller.updateArrangement(across(.intrinsic))
        let primaryHolder = try XCTUnwrap(controller.viewController(for: .primary))
        let axis = { controller.placement(for: primaryHolder).flatMap { controller.state(for: $0)?.splitAxis } }
        waitFor("UIKit reports the split side by side", cap: 5, facts: { "splitAxis \(String(describing: axis()))" }) {
            self.window?.layoutIfNeeded(); controller.view.layoutIfNeeded(); return axis() == .horizontal
        }
        session.presenter.press(try node("across").id)
        waitFor("the percentages land", cap: 5) { session.runtime.measureBlock(primary.id, in: .region(CGSize(width: 600, height: 800)))?.used == CGSize(width: 600, height: 400) }
        controller.view.layoutIfNeeded()
        let intrinsic = pane.intrinsicContentSize
        let room = controller.view.bounds.size
        XCTAssertLessThan(intrinsic.width, room.width / 2, "the content's width, not the room's")
        XCTAssertEqual(intrinsic.height, room.height / 2, accuracy: 1, "half the room's height")
        let fitted = pane.systemLayoutSizeFitting(CGSize(width: 600, height: 300), withHorizontalFittingPriority: .fittingSizeLevel,
                                                  verticalFittingPriority: .fittingSizeLevel)
        XCTAssertEqual(fitted, CGSize(width: intrinsic.width, height: 150), "the width the content's, the height half the target's")
        // UIKit settles the pane's allocation at the content's width
        waitFor("the side-by-side pane settles at its content's width", cap: 5, facts: { "pane \(pane.bounds.width)" }) {
            self.window?.layoutIfNeeded(); controller.view.layoutIfNeeded(); return abs(pane.bounds.width - intrinsic.width) < 1
        }
        controller.updateArrangement(.overlay)
        controller.view.layoutIfNeeded()
        pane.invalidateIntrinsicContentSize()
        XCTAssertEqual(pane.intrinsicContentSize.width, room.width, accuracy: 1, "an overlay that stacks the panes sizes no axis: the room's")
    }

    func testIntrinsicAllocationFollowsContentAndOtherDimensionsStillWork() throws {
        guard #available(iOS 27.1, *) else { throw XCTSkip("arrangements require iOS 27.1") }
        let (session, controller, pane) = try fixture()
        func layout() { window?.layoutIfNeeded(); controller.view.layoutIfNeeded() }
        waitFor("the intrinsic pane takes its content height", cap: 5) { layout(); return abs(pane.bounds.height - 60) < 1 }
        session.presenter.press(try node("add-row").id)
        waitFor("new content resizes the intrinsic pane", cap: 5) { layout(); return abs(pane.bounds.height - 80) < 1 }
        session.presenter.press(try node("change-text").id)
        var expected = fitted(pane, width: pane.bounds.width).height
        XCTAssertGreaterThan(expected, 80)
        waitFor("changed text resizes the intrinsic pane", cap: 5) { layout(); return abs(pane.bounds.height - expected) < 1 }
        session.presenter.press(try node("change-margin").id)
        expected = fitted(pane, width: pane.bounds.width).height
        waitFor("changed margins resize the intrinsic pane", cap: 5) { layout(); return abs(pane.bounds.height - expected) < 1 }
        // A fraction is of the arrangement view's height. Each allocation is far from the last, so the dimension, not the
        // content, is what moves the pane.
        let arranged = controller.view.bounds.height
        for (dimension, height) in [(UISplitArrangement.Dimension.absolute(expected + 100), expected + 100), (.fractional(0.5), arranged / 2)] {
            XCTAssertGreaterThan(abs(pane.bounds.height - height), 20, "the dimension changes the allocation")
            controller.updateArrangement(split(dimension))
            waitFor("the native dimension sets the allocation", cap: 5, facts: { "pane \(pane.bounds.height), arranged in \(controller.view.bounds.height)" }) {
                layout(); return abs(pane.bounds.height - height) < 1
            }
        }
        controller.updateArrangement(split(.automatic))
        layout()
        XCTAssertGreaterThan(pane.bounds.height, 0)
        controller.updateArrangement(split(.intrinsic))
        waitFor("intrinsic sizing resumes", cap: 5) { layout(); return abs(pane.bounds.height - expected) < 1 }
        // Count from a quiet session: the data's activation and the runtime's answers to the resumed allocation land first
        waitForActivation(session)
        waitFor("the session's own work lands") { !session.ownerInFlight && !session.hasPublished() }
        let batches = session.appliedBatches
        for _ in 0..<5 { layout(); waitForMainQueue() }
        XCTAssertEqual(session.appliedBatches, batches, "allocation and measurement settle without feedback")
    }
}
#endif
