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
    /// A plain list beside a side band (the folded iPhone Duo's strip): its cells end at the band's inner side and
    /// their ground runs once across the band to the screen's edge, never past it; mounted by its frame with no batch
    /// (`GroupedListHost.geometryChanged`), a list moved off the band loses the strips and one moved back regains them,
    /// and one mounted out of the window regains them once it is in one.
    func testAPlainListsGroundReachesAcrossTheBandOnceAndFollowsItsFrame() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a grouped list's side band is a scene window's safe area") }
        let p = presenter(props: ["listStyle": "plain"]) { self.model("plain") }
        let root = inScene(p, scene)
        let band = root.view.safeAreaInsets
        try XCTSkipIf(band.right < 1, "no right band on this device")
        let box = CGRect(x: band.left, y: band.top, width: root.view.bounds.width - band.left - band.right, height: root.view.bounds.height - band.top - band.bottom)
        p.apply(wireBatch([["op": "frame", "id": 1, "x": box.minX, "y": box.minY, "w": box.width, "h": box.height]]))
        root.view.layoutIfNeeded()
        let cell = try XCTUnwrap(try self.cell(p, 10) as? GroupedCell)
        let ground = { cell.convert(cell.reachedBounds, to: root.view) }
        XCTAssertEqual(cell.convert(cell.bounds, to: root.view).maxX, box.maxX, accuracy: 0.5, "the cell ends at the band's inner side")
        XCTAssertEqual(ground().maxX, root.view.bounds.maxX, accuracy: 0.5, "its ground runs once across the band to the screen's edge")
        // Its lines run on across the band, from UIKit's automatic trailing inset as the cell's edge
        let line = try list(p).separator(UIListSeparatorConfiguration(listAppearance: .plain), at: IndexPath(item: 1, section: 0))
        XCTAssertEqual(line.bottomSeparatorInsets.trailing, -band.right, accuracy: 0.5, "its line runs on across the band")
        let node = try XCTUnwrap(p.views[1])
        let move = { (width: CGFloat) in
            node.frame = CGRect(x: box.minX, y: box.minY, width: width, height: box.height)
            self.host(p).geometryChanged(1)
            root.view.layoutIfNeeded()
        }
        move(box.width - 40)
        XCTAssertEqual(cell.reachedBounds.maxX, cell.bounds.maxX, accuracy: 0.5, "a list off the band paints no strip there")
        move(box.width)
        XCTAssertEqual(ground().maxX, root.view.bounds.maxX, accuracy: 0.5, "back on the band, its ground reaches the edge again")
        // Mounted out of the window, the list finds no band; put back with no batch, it finds the band again
        p.viewport.removeFromSuperview()
        move(box.width)
        root.view.addSubview(p.viewport)
        root.view.layoutIfNeeded()
        let again = try XCTUnwrap(try self.cell(p, 10) as? GroupedCell)
        XCTAssertEqual(again.convert(again.reachedBounds, to: root.view).maxX, root.view.bounds.maxX, accuracy: 0.5, "in the window again, its ground reaches the edge")
        // Under an ancestor's perspective (CSS `perspective`), which can scale what it draws, no strip; without it, again
        var perspective = CATransform3DIdentity
        perspective.m34 = -1 / 500
        root.view.layer.sublayerTransform = perspective
        move(box.width)
        let deep = try XCTUnwrap(try self.cell(p, 10) as? GroupedCell)
        XCTAssertEqual(deep.reachedBounds.maxX, deep.bounds.maxX, accuracy: 0.5, "under a perspective, no strip")
        root.view.layer.sublayerTransform = CATransform3DIdentity
        move(box.width)
        let flat = try XCTUnwrap(try self.cell(p, 10) as? GroupedCell)
        XCTAssertEqual(flat.convert(flat.reachedBounds, to: root.view).maxX, root.view.bounds.maxX, accuracy: 0.5, "without it, the strip again")
    }

    /// An ancestor's mask counts for a list's side reach only where its coverage is known from the layer: an opaque
    /// plain layer covers its frame; a gradient, a shape, a transform or a fill that is not opaque may hide the band.
    func testAMaskCountsForTheSideReachOnlyByACoverageItsLayerShows() {
        let frame = CGRect(x: 0, y: 0, width: 100, height: 50)
        let layer = { (make: () -> CALayer, fill: UIColor?) -> CALayer in
            let mask = make()
            mask.frame = frame
            mask.backgroundColor = fill?.cgColor
            return mask
        }
        XCTAssertEqual(GroupedListView.maskCoverage(layer({ CALayer() }, .black)), frame, "an opaque plain layer covers its frame")
        XCTAssertNil(GroupedListView.maskCoverage(layer({ CAGradientLayer() }, .black)), "a gradient")
        XCTAssertNil(GroupedListView.maskCoverage(layer({ CAShapeLayer() }, .black)), "a shape")
        XCTAssertNil(GroupedListView.maskCoverage(layer({ CALayer() }, UIColor.black.withAlphaComponent(0.5))), "a fill not opaque")
        XCTAssertNil(GroupedListView.maskCoverage(layer({ CALayer() }, nil)), "no fill")
        let rounded = layer({ CALayer() }, .black)
        rounded.cornerRadius = 8
        XCTAssertNil(GroupedListView.maskCoverage(rounded), "rounded corners")
        let faded = layer({ CALayer() }, .black)
        faded.opacity = 0
        XCTAssertNil(GroupedListView.maskCoverage(faded), "no opacity")
        let turned = layer({ CALayer() }, .black)
        turned.transform = CATransform3DMakeRotation(.pi / 4, 0, 0, 1)
        XCTAssertNil(GroupedListView.maskCoverage(turned), "a transform, whose frame's corners it leaves clear")
    }

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
