#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactGroupedLists

final class GroupedPaddingIOSTests: GroupedListFixture {
    /// A list's view reaches past its laid-out box to a side safe area, where UIKit keeps its cards clear of that area.
    /// The kernel is told the width UIKit's layout gives the cards there: no part of it comes from the box or the reach,
    /// so the strip beside an open iPhone Duo's inspector is taken off the rows once, by UIKit.
    func testTheToldWidthIsUIKitsCellWidthWhereTheViewReachesPastItsBox() throws {
        let p = presenter { self.model("inset-grouped", custom: true) }
        let l = try list(p), collection = l.collection
        let before = try XCTUnwrap(l.cells())
        SafeReach.apply(UIEdgeInsets(top: 0, left: 0, bottom: 0, right: 84), to: l.owner)
        collection.frame = l.owner.scrollFrame
        (collection as? GroupedCollectionView)?.sideSafeArea = (0, 84)
        let after = try XCTUnwrap(l.cells())
        let card = try XCTUnwrap(collection.layoutAttributesForItem(at: IndexPath(item: 0, section: 0))).frame
        XCTAssertEqual(collection.bounds.width, l.owner.frame.width, "the view reached")
        XCTAssertEqual(after[0], card.width, accuracy: 0.5, "the width told is the cell's, whatever the reach")
        XCTAssertEqual(Array(after[1...]), Array(before[1...]), "the cells' margins")
    }

