#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactGroupedLists

/// A section header that holds more than one text (an avatar beside a name)
/// keeps its views, carried into UIKit's header at the kernel's layout; a
/// header of one text stays UIKit's own. UIKit, so a simulator runs it.
final class GroupedCustomHeadersIOSTests: GroupedListFixture {
    let headerKind = UICollectionView.elementKindSectionHeader

    /// Two sections of five rows: the first's header custom when `custom`,
    /// holding a control when `parts`.
    func favorites(_ style: String, custom: Bool = true, parts: Bool = false) -> GroupedListModel {
        var first = GroupedListModel.Section(view: 2, header: "Ryu", footer: nil, rows: [10, 11, 12].map { Row(view: $0, title: "Move \($0)", pressable: true) })
        if custom { first.headerView = 40; first.headerParts = parts }
        return GroupedListModel(style: style, sections: [
            first,
            .init(view: 3, header: "Plain", footer: nil, rows: [20, 21].map { Row(view: $0, title: "Move \($0)", pressable: true) }),
        ])
    }

    /// The kernel's boxes for `header` → `row` → an avatar holding an
    /// image, then a text 8 points after it.
    func headerOps(height: Double = 44) -> [[String: Any]] {
        let view = { (id: Int, kind: String, props: [String: String]) -> [String: Any] in
            ["op": "create", "id": id, "kind": kind, "props": props, "style": ["text_color": [0, 0, 0, 255]]]
        }
        return [
            view(40, "view", ["semanticTag": "header", "testId": "header"]), view(44, "view", [:]),
            view(41, "view", ["accessibilityElementsHidden": "true"]),
            ["op": "create", "id": 43, "kind": "image", "props": ["imageSource": "symbol:person", "symbolName": "person.circle"], "style": ["font_size": 17.0]],
            view(42, "text", ["text": "Ryu"]),
            ["op": "children", "id": 2, "ids": [40, 4]], ["op": "children", "id": 40, "ids": [44]],
            ["op": "children", "id": 44, "ids": [41, 42]], ["op": "children", "id": 41, "ids": [43]],
            ["op": "frame", "id": 40, "x": 0.0, "y": 0.0, "w": 402.0, "h": height],
            ["op": "frame", "id": 44, "x": 16.0, "y": 10.0, "w": 370.0, "h": 24.0],
            ["op": "frame", "id": 41, "x": 0.0, "y": 0.0, "w": 24.0, "h": 24.0],
            ["op": "frame", "id": 43, "x": 0.0, "y": 0.0, "w": 24.0, "h": 24.0],
            ["op": "frame", "id": 42, "x": 32.0, "y": 2.0, "w": 100.0, "h": 20.0],
        ]
    }

    func header(_ p: Presenter, section: Int) throws -> GroupedHeaderView {
        let l = try list(p)
        l.collection.layoutIfNeeded()
        return try XCTUnwrap(l.collection.supplementaryView(forElementKind: headerKind, at: IndexPath(item: 0, section: section)) as? GroupedHeaderView,
                             "section \(section) shows a header")
    }

    func testAnImageAndATextAreDrawnInUIKitsHeaderAtTheKernelsFrames() throws {
        let p = presenter(height: 200, props: ["listStyle": "plain"]) { self.favorites("plain") }
        p.apply(wireBatch(headerOps()))
        let l = try list(p), view = try header(p, section: 0)
        let node = try XCTUnwrap(p.views[40]), avatar = try XCTUnwrap(p.views[41]), image = try XCTUnwrap(p.views[43]), name = try XCTUnwrap(p.views[42])
        XCTAssertTrue(node.superview === view.contentView, "the header's own views are carried into UIKit's header")
        XCTAssertNil(view.contentConfiguration, "UIKit draws no text of its own there")
        XCTAssertEqual(view.frame.height, 44, accuracy: 0.5, "as tall as the kernel laid the header out")
        XCTAssertEqual(avatar.convert(avatar.bounds, to: view), CGRect(x: 16, y: 10, width: 24, height: 24))
        XCTAssertEqual(image.convert(image.bounds, to: view), CGRect(x: 16, y: 10, width: 24, height: 24))
        XCTAssertEqual(name.convert(name.bounds, to: view), CGRect(x: 48, y: 12, width: 100, height: 20))
        XCTAssertFalse(node.isHidden)
        XCTAssertTrue(p.views[2]?.isHidden == true, "its section stays hidden under the list")
        XCTAssertTrue(host(p).projects(node))

        XCTAssertTrue(view.isAccessibilityElement, "one element, as UIKit's own header is")
        XCTAssertEqual(view.accessibilityLabel, "Ryu")
        XCTAssertTrue(view.accessibilityTraits.contains(.header))
        XCTAssertEqual(view.accessibilityIdentifier, "header")

        // A batch that makes the header taller: UIKit's header follows.
        p.apply(wireBatch([["op": "frame", "id": 40, "x": 0.0, "y": 0.0, "w": 402.0, "h": 60.0]]))
        l.collection.layoutIfNeeded()
        XCTAssertEqual(try header(p, section: 0).frame.height, 60, accuracy: 0.5)
        XCTAssertTrue(node.superview === (try header(p, section: 0)).contentView)
    }

