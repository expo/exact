#if os(iOS)
@testable import ExactKit
import UIKit
import XCTest

/// D13's reach for a scroller on iOS (LLP 1123): its scroll view outset past its box by the
/// kernel's reach (`setReach`), with D14's writer holding the insets measured from the reached
/// edge, so the content stays where layout put it. Activation applies the two together; these
/// tests apply them to one scroller.
@MainActor
final class ReachIOSTests: XCTestCase {
    private var window: UIWindow?
    /// The presenter, which its nodes hold weakly.
    private var presenter: Presenter?

    override func tearDown() {
        window = nil
        presenter = nil
        super.tearDown()
    }

    func testReachOutsetsTheScrollViewAndTheWriterKeepsTheContentWhereLayoutPutIt() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "reach and the band are a scene window's") }
        // At the top band's inner side, reaching the window's top, and at the bottom band's,
        // reaching its bottom, wherever the device has that band
        var reached = 0
        for edge in [Place.top, .bottom] {
            let (node, scroll, owner) = try scroller(in: scene, at: edge, clipsX: true)
            let band = edge.band(owner)
            guard band > 0 else { continue }
            reached += 1
            XCTAssertTrue(node.clipsToBounds, "\(edge): the box clips its other axis")
            node.setReach(edge.reach(band))
            XCTAssertEqual(edge == .top ? scroll.frame.minY : scroll.frame.maxY, edge == .top ? -band : node.bounds.height + band, "\(edge)")
            XCTAssertFalse(node.clipsToBounds, "\(edge): the box's own clip would cut the reached strip off")
            ScrollInsetsIOS.write(scroll, in: owner, terms: .init(reach: edge.edges(band)), rows: .init())
            if edge == .top {
                XCTAssertEqual(scroll.convert(CGPoint.zero, to: node).y, 0, accuracy: 0.5, "the content's top is the box's")
            } else {
                scroll.contentOffset.y = scroll.contentSize.height + scroll.adjustedContentInset.bottom - scroll.bounds.height
                XCTAssertEqual(scroll.convert(CGPoint(x: 0, y: scroll.contentSize.height), to: node).y, node.bounds.height, accuracy: 0.5,
                               "the content's end is the box's")
            }
            node.setReach(.zero)
            XCTAssertTrue(node.clipsToBounds, "\(edge): the clip comes back")
        }
        XCTAssertGreaterThan(reached, 0, "the device has a band to reach into")
    }

    func testReachAndTheWriterKeepTheScrollTop() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "reach and the band are a scene window's") }
        // Scrolled 300 into the content and at its end, reach and the writer keep `scrollTop`, and
        // taking reach away keeps it too
        var reached = 0
        for edge in [Place.top, .bottom] {
            for at in ["300", "the end"] {
                let (node, scroll, owner) = try scroller(in: scene, at: edge)
                let band = edge.band(owner)
                guard band > 0 else { continue }
                reached += 1
                let scrollTop = at == "300" ? 300 : scroll.contentSize.height - node.bounds.height
                scroll.contentOffset.y = scrollTop - node.scrollTopInset(scroll)
                node.setReach(edge.reach(band))
                XCTAssertEqual(scroll.contentOffset.y + node.scrollTopInset(scroll), scrollTop, accuracy: 0.5, "\(edge), \(at): reach")
                ScrollInsetsIOS.write(scroll, in: owner, terms: .init(reach: edge.edges(band)), rows: .init())
                XCTAssertEqual(scroll.contentOffset.y + node.scrollTopInset(scroll), scrollTop, accuracy: 0.5, "\(edge), \(at): the writer")
                node.setReach(.zero)
                ScrollInsetsIOS.write(scroll, in: owner, terms: .init(), rows: .init())
                XCTAssertEqual(scroll.contentOffset.y + node.scrollTopInset(scroll), scrollTop, accuracy: 0.5, "\(edge), \(at): taking reach away")
            }
        }
        XCTAssertGreaterThan(reached, 0, "the device has a band to reach into")
    }

    func testReachAndTheWriterKeepAFollowersPlace() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "reach and the band are a scene window's") }
        // A list following its end, short of it by less than the reach, keeps its `scrollTop` as
        // reach and the writer come; at its end it stays at its end
        var reached = 0
        for edge in [Place.top, .bottom] {
            for atEnd in [false, true] {
                let (node, scroll, owner) = try scroller(in: scene, at: edge, props: ["scrollFollowEnd": "true"])
                let band = edge.band(owner)
                guard band > 1 else { continue }
                reached += 1
                let follower = try XCTUnwrap(scroll as? ScrollView)
                let scrollTop = scroll.contentSize.height - node.bounds.height - (atEnd ? 0 : min(20, band / 2))
                scroll.contentOffset.y = scrollTop - node.scrollTopInset(scroll)
                node.setReach(edge.reach(band))
                ScrollInsetsIOS.write(scroll, in: owner, terms: .init(reach: edge.edges(band)), rows: .init())
                if atEnd {
                    XCTAssertEqual(scroll.contentOffset.y, follower.end(under: scroll.adjustedContentInset), accuracy: 0.5, "\(edge): at its end")
                } else {
                    XCTAssertEqual(scroll.contentOffset.y + node.scrollTopInset(scroll), scrollTop, accuracy: 0.5, "\(edge): short of its end")
                }
            }
        }
        XCTAssertGreaterThan(reached, 0, "the device has a band to reach into")
    }

    func testTheScrollViewLiesPastItsBoxByTheLargerOfBandAndReach() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "reach and the band are a scene window's") }
        let (node, scroll, _) = try scroller(in: scene, at: .middle)
        node.setPageBand(UIEdgeInsets(top: 20, left: 0, bottom: 0, right: 0))
        node.setReach(UIEdgeInsets(top: 40, left: 0, bottom: 0, right: 0))
        XCTAssertEqual(scroll.frame.minY, -40, "reach past the band")
        node.setReach(UIEdgeInsets(top: 10, left: 0, bottom: 0, right: 0))
        XCTAssertEqual(scroll.frame.minY, -20, "the band past reach")
        node.setReach(.zero)
        XCTAssertEqual(scroll.frame.minY, -20, "the band alone")
    }

    func testATouchInTheReachedStripReachesTheScrollView() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "reach and the band are a scene window's") }
        var reached = 0
        for edge in [Place.top, .bottom] {
            let (node, scroll, owner) = try scroller(in: scene, at: edge)
            let band = edge.band(owner)
            guard band >= 10 else { continue }
            reached += 1
            let point = CGPoint(x: 20, y: edge == .top ? -band / 2 : node.bounds.height + band / 2)
            XCTAssertFalse(isScroll(node.hitTest(point, with: nil), scroll), "\(edge): no reach, no touch past the box")
            node.setReach(edge.reach(band))
            XCTAssertTrue(isScroll(node.hitTest(point, with: nil), scroll), "\(edge)")
        }
        XCTAssertGreaterThan(reached, 0, "the device has a band to reach into")
    }

    func testTheKernelHearsTheScrollPositionPastTheReach() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "reach and the band are a scene window's") }
        let (node, scroll, owner) = try scroller(in: scene, at: .top)
        let band = owner.safeAreaInsets.top
        try XCTSkipIf(band < 1, "no top band to reach into; the side reach test covers the kernel's x")
        var heard: [Double] = []
        node.presenter?.onScrolled = { _, _, y in heard.append(y) }
        node.setReach(Place.top.reach(band))
        ScrollInsetsIOS.write(scroll, in: owner, terms: .init(reach: [band, 0, 0, 0]), rows: .init())
        node.scrollViewDidScroll(scroll)
        XCTAssertEqual(heard.last ?? -1, 0, accuracy: 0.5, "at rest, the top of the content")
        scroll.contentOffset.y += 120
        node.scrollViewDidScroll(scroll)
        XCTAssertEqual(heard.last ?? -1, 120, accuracy: 0.5)
    }

    func testASideReachWithTheWriterKeepsTheContentInTheBoxAndScrollLeftInCSS() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "reach and the band are a scene window's") }
        // 30 on the left and 20 on the right, the writer holding them as `reserved`, as it does
        // for a reach into a side band
        let reach = UIEdgeInsets(top: 0, left: 30, bottom: 0, right: 20)
        let terms = ScrollInsetsIOS.Terms(reach: [0, 20, 0, 30], reserved: [nil, 20, nil, 30])
        // A vertical list: its content is the box's width, so UIKit cannot pan it sideways, and
        // its left is the box's
        let (list, listScroll, listOwner) = try scroller(in: scene, at: .middle)
        list.setReach(reach)
        ScrollInsetsIOS.write(listScroll, in: listOwner, terms: terms, rows: .init())
        XCTAssertEqual([listScroll.frame.minX, listScroll.frame.width], [-30, list.bounds.width + 50])
        XCTAssertEqual(listScroll.contentSize.width, list.bounds.width, accuracy: 0.5)
        XCTAssertEqual(listScroll.contentSize.width + listScroll.adjustedContentInset.left + listScroll.adjustedContentInset.right,
                       listScroll.bounds.width, accuracy: 0.5, "no sideways range")
        XCTAssertEqual(listScroll.convert(CGPoint.zero, to: list).x, 0, accuracy: 0.5, "the content's left is the box's")
        // A scroller of both axes, 2000 wide: CSS `scrollLeft` lies past the left inset, in a write,
        // the `scroll` event, the kernel's position and a touch in the strip
        let (node, scroll, owner) = try scroller(in: scene, at: .middle, wide: true)
        var heard: [Double] = []
        node.presenter?.onScrolled = { _, x, _ in heard.append(x) }
        node.setReach(reach)
        ScrollInsetsIOS.write(scroll, in: owner, terms: terms, rows: .init())
        XCTAssertEqual(node.scrollLeftInset(scroll), 30)
        node.pendingScrollLeft = 100
        node.applyPendingScroll(instant: true)
        XCTAssertEqual(scroll.contentOffset.x, 70, accuracy: 0.5, "a scrollLeft of 100 lies 30 past the inset")
        node.handlers = ["scroll"]
        node.sendScrollEvent()
        XCTAssertEqual([node.lastScrollEvent[0], node.lastScrollEvent[2]], [100, 2000], "scrollLeft and scrollWidth")
        node.scrollViewDidScroll(scroll)
        XCTAssertEqual(heard.last ?? -1, 100, accuracy: 0.5, "the kernel's x")
        XCTAssertTrue(isScroll(node.hitTest(CGPoint(x: -15, y: 100), with: nil), scroll), "a touch in the left strip")
        node.pendingScrollLeft = nil
        node.retainScrollPosition(from: scroll)
        XCTAssertEqual(node.pendingScrollLeft, 100, "the scrollLeft a replaced scroll view carries")
        // Scrolling sideways with content the box's width, the side reach adds no sideways range
        let (fits, fitsScroll, fitsOwner) = try scroller(in: scene, at: .middle, wide: true, width: list.bounds.width)
        fits.setReach(reach)
        ScrollInsetsIOS.write(fitsScroll, in: fitsOwner, terms: terms, rows: .init())
        XCTAssertEqual(fitsScroll.contentSize.width, fits.bounds.width, accuracy: 0.5)
        XCTAssertEqual(fitsScroll.contentSize.width + fitsScroll.adjustedContentInset.left + fitsScroll.adjustedContentInset.right,
                       fitsScroll.bounds.width, accuracy: 0.5, "no sideways range")
    }

    func testReachGoesWithItsScrollView() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "reach and the band are a scene window's") }
        // A scroller with a side reach stops scrolling: its box clips again; scrolling once more, its
        // new scroll view lies at the box until reach is set again
        let (node, _, _) = try scroller(in: scene, at: .middle, clipsX: true)
        node.setReach(UIEdgeInsets(top: 0, left: 30, bottom: 0, right: 20))
        XCTAssertFalse(node.clipsToBounds)
        let scrolling = node.style
        var still = scrolling
        still["overflow_y"] = .string("visible")
        node.applyStyle(still)
        XCTAssertNil(node.scrollView)
        XCTAssertEqual(node.reach, .zero)
        XCTAssertTrue(node.clipsToBounds, "the box clips its other axis again")
        node.applyStyle(scrolling)
        let scroll = try XCTUnwrap(node.scrollView)
        XCTAssertEqual(scroll.frame, node.bounds)
        XCTAssertEqual(node.reach, .zero)
    }

    private func isScroll(_ hit: UIView?, _ scroll: UIScrollView) -> Bool {
        hit.map { $0 === scroll || $0.isDescendant(of: scroll) } ?? false
    }

    /// Where a test's scroller lies: at the top band's inner side, at the bottom band's, or 200
    /// below the top band's.
    private enum Place: CustomStringConvertible {
        case top, bottom, middle
        var description: String { self == .top ? "top" : self == .bottom ? "bottom" : "middle" }
        /// The owner's band on this edge.
        func band(_ owner: UIView) -> CGFloat { self == .top ? owner.safeAreaInsets.top : self == .bottom ? owner.safeAreaInsets.bottom : 0 }
        /// A reach of `depth` on this edge, as `setReach` takes it.
        func reach(_ depth: CGFloat) -> UIEdgeInsets {
            UIEdgeInsets(top: self == .top ? depth : 0, left: 0, bottom: self == .bottom ? depth : 0, right: 0)
        }
        /// The same reach as the kernel's terms give it, top right bottom left.
        func edges(_ depth: CGFloat) -> [CGFloat] { [self == .top ? depth : 0, 0, self == .bottom ? depth : 0, 0] }
    }

    /// A presenter in a window of `scene`, its viewport filling the root view, holding scroller 1
    /// at `at`, 500 tall, with content 2000 tall and scrolling sideways too where `wide`, its content
    /// then `width` wide (2000 by default), clipping sideways where `clipsX`, with `props`. The
    /// scroller's node, its scroll view, and the root view, the owner's view here.
    private func scroller(in scene: UIWindowScene, at place: Place, wide: Bool = false, width: CGFloat = 2000, clipsX: Bool = false,
                          props: [String: String] = [:]) throws -> (NodeView, UIScrollView, UIView) {
        let window = UIWindow(windowScene: scene)
        window.frame = scene.coordinateSpace.bounds
        let root = UIViewController()
        window.rootViewController = root
        window.makeKeyAndVisible()
        self.window = window
        root.view.layoutIfNeeded()
        let p = Presenter()
        presenter = p
        p.viewport.frame = root.view.bounds
        root.view.addSubview(p.viewport)
        let area = root.view.safeAreaInsets
        let y = place == .bottom ? root.view.bounds.height - area.bottom - 500 : area.top + (place == .top ? 0 : 200)
        let overflow: [String: Any] = ["overflow_y": "scroll", "overflow_x": wide ? "scroll" : clipsX ? "hidden" : "visible"]
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "props": props, "style": overflow],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": y, "w": root.view.bounds.width, "h": 500],
            ["op": "content", "id": 1, "w": wide ? width : root.view.bounds.width, "h": 2000],
        ]))
        root.view.layoutIfNeeded()
        let node = try XCTUnwrap(p.views[1]), scroll = try XCTUnwrap(node.scroll)
        XCTAssertEqual(node.convert(CGPoint.zero, to: root.view).y, y, accuracy: 0.5, "the box is where layout put it")
        return (node, scroll, root.view)
    }
}
#endif
