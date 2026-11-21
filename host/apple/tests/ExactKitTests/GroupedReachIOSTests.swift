#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactGroupedLists

/// A grouped list under D13's reach (LLP 1123): its collection's frame grows over the side band once,
/// whether the list's own side reach or the kernel's gives it, its cards stand where the list's own
/// reach puts them, inset by UIKit from the depth its layout consumes, and D14's writer writes none of it.
final class GroupedReachIOSTests: GroupedListFixture {
    func testAGroupedListUnderTheKernelsSideReachGrowsOnceAndConsumesTheBand() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a grouped list's side band is a scene window's safe area") }
        let p = presenter { self.model() }
        let root = inScene(p, scene)
        let band = root.view.safeAreaInsets
        try XCTSkipIf(band.left + band.right < 1, "no side band on this device")
        let node = try XCTUnwrap(p.views[1]), l = try list(p), collection = l.collection
        // The list's box between the side bands' inner sides
        let box = CGRect(x: band.left, y: band.top, width: root.view.bounds.width - band.left - band.right, height: root.view.bounds.height - band.top - band.bottom)
        let place = { (height: CGFloat) in p.apply(wireBatch([["op": "frame", "id": 1, "x": box.minX, "y": box.minY, "w": box.width, "h": height]])) }
        place(box.height)
        root.view.layoutIfNeeded()
        let cell = { () throws -> CGRect in
            collection.layoutIfNeeded()
            return collection.convert(try XCTUnwrap(collection.layoutAttributesForItem(at: IndexPath(item: 0, section: 0))).frame, to: root.view)
        }
        // Today: the list's own side reach lays the collection over the bands
        let reached = box.inset(by: UIEdgeInsets(top: 0, left: -band.left, bottom: 0, right: -band.right))
        XCTAssertEqual(collection.convert(collection.bounds, to: root.view), reached)
        let card = try cell()
        // The kernel's side reach gives the same frame, through a batch's mount too
        node.setReach(UIEdgeInsets(top: 0, left: band.left, bottom: 0, right: band.right))
        place(box.height + 1)
        place(box.height)
        root.view.layoutIfNeeded()
        XCTAssertEqual(collection.convert(collection.bounds, to: root.view), reached, "the side reach counted once")
        XCTAssertEqual(try cell(), card, "the cards where the list's own reach put them")
        // The writer leaves the side band to the list's layout
        ScrollInsetsIOS.write(collection, in: root.view, terms: .init(reach: [0, band.right, 0, band.left]), rows: .init())
        XCTAssertEqual([collection.contentInset.left, collection.contentInset.right], [0, 0])
        XCTAssertEqual([collection.adjustedContentInset.left, collection.adjustedContentInset.right], [0, 0])
        XCTAssertEqual(try cell(), card, "the cards inset from the band once")
        // A left reach of 30 more than the list's own, a reservation clear of the band, given between batches: the
        // frame, the cards and the margins the kernel is told follow at once
        let margins = try XCTUnwrap(l.margins())
        node.setReach(UIEdgeInsets(top: 0, left: band.left + 30, bottom: 0, right: band.right))
        let wider = reached.inset(by: UIEdgeInsets(top: 0, left: -30, bottom: 0, right: 0))
        XCTAssertEqual(collection.convert(collection.bounds, to: root.view), wider, "the frame, with no batch")
        XCTAssertEqual(try cell(), card, "the cards where they were")
        XCTAssertEqual(try XCTUnwrap(l.margins()), margins, "the margins mapped by the new reach")
        // The writer, the reservation as `reserved`, under `never` and `always`: no side inset of its own, UIKit's
        // adjustment over the consumed depth cancelled, and the cards where they were
        for adjustment in [UIScrollView.ContentInsetAdjustmentBehavior.never, .always] {
            var rows = ScrollInsetsIOS.Rows()
            rows.adjustment = adjustment
            ScrollInsetsIOS.write(collection, in: root.view, terms: .init(reach: [0, band.right, 0, band.left + 30], reserved: [nil, nil, nil, band.left + 30]), rows: rows)
            XCTAssertEqual([collection.adjustedContentInset.left, collection.adjustedContentInset.right], [0, 0], "\(adjustment.rawValue)")
            XCTAssertEqual(try cell(), card, "\(adjustment.rawValue): the cards inset once")
        }
    }

    func testACustomHeaderAndTheMarginsFollowTheKernelsReachAndItsZero() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a grouped list's side band is a scene window's safe area") }
        let p = presenter { self.favorites("inset-grouped") }
        p.apply(wireBatch(headerOps()))
        let root = inScene(p, scene)
        let band = root.view.safeAreaInsets
        try XCTSkipIf(band.right < 1, "no right band on this device")
        let node = try XCTUnwrap(p.views[1]), l = try list(p), collection = l.collection, avatar = try XCTUnwrap(p.views[41])
        // The list's box over the right band, laid out across the whole width
        let box = CGRect(x: 0, y: band.top, width: root.view.bounds.width, height: root.view.bounds.height - band.top - band.bottom)
        p.apply(wireBatch([["op": "frame", "id": 1, "x": box.minX, "y": box.minY, "w": box.width, "h": box.height]]))
        root.view.layoutIfNeeded()
        let header = { () -> CGFloat in collection.layoutIfNeeded(); return avatar.convert(avatar.bounds, to: node).minX }
        let card = { () throws -> CGRect in
            collection.layoutIfNeeded()
            return collection.convert(try XCTUnwrap(collection.layoutAttributesForItem(at: IndexPath(item: 0, section: 0))).frame, to: root.view)
        }
        let (kernels, cards) = (header(), try card())
        // A left reach of 30, between batches, then a page band under it: the header and the cards stay where the kernel
        // and UIKit put them, and the sampled margins follow at once
        let margins = try XCTUnwrap(l.margins())
        node.setReach(UIEdgeInsets(top: 0, left: 30, bottom: 0, right: 0))
        XCTAssertEqual(collection.convert(collection.bounds, to: root.view).minX, -30)
        XCTAssertEqual(header(), kernels, accuracy: 0.5, "the header where the kernel put it")
        XCTAssertEqual(try XCTUnwrap(l.margins()), margins, "the margins mapped by the new reach")
        // A vertical scroll reports no `scrollLeft` for the reach the list's layout consumes
        var reported: Double?
        p.onScrolled = { _, x, _ in reported = x }
        collection.contentOffset.y += 40
        XCTAssertEqual(try XCTUnwrap(reported), 0, accuracy: 0.5, "scrollLeft at rest")
        XCTAssertEqual(header(), kernels, accuracy: 0.5)
        collection.contentOffset.y -= 40
        node.setPageBand(UIEdgeInsets(top: 20, left: 0, bottom: 0, right: 0))
        XCTAssertEqual(collection.convert(collection.bounds, to: root.view).minX, -30, "a page band keeps the side reach")
        XCTAssertEqual(collection.convert(collection.bounds, to: root.view).maxX, box.maxX)
        XCTAssertEqual(header(), kernels, accuracy: 0.5)
        XCTAssertEqual([try card().minX, try card().maxX], [cards.minX, cards.maxX])
        node.setPageBand(.zero)
        // The kernel's answer of zero: the collection lies at the box, over the band, whose depth UIKit gives it and the
        // list consumes; the writer under `never` leaves it to the layout, its region observed or ignored, since an inset
        // does not move the cards UIKit lays out from the safe area
        node.setReach(.zero)
        XCTAssertEqual(collection.convert(collection.bounds, to: root.view), box)
        XCTAssertEqual((collection as? BandConsumer)?.consumedBand[1] ?? 0, band.right, accuracy: 0.5, "the inherited side depth the layout clears")
        var never = ScrollInsetsIOS.Rows()
        never.adjustment = .never
        ScrollInsetsIOS.write(collection, in: root.view, terms: .init(), rows: never)
        XCTAssertEqual(collection.contentInset.right, 0, "the band cleared once, by the layout")
        XCTAssertEqual(header(), kernels, accuracy: 0.5)
        let observed = try card()
        ScrollInsetsIOS.write(collection, in: root.view, terms: .init(container: [true, false, true, true]), rows: never)
        XCTAssertEqual(collection.contentInset.right, 0, "ignored")
        collection.contentInset.right = -band.right
        XCTAssertEqual(try card().maxX, observed.maxX, accuracy: 0.5, "a negative inset leaves the card")
        collection.contentInset.right = 0
    }

    func testAGroupedListsFirstKernelAnswerOfZeroLaysItAtItsBox() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a grouped list's side band is a scene window's safe area") }
        let p = presenter { self.model() }
        let root = inScene(p, scene)
        let band = root.view.safeAreaInsets
        try XCTSkipIf(band.left + band.right < 1, "no side band on this device")
        let node = try XCTUnwrap(p.views[1]), collection = try list(p).collection
        let box = CGRect(x: band.left, y: band.top, width: root.view.bounds.width - band.left - band.right, height: root.view.bounds.height - band.top - band.bottom)
        let place = { (height: CGFloat) in p.apply(wireBatch([["op": "frame", "id": 1, "x": box.minX, "y": box.minY, "w": box.width, "h": height]])) }
        place(box.height)
        root.view.layoutIfNeeded()
        XCTAssertNotEqual(collection.convert(collection.bounds, to: root.view), box, "the list's own side reach")
        node.setReach(.zero)
        XCTAssertEqual(collection.convert(collection.bounds, to: root.view), box, "the kernel's zero, at once")
        place(box.height + 1)
        place(box.height)
        XCTAssertEqual(collection.convert(collection.bounds, to: root.view), box, "and through a batch's mount")
    }

    /// `p`'s viewport in a window of `scene`, under a root view controller whose view is the owner's view here.
    private func inScene(_ p: Presenter, _ scene: UIWindowScene) -> UIViewController {
        window.isHidden = true
        window = UIWindow(windowScene: scene)
        window.frame = scene.coordinateSpace.bounds
        let root = UIViewController()
        window.rootViewController = root
        window.makeKeyAndVisible()
        p.viewport.frame = root.view.bounds
        root.view.addSubview(p.viewport)
        root.view.layoutIfNeeded()
        return root
    }
}
#endif