    func testAOneTextHeaderIsUIKitsOwnBesideACustomOne() throws {
        let p = presenter(height: 600, props: ["listStyle": "plain"]) { self.favorites("plain") }
        p.apply(wireBatch(headerOps()))
        let plain = try header(p, section: 1)
        let text = try XCTUnwrap(plain.contentConfiguration as? UIListContentConfiguration, "UIKit's header content")
        XCTAssertEqual(text.attributedText?.string, "Plain")
        XCTAssertNil(plain.height, "sized by UIKit")
        XCTAssertFalse(plain.speaks)
        XCTAssertTrue(plain.contentView.subviews.allSatisfy { !($0 is NodeView) })

        // The same list with no custom header: the one-text header is the same view, the same size.
        let q = presenter(height: 600, props: ["listStyle": "plain"]) { self.favorites("plain", custom: false) }
        let alone = try header(q, section: 1)
        XCTAssertEqual(alone.frame.size, plain.frame.size)
        XCTAssertEqual((alone.contentConfiguration as? UIListContentConfiguration)?.attributedText, text.attributedText)
        XCTAssertEqual(alone.accessibilityTraits, plain.accessibilityTraits)
        XCTAssertEqual(alone.isAccessibilityElement, plain.isAccessibilityElement)
        let first = try header(q, section: 0)
        XCTAssertEqual((first.contentConfiguration as? UIListContentConfiguration)?.attributedText?.string, "Ryu")
    }

    func testACustomHeaderPinsAsAPlainListsHeaderDoesAndComesBackAfterItLeaves() throws {
        func pinned(custom: Bool) throws -> (offset: CGFloat, header: CGRect, presenter: Presenter) {
            let p = presenter(height: 100, props: ["listStyle": "plain"]) { self.favorites("plain", custom: custom) }
            if custom { p.apply(wireBatch(headerOps())) }
            let c = try list(p).collection
            c.layoutIfNeeded()
            c.contentOffset.y += 70
            c.layoutIfNeeded()
            return (c.contentOffset.y + c.adjustedContentInset.top, try header(p, section: 0).frame, p)
        }
        let native = try pinned(custom: false)
        XCTAssertEqual(native.header.minY, native.offset, accuracy: 0.5, "UIKit pins a plain list's header")
        let custom = try pinned(custom: true)
        XCTAssertEqual(custom.header.minY, custom.offset, accuracy: 0.5, "the custom header is pinned the same")
        let p = custom.presenter, c = try list(p).collection
        let node = try XCTUnwrap(p.views[40]), name = try XCTUnwrap(p.views[42])
        XCTAssertTrue(node.superview === (try header(p, section: 0)).contentView)
        XCTAssertEqual(name.convert(name.bounds, to: c).minY, custom.offset + 12, accuracy: 0.5, "its text rides with it")

        // Past its section the header leaves, and its views go home.
        c.contentOffset.y = max(0, c.contentSize.height + c.adjustedContentInset.bottom - c.bounds.height)
        c.layoutIfNeeded()
        XCTAssertNil(c.supplementaryView(forElementKind: headerKind, at: IndexPath(item: 0, section: 0)), "scrolled past")
        XCTAssertTrue(node.superview === p.views[2]?.container, "home, under its hidden section")
        XCTAssertTrue((try header(p, section: 1)).contentView.subviews.allSatisfy { !($0 is NodeView) }, "no view of it in another header")

        c.contentOffset.y = -c.adjustedContentInset.top
        c.layoutIfNeeded()
        let back = try header(p, section: 0)
        XCTAssertTrue(node.superview === back.contentView, "carried again as its header shows")
        XCTAssertEqual(name.convert(name.bounds, to: back), CGRect(x: 48, y: 12, width: 100, height: 20))
        XCTAssertEqual(back.frame.height, 44, accuracy: 0.5)
    }