    /// A box and the list inside it both reach a side safe edge, as the Info route and its list in the iPhone Duo's
    /// inspector column reach the fold's band at a partial fold, and both stop reaching when the edge goes. UIKit infers
    /// the list's margins, from which it insets its cards, from its superview's bounds when the list's frame changes:
    /// the outer box must stop reaching first, or the list is measured against the box's old origin and its cards
    /// keep no inset until it is built again.
    func testAListThatStopsReachingWithTheBoxAroundItKeepsItsCardInset() throws {
        // Each box is a fill, as the Info route's ground is: a fill reaches the edges it touches.
        let ground: [String: Any] = ["background_color": [242, 242, 247, 255]]
        let p = presenter(style: ground) { self.model("inset-grouped") }
        let l = try list(p), collection = l.collection
        let holder = UIViewController()
        window.rootViewController = holder
        holder.view.frame = window.bounds
        holder.view.addSubview(p.viewport)
        holder.view.layoutIfNeeded()
        p.apply(wireBatch([
            ["op": "create", "id": 50, "kind": "view", "props": [:], "handlers": [], "style": ground],
            ["op": "children", "id": 50, "ids": [1]], ["op": "roots", "ids": [50]],
            ["op": "frame", "id": 50, "x": 0.0, "y": 0.0, "w": 402.0, "h": 874.0],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 402.0, "h": 874.0],
        ]))
        let box = try XCTUnwrap(p.views[50])
        /// The first card's inset from the list's box, in the window.
        func inset() throws -> CGFloat {
            collection.layoutIfNeeded()
            let card = try XCTUnwrap(collection.layoutAttributesForItem(at: IndexPath(item: 0, section: 0))).frame
            return collection.convert(card, to: window).minX - l.owner.convert(l.owner.boxBounds, to: window).minX
        }
        // The safe edge is UIKit's too, as the fold's band is: the controller's safe area reaches that far.
        func reach(safeLeft: CGFloat) {
            holder.additionalSafeAreaInsets.left = safeLeft
            holder.view.layoutIfNeeded()
            let a = p.viewport.bounds
            let walk = SafeReach(view: p.viewport, allocation: a, safe: a.inset(by: UIEdgeInsets(top: 0, left: safeLeft, bottom: 0, right: 0)), pixel: 1 / 3) { _ in }
            _ = walk.visit(box, allowed: .all)
            walk.applyPlan()
            p.groupedLists?.sync(changed: [])
        }
        let before = try inset()
        XCTAssertGreaterThan(before, 0, "UIKit insets an inset-grouped list's cards")
        // A safe edge 40 points in: the box and the list are laid out at it, and both reach the allocation's edge.
        p.apply(wireBatch([["op": "frame", "id": 50, "x": 40.0, "y": 0.0, "w": 362.0, "h": 874.0],
                           ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 362.0, "h": 874.0]]))
        reach(safeLeft: 40)
        XCTAssertEqual(box.reach.left, 40, "the box reaches the edge")
        XCTAssertEqual(l.owner.reach.left, 40, "the list reaches the edge")
        XCTAssertEqual(try inset(), before, accuracy: 0.5, "UIKit keeps the cards clear of the safe area by as much")
        // The edge goes: the boxes take the whole width again and stop reaching.
        p.apply(wireBatch([["op": "frame", "id": 50, "x": 0.0, "y": 0.0, "w": 402.0, "h": 874.0],
                           ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 402.0, "h": 874.0]]))
        reach(safeLeft: 0)
        XCTAssertEqual(box.reach, .zero)
        XCTAssertEqual(l.owner.reach, .zero)
        XCTAssertEqual(try inset(), before, accuracy: 0.5, "the cards' inset is UIKit's again")
    }

    func testHorizontalPaddingInsetsSectionsAndIsNotReportedAsACellMargin() throws {
        for appearance in ["plain", "grouped", "inset-grouped"] {
            for direction in ["ltr", "rtl"] {
                let p = presenter(style: ["direction": direction]) { self.model(appearance, custom: true) }
                // Without a scene the list layout's environment stays left to
                // right whatever the list's direction.
                guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
                    throw missing("scene", "a right-to-left list lays out right to left only in a scene's window")
                }
                window.isHidden = true
                window = UIWindow(windowScene: scene)
                window.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
                window.addSubview(p.viewport)
                window.makeKeyAndVisible()
                defer { window.isHidden = true }
                let l = try list(p), collection = l.collection
                p.apply(wireBatch([contentOp(1, 402, 874, padding: [12, 0, 23, 0])]))
                func frames() throws -> [CGRect] {
                    collection.layoutIfNeeded()
                    return try [IndexPath(item: 0, section: 0), IndexPath(item: 1, section: 1)].map {
                        try XCTUnwrap(collection.layoutAttributesForItem(at: $0)).frame
                    } + [UICollectionView.elementKindSectionHeader, UICollectionView.elementKindSectionFooter].map {
                        try XCTUnwrap(collection.layoutAttributesForSupplementaryElement(ofKind: $0, at: IndexPath(item: 0, section: 0))).frame
                    }
                }
                let original = try frames(), cells = try XCTUnwrap(l.cells())
                let port = collection.frame
                // Other owners can add room for refresh and keyboard avoidance.
                collection.contentInset.top += 31
                collection.contentInset.bottom += 47
                let insets = collection.contentInset
                for (left, right) in [(230.0 / 3, 0.0), (31.0, 17.0), (0.0, 0.0)] {
                    // A resolved padding can change without a style or frame change.
                    p.apply(wireBatch([contentOp(1, 402, 874, padding: [12, right, 23, left])]))
                    let note = "\(appearance), \(direction), padding \(left)/\(right)"
                    let padded = try frames()
                    for (before, after) in zip(original, padded) {
                        XCTAssertEqual(after.minX, before.minX + left, accuracy: 0.5, note)
                        XCTAssertEqual(after.maxX, before.maxX - right, accuracy: 0.5, note)
                    }
                    let reported = try XCTUnwrap(l.cells())
                    XCTAssertEqual(reported[0], cells[0] - left - right, accuracy: 0.5, "UIKit's cells take the padding off: \(note)")
                    XCTAssertEqual(Array(reported[1...]), Array(cells[1...]), "CSS padding is not a cell's margin: \(note)")
                    XCTAssertEqual(collection.frame, port, "padding leaves the scroll port intact")
                    XCTAssertEqual(collection.contentInset, insets, "horizontal padding preserves the other inset owners")
                    p.apply(wireBatch([]))
                    XCTAssertEqual(try frames(), padded, "a remount keeps the section geometry")
                }
            }
        }
    }

    /// A plain list's header is measured at the width it is drawn at, its padding taken off.
    func testAPlainHeaderWrappedByThePaddingIsAsTallAsItsText() throws {
        let heading = "Announcements, events and plans for the season ahead"
        let p = presenter { GroupedListModel(style: "plain", sections: [.init(view: 2, header: heading, footer: nil, rows: [Row(view: 10, title: "Profile", pressable: true)])]) }
        defer { window.isHidden = true }
        let collection = try list(p).collection, path = IndexPath(item: 0, section: 0)
        func header() throws -> (frame: CGRect, needs: CGFloat) {
            collection.layoutIfNeeded()
            let view = try XCTUnwrap(collection.supplementaryView(forElementKind: UICollectionView.elementKindSectionHeader, at: path))
            let needs = view.systemLayoutSizeFitting(CGSize(width: view.bounds.width, height: 0), withHorizontalFittingPriority: .required,
                                                     verticalFittingPriority: .fittingSizeLevel).height
            func labels(_ v: UIView) -> [UILabel] { (v as? UILabel).map { [$0] } ?? v.subviews.flatMap(labels) }
            let label = labels(view).first
            XCTAssertGreaterThan(label?.bounds.width ?? 0, view.bounds.width / 2, "the heading's text takes the header's width")
            return (view.frame, needs)
        }
        p.apply(wireBatch([contentOp(1, 402, 874, padding: [0, 0, 0, 0])]))
        let full = try header()
        p.apply(wireBatch([contentOp(1, 402, 874, padding: [0, 0, 0, 200])]))
        let padded = try header()
        XCTAssertEqual(padded.frame.minX, 200, accuracy: 0.5)
        XCTAssertEqual(padded.frame.minY, full.frame.minY, accuracy: 0.5, "the padding moves the heading sideways only")
        XCTAssertGreaterThan(padded.frame.height, full.frame.height + 1, "the heading wraps to more lines in the padded width")
        XCTAssertGreaterThanOrEqual(padded.frame.height + 0.5, padded.needs, "and its frame holds them")
        let row = try XCTUnwrap(collection.layoutAttributesForItem(at: path)).frame
        XCTAssertGreaterThanOrEqual(row.minY + 0.5, padded.frame.maxY, "the row starts under the heading")
    }
}

