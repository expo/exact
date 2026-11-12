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

    private func booted() throws -> ExactSession {
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_ARRANGEMENT_SIZING_PLAN"])
        let session = ExactApp.shared.makeSession(label: "arrangement-sizing")
        self.session = session
        let host = UIViewController(), view = ExactView(session: session)
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 600, height: 800))
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
    private func fixture() throws -> (ExactSession, UIArrangementViewController, UIView) {
        let session = try booted()
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

    private func fitted(_ pane: UIView, width: CGFloat) -> CGSize {
        pane.systemLayoutSizeFitting(CGSize(width: width, height: 800),
                                    withHorizontalFittingPriority: .required, verticalFittingPriority: .fittingSizeLevel)
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
        for (dimension, height) in [(UISplitArrangement.Dimension.absolute(140), CGFloat(140)), (.fractional(0.25), 200)] {
            controller.updateArrangement(split(dimension))
            waitFor("the native dimension sets the allocation", cap: 5) { layout(); return abs(pane.bounds.height - height) < 1 }
        }
        controller.updateArrangement(split(.automatic))
        layout()
        XCTAssertGreaterThan(pane.bounds.height, 0)
        controller.updateArrangement(split(.intrinsic))
        waitFor("intrinsic sizing resumes", cap: 5) { layout(); return abs(pane.bounds.height - expected) < 1 }
        waitForMainQueue()
        let batches = session.appliedBatches
        for _ in 0..<5 { layout(); waitForMainQueue() }
        XCTAssertEqual(session.appliedBatches, batches, "allocation and measurement settle without feedback")
    }
}
#endif