    func testAHeaderThatTurnsToOneTextIsGivenBackToUIKit() throws {
        var custom = true
        let p = presenter(height: 600, props: ["listStyle": "plain"]) { self.favorites("plain", custom: custom) }
        p.apply(wireBatch(headerOps()))
        let node = try XCTUnwrap(p.views[40])
        XCTAssertTrue(node.superview === (try header(p, section: 0)).contentView)
        custom = false
        p.apply(wireBatch([["op": "props", "id": 2, "set": ["testId": "section"], "clear": [String]()]]))
        let view = try header(p, section: 0)
        XCTAssertTrue(node.superview === p.views[2]?.container, "given back")
        XCTAssertEqual((view.contentConfiguration as? UIListContentConfiguration)?.attributedText?.string, "Ryu")
        XCTAssertNil(view.height)
        XCTAssertFalse(view.speaks)
        XCTAssertNil(view.accessibilityIdentifier)
        custom = true
        p.apply(wireBatch([["op": "props", "id": 2, "set": ["testId": "again"], "clear": [String]()]]))
        XCTAssertTrue(node.superview === (try header(p, section: 0)).contentView, "custom again: carried")
        XCTAssertEqual(try header(p, section: 0).frame.height, 44, accuracy: 0.5)
    }

    /// UIKit insets an inset-grouped list's header with its card; the
    /// kernel lays the header out across the section, its insets inside it.
    func testAnInsetListsCustomHeaderStandsWhereTheKernelPutItInTheList() throws {
        var parts = false
        let p = presenter(height: 600) { self.favorites("inset-grouped", parts: parts) }
        p.apply(wireBatch(headerOps()))
        let l = try list(p), view = try header(p, section: 0)
        let avatar = try XCTUnwrap(p.views[41])
        XCTAssertGreaterThan(view.frame.minX, 0, "UIKit's header starts at the card's inset")
        XCTAssertEqual(avatar.convert(avatar.bounds, to: l.collection).minX, 16, accuracy: 0.5, "the kernel's 16 from the list's edge")
        XCTAssertTrue(view.speaks)
        // A header with a control keeps each part its own element.
        parts = true
        p.apply(wireBatch([
            ["op": "create", "id": 45, "kind": "button", "props": ["testId": "more"], "handlers": ["press"], "style": [:]],
            ["op": "children", "id": 44, "ids": [41, 42, 45]],
            ["op": "frame", "id": 45, "x": 300.0, "y": 0.0, "w": 44.0, "h": 24.0],
        ]))
        XCTAssertFalse(try header(p, section: 0).speaks)
        XCTAssertNil(try header(p, section: 0).accessibilityLabel)
    }