/// A grouped list's custom row starts where a standard row's text does: at
/// its cell's layout margin, which UIKit widens from 16 to 20 points on a
/// wide screen (the iPhone Duo's inner panel, an iPhone in landscape); on
/// the Duo's simulator it is 20 at both widths used here. The host tells
/// the kernel the margins and width UIKit chose, again when they change, and
/// the kernel lays the custom row out by them.
final class GroupedMarginsIOSTests: XCTestCase {
    func testACustomRowStartsAtItsCellsMarginAsTheWidthChanges() throws {
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_GROUPED_REVEAL_PLAN"])
        ExactGroupedLists.install()
        let session = ExactApp.shared.makeSession(label: "grouped-margins")
        defer { session.destroy() }
        let view = ExactView(session: session)
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "the list's margins need a hosted window scene")
        }
        // As wide as the Duo's inner panel, where UIKit's list margins are 20.
        let window = UIWindow(windowScene: scene)
        window.frame = CGRect(x: 0, y: 0, width: 951, height: 669)
        let root = UIViewController()
        window.rootViewController = root
        window.makeKeyAndVisible()
        root.view.addSubview(view)
        view.frame = root.view.bounds
        defer { window.isHidden = true }
        XCTAssertNil(session.boot(plan: try Data(contentsOf: URL(fileURLWithPath: path)), size: view.bounds.size).error)
        let p = session.presenter, agent = Agent(session: session)
        func settle() {
            window.layoutIfNeeded()
            let reply = agent.clock(["settle": true])
            XCTAssertEqual(reply["settled"] as? Bool, true, "\(reply)")
        }
        func tap(_ name: String) throws {
            let node = try XCTUnwrap(p.views.values.first { $0.props["testId"] == name }, name)
            XCTAssertNil(agent.reveal(["id": Int(node.id)])["error"])
            XCTAssertNil(agent.tap(["id": Int(node.id)])["error"])
            settle()
        }
        settle()
        // The fixture's custom row, a row holding a button, comes with the
        // section its sheet adds.
        try tap("add")
        try tap("save")
        XCTAssertNil(agent.clock(["data": true])["error"])
        settle()
        let host = try XCTUnwrap(p.groupedLists as? GroupedListHost)
        let button = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "remove" })
        XCTAssertNil(agent.reveal(["id": Int(button.id)])["error"])
        settle()
        let list = try XCTUnwrap(host.lists.values.first(where: \.carriesCustomRows))
        // Where UIKit's own cell configurations start their text (`UIListContentConfiguration`), and the custom row's content.
        func starts() throws -> (margin: CGFloat, row: CGFloat) {
            let cell = try XCTUnwrap(list.collection.visibleCells.first { button.isDescendant(of: $0) } as? UICollectionViewListCell,
                                     "the custom row is carried into its cell")
            return (cell.directionalLayoutMargins.leading, button.convert(button.bounds, to: cell).minX)
        }
        let wide = try starts()
        XCTAssertEqual(wide.margin, 20, "UIKit's cell margin on a screen this wide; the test needs one other than the sheet's 16")
        XCTAssertEqual(host.told[list.owner.id]?[1], wide.margin, "the kernel was told UIKit's margin")
        let cellWidth = try XCTUnwrap(list.collection.visibleCells.first { button.isDescendant(of: $0) }).frame.width
        XCTAssertEqual(host.told[list.owner.id]?[0], cellWidth, "and its cell's width")
        XCTAssertEqual(wide.row, wide.margin, accuracy: 0.5, "the custom row's content starts at the margin")
        // An iPhone's width, as a fold or a turn changes it, while a
        // collection's slice is in flight: the change lands with the slice,
        // first in line behind it, and a report owed before it (the kernel
        // told nothing yet) runs after. Sampled when it runs, the report
        // tells the margin that shows, not the one when it was owed.
        p.collections.onFill?(list.owner.id, Data())
        XCTAssertTrue(session.fillInFlight)
        session.whenIdle {
            window.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
            view.frame = root.view.bounds
            view.layoutIfNeeded()
        }
        host.told[list.owner.id] = nil
        host.sync(changed: [])
        let deadline = Date().addingTimeInterval(5)
        while session.fillInFlight, Date() < deadline { RunLoop.main.run(until: Date().addingTimeInterval(0.02)) }
        XCTAssertFalse(session.fillInFlight, "the slice lands")
        settle()
        XCTAssertNil(agent.reveal(["id": Int(button.id)])["error"])
        settle()
        let narrow = try starts()
        XCTAssertEqual(host.told[list.owner.id]?[1], narrow.margin)
        XCTAssertEqual(narrow.row, narrow.margin, accuracy: 0.5, "the custom row follows the margin that shows")
        guard narrow.margin != wide.margin else {
            throw XCTSkip("UIKit's list margin is \(narrow.margin) at both widths on this device (the iPhone Duo's), so a report sampled when it was owed would pass too")
        }
        XCTAssertEqual(narrow.margin, 16, "UIKit's cell margin at an iPhone's width")
    }
}
#endif