    /// The whole path, from a `.contract` file: the compiler takes the
    /// header, the kernel names it and lays it out by UIKit's margins, and
    /// the list draws it pinned, over rows that stand where they do under a
    /// header of one text.
    func testACompiledHeaderOfAnAvatarAndANameIsDrawnAtUIKitsMargin() throws {
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_GROUPED_HEADER_PLAN"])
        ExactGroupedLists.install()
        let session = ExactApp.shared.makeSession(label: "grouped-header")
        defer { session.destroy(); window?.isHidden = true }
        let view = ExactView(session: session), root = UIViewController()
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "the list's margins need a hosted window scene")
        }
        window = UIWindow(windowScene: scene)
        window.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        window.rootViewController = root
        window.makeKeyAndVisible()
        view.frame = root.view.bounds
        root.view.addSubview(view)
        XCTAssertNil(session.boot(plan: try Data(contentsOf: URL(fileURLWithPath: path)), size: view.bounds.size).error)
        let p = session.presenter, agent = Agent(session: session)
        func settle() {
            window.layoutIfNeeded()
            RunLoop.main.run(until: Date().addingTimeInterval(0.05))
            let reply = agent.clock(["settle": true])
            XCTAssertEqual(reply["settled"] as? Bool, true, "\(reply)")
            window.layoutIfNeeded()
        }
        func node(_ name: String) throws -> NodeView { try XCTUnwrap(p.views.values.first { $0.props["testId"] == name }, name) }
        settle()
        let owner = try node("list"), l = try XCTUnwrap(host(p).lists[owner.id]), c = l.collection
        func shownHeader() throws -> GroupedHeaderView {
            c.layoutIfNeeded()
            return try XCTUnwrap(c.supplementaryView(forElementKind: headerKind, at: IndexPath(item: 0, section: 0)) as? GroupedHeaderView)
        }
        /// Where the first row's title starts, in the list.
        func titleX() throws -> CGFloat { let title = try node("title-Ryu-1"); return title.convert(title.bounds, to: c).minX }
        c.layoutIfNeeded()
        XCTAssertEqual(l.model.sections.map(\.header), ["Ryu", "Ken", "Guile", "Plain"])
        XCTAssertEqual(l.model.sections.map { $0.headerView != nil }, [true, true, true, false])
        let told = try XCTUnwrap(host(p).told[owner.id], "the kernel is told UIKit's margins for a custom header")
        let shown = try shownHeader()
        let header = try node("header-Ryu"), avatar = try node("avatar-Ryu"), name = try node("name-Ryu")
        XCTAssertTrue(header.superview === shown.contentView)
        let box = avatar.convert(avatar.bounds, to: c), text = name.convert(name.bounds, to: c)
        let cell = try XCTUnwrap(c.visibleCells.first as? UICollectionViewListCell)
        let margin = cell.convert(cell.bounds, to: c).minX + cell.directionalLayoutMargins.leading
        XCTAssertEqual(box.size, CGSize(width: 24, height: 24))
        XCTAssertEqual(told[0] + told[2], margin, accuracy: 0.5)
        XCTAssertEqual(box.minX, margin, accuracy: 0.5, "at UIKit's margin, where a standard row's text starts")
        XCTAssertEqual(text.minX - box.maxX, 8, accuracy: 0.5, "the name 8 points after the avatar")
        XCTAssertEqual(shown.frame.height, header.bounds.height, accuracy: 0.5, "as tall as the kernel's header")
        XCTAssertEqual(shown.frame.height, 44, accuracy: 0.5, "the avatar's 24 in the sheet's 10 above and below")
        XCTAssertEqual(box.minY - shown.frame.minY, 10, accuracy: 0.5)
        XCTAssertGreaterThan(text.width, 0)
        XCTAssertEqual(shown.accessibilityLabel, "Ryu")
        XCTAssertTrue(shown.isAccessibilityElement && shown.accessibilityTraits.contains(.header))
        let customTitle = try titleX(), customTop = shown.frame.minY
        XCTAssertEqual(customTitle, margin, accuracy: 0.5, "a row's content at the cell's margin")

        // A list built behind a tab is out of the window, where UIKit has
        // not yet given its cells their margins; shown with no batch, the
        // kernel is told the margins that show, and rows and header follow.
        view.removeFromSuperview()
        host(p).told[owner.id] = [0, 0, 8, 8]
        session.apply(session.runtime.groupedMargins([(owner.id, [0, 0, 8, 8])]))
        settle()
        XCTAssertEqual(host(p).told[owner.id], [0, 0, 8, 8], "nothing is told of a list out of the window")
        root.view.addSubview(view)
        settle()
        XCTAssertEqual(host(p).told[owner.id], told, "told UIKit's margins as the list shows")
        XCTAssertEqual(try titleX(), customTitle, accuracy: 0.5, "the rows at the cell's margin")
        XCTAssertEqual(avatar.convert(avatar.bounds, to: c).minX, margin, accuracy: 0.5, "and the header's content")

        // The same list with a header of one text: its rows stand where
        // they do under the custom header, and UIKit's own header shows
        // the insets the sheet stands in for.
        let toggle = try node("toggle")
        XCTAssertNil(agent.tap(["id": Int(toggle.id)])["error"])
        settle()
        let native = try shownHeader()
        XCTAssertNotNil(native.contentConfiguration as? UIListContentConfiguration)
        XCTAssertEqual(try titleX(), customTitle, accuracy: 0.5, "rows keep their inset under either header")
        XCTAssertEqual(native.frame.minY, customTop, accuracy: 0.5, "the list's header top padding is above either header")
        func labels(_ v: UIView) -> [UIView] { (v is UILabel ? [v] : []) + v.subviews.flatMap(labels) }
        let label = try XCTUnwrap(labels(native).first).convert(try XCTUnwrap(labels(native).first).bounds, to: native)
        XCTAssertEqual(label.minX, box.minX, accuracy: 0.5, "UIKit's header text starts where the custom header's content does")
        XCTAssertEqual(label.minY, 10, accuracy: 0.5, "UIKit's header content margin, the sheet's 10")
        XCTAssertEqual(native.frame.height - label.height, 20, accuracy: 0.5)
        XCTAssertNil(agent.tap(["id": Int(toggle.id)])["error"])
        settle()

        let again = try shownHeader()
        let inset = try node("avatar-Ryu").convert(try node("avatar-Ryu").bounds, to: c).minY - again.frame.minY
        c.contentOffset.y += 120
        c.layoutIfNeeded()
        let pinned = try shownHeader()
        XCTAssertEqual(pinned.frame.minY, c.contentOffset.y + c.adjustedContentInset.top, accuracy: 0.5, "pinned over its rows")
        XCTAssertTrue(try node("header-Ryu").superview === pinned.contentView)
        XCTAssertEqual(try node("avatar-Ryu").convert(try node("avatar-Ryu").bounds, to: c).minY - pinned.frame.minY, inset, accuracy: 0.5, "its views ride with it")
    }
}
#endif
