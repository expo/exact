#if os(iOS)
@testable import ExactKit
import ExactCore
import UIKit
import XCTest

/// D14's one writer of a scroller's insets (LLP 1123): the kernel's terms, the authored rows and
/// UIKit's band and adjustment, written as one content inset and two indicator insets.
@MainActor
final class ScrollInsetsIOSTests: XCTestCase {
    private var window: UIWindow?
    /// The presenter of a test that has one, which its nodes hold weakly.
    private var presenter: Presenter?

    override func tearDown() {
        window = nil
        presenter = nil
        super.tearDown()
    }

    func testTheOccupiedDepthIsTheLargerTermAndThePlatformsAdjustmentIsLeftOut() {
        let band: [CGFloat] = [62, 0, 34, 0]
        let automatic = band, never: [CGFloat] = [0, 0, 0, 0]
        let rows = ScrollInsetsIOS.Rows()
        // Nothing reserved: UIKit's adjustment covers the band, and an adjustment past the band is
        // not taken back
        XCTAssertEqual(ScrollInsetsIOS.insets(terms: .init(), rows: rows, band: band, platform: automatic).content, .zero)
        XCTAssertEqual(ScrollInsetsIOS.insets(terms: .init(), rows: rows, band: band, platform: [82, 0, 34, 0]).content.top, 0)
        // A 50-point bar under the band: the writer adds the 50 UIKit leaves; under `never`, all 112
        let bar = ScrollInsetsIOS.Terms(reserved: [112, nil, nil, nil])
        XCTAssertEqual(ScrollInsetsIOS.insets(terms: bar, rows: rows, band: band, platform: automatic).content.top, 50)
        XCTAssertEqual(ScrollInsetsIOS.insets(terms: bar, rows: rows, band: band, platform: never).content.top, 112)
        // `reserved` replaces the band, unclamped: a negative inset's 27 under a 62-point band takes
        // 35 from UIKit's adjustment, and is written whole under `never`
        let reduced = ScrollInsetsIOS.Terms(reserved: [27, nil, nil, nil])
        XCTAssertEqual(ScrollInsetsIOS.insets(terms: reduced, rows: rows, band: band, platform: automatic).content.top, -35)
        XCTAssertEqual(ScrollInsetsIOS.insets(terms: reduced, rows: rows, band: band, platform: never).content.top, 27)
        // A band a scroller above took is no term, and UIKit's adjustment there is cancelled
        let under = ScrollInsetsIOS.Terms(bandReaches: [false, true, true, true])
        XCTAssertEqual(ScrollInsetsIOS.insets(terms: under, rows: rows, band: [30, 0, 34, 0], platform: [30, 0, 34, 0]).content.top, -30)
        // The keyboard covers the band rather than adding to it
        let keyboard = ScrollInsetsIOS.Terms(keyboard: [0, 0, 328, 0])
        XCTAssertEqual(ScrollInsetsIOS.insets(terms: keyboard, rows: rows, band: band, platform: automatic).content.bottom, 294)
        // Ignoring the container region on the top cancels UIKit's adjustment there
        let ignores = ScrollInsetsIOS.Terms(container: [false, true, true, true])
        XCTAssertEqual(ScrollInsetsIOS.insets(terms: ignores, rows: rows, band: band, platform: automatic).content.top, -62)
        let ignoresBar = ScrollInsetsIOS.Terms(reserved: [112, nil, nil, nil], container: [false, true, true, true])
        XCTAssertEqual(ScrollInsetsIOS.insets(terms: ignoresBar, rows: rows, band: band, platform: automatic).content.top, -62, "what it reserves is the container's too")
        // The authored and held terms add; a negative authored inset may take from the result
        var authored = rows
        authored.authored = [10, 0, -4, 0]
        let held = ScrollInsetsIOS.insets(terms: bar, rows: authored, held: [5, 0, 0, 0], band: band, platform: automatic)
        XCTAssertEqual([held.content.top, held.content.bottom], [65, -4])
        // Each edge is its own, the sides included
        let sides = ScrollInsetsIOS.insets(terms: .init(), rows: rows, band: [62, 21, 34, 44], platform: never)
        XCTAssertEqual(sides.content, UIEdgeInsets(top: 62, left: 44, bottom: 34, right: 21))
        // A native list's layout clears what it consumes: an 84-point side band it consumes whole is none of the
        // writer's, 20 reserved past it is, UIKit's adjustment over the consumed depth is cancelled, and the horizontal
        // indicator, which the layout does not move, still takes the occupied depth; the vertical one takes no side
        // band, lying at the scroll view's side as UIKit's own does (`SafeIndicatorTerm`)
        let side: [CGFloat] = [0, 84, 0, 0], consumed: [CGFloat] = [0, 84, 0, 0]
        XCTAssertEqual(ScrollInsetsIOS.insets(terms: .init(), rows: rows, band: side, platform: never, consumed: consumed).content.right, 0)
        let reserved = ScrollInsetsIOS.Terms(reserved: [nil, 104, nil, nil])
        XCTAssertEqual(ScrollInsetsIOS.insets(terms: reserved, rows: rows, band: side, platform: never, consumed: consumed).content.right, 20)
        let adjusted = ScrollInsetsIOS.insets(terms: .init(), rows: rows, band: side, platform: side, consumed: consumed)
        XCTAssertEqual([adjusted.content.right, adjusted.verticalIndicator.right, adjusted.horizontalIndicator.right], [-84, 0, 84])
        // On an edge whose region it ignores, the consumed depth is none of the writer's either way
        let ignored = ScrollInsetsIOS.Terms(container: [true, false, true, true])
        XCTAssertEqual(ScrollInsetsIOS.insets(terms: ignored, rows: rows, band: side, platform: never, consumed: consumed).content.right, 0)
    }

    func testTheIndicatorsTakeTheOccupiedDepthUnlessTheirAdjustmentIsNone() {
        var rows = ScrollInsetsIOS.Rows()
        rows.verticalIndicator = [2, 0, 0, 0]
        let bar = ScrollInsetsIOS.Terms(reserved: [112, nil, nil, nil])
        let follow = ScrollInsetsIOS.insets(terms: bar, rows: rows, band: [62, 0, 34, 0], platform: [62, 0, 34, 0])
        XCTAssertEqual([follow.verticalIndicator.top, follow.verticalIndicator.bottom, follow.horizontalIndicator.top], [114, 34, 112])
        rows.indicatorsFollow = false
        let none = ScrollInsetsIOS.insets(terms: bar, rows: rows, band: [62, 0, 34, 0], platform: [62, 0, 34, 0])
        XCTAssertEqual([none.verticalIndicator.top, none.verticalIndicator.bottom], [2, 0])
    }

    func testTheTermsAndRowsAreReadAsTheKernelAndTheHostGiveThem() throws {
        let json = #"{"id":4,"scrollTerms":{"reach":[47,0,34,0],"reserved":[97,null,74,null],"keyboard":[0,0,328,0],"bandReaches":[true,true,false,true],"container":[true,false,true,true]}}"#
        let reply = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(json.utf8)) as? [String: Any])
        let terms = try XCTUnwrap(ScrollInsetsIOS.Terms(reply: reply))
        XCTAssertEqual(terms, .init(reach: [47, 0, 34, 0], reserved: [97, nil, 74, nil], keyboard: [0, 0, 328, 0],
                                    bandReaches: [true, true, false, true], container: [true, false, true, true]))
        XCTAssertNil(ScrollInsetsIOS.Terms(reply: ["id": 3]))
        let style: NodeStyle = ["scroll_content_inset_top": .number(10), "scroll_content_inset_bottom": .number(-20),
                                "scroll_inset_adjustment": .string("never"), "scroll_indicator_adjustment": .string("none"),
                                "scroll_vertical_indicator_insets_right": .number(3), "scroll_horizontal_indicator_insets_left": .number(5)]
        let rows = ScrollInsetsIOS.Rows(style: style)
        XCTAssertEqual(rows.authored, [10, 0, -20, 0])
        XCTAssertEqual(rows.verticalIndicator, [0, 3, 0, 0])
        XCTAssertEqual(rows.horizontalIndicator, [0, 0, 0, 5])
        XCTAssertEqual(rows.adjustment, .never)
        XCTAssertFalse(rows.indicatorsFollow)
        XCTAssertEqual(ScrollInsetsIOS.Rows(style: [:]), ScrollInsetsIOS.Rows())
        XCTAssertEqual(ScrollInsetsIOS.Rows(style: ["scroll_inset_adjustment": .string("scrollable-axes")]).adjustment, .scrollableAxes)
        XCTAssertEqual(ScrollInsetsIOS.Rows(style: ["scroll_inset_adjustment": .string("always")]).adjustment, .always)
    }

    func testAWriteGivesUIKitsAdjustedInsetsAndKeepsTheScrollTop() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the band is a scene window's safe area") }
        let (scroll, owner) = try scrollView(in: scene)
        let band = owner.safeAreaInsets
        // A 50-point bar under the band: UIKit's adjusted inset is the bar's far side, under the
        // policy's adjustment or under `never`, the content at rest at its top either way
        let bar = ScrollInsetsIOS.Terms(reserved: [band.top + 50, nil, nil, nil])
        var indicated = ScrollInsetsIOS.Rows()
        indicated.horizontalIndicator = [0, 0, 7, 0]
        ScrollInsetsIOS.write(scroll, in: owner, terms: bar, rows: indicated)
        XCTAssertEqual(scroll.adjustedContentInset.top, band.top + 50)
        XCTAssertEqual(scroll.contentOffset.y, -(band.top + 50), "at rest")
        XCTAssertFalse(scroll.automaticallyAdjustsScrollIndicatorInsets)
        XCTAssertEqual(scroll.verticalScrollIndicatorInsets.top, band.top + 50)
        XCTAssertEqual([scroll.horizontalScrollIndicatorInsets.top, scroll.horizontalScrollIndicatorInsets.bottom], [band.top + 50, band.bottom + 7])
        var never = ScrollInsetsIOS.Rows()
        never.adjustment = .never
        ScrollInsetsIOS.write(scroll, in: owner, terms: bar, rows: never)
        XCTAssertEqual([scroll.contentInset.top, scroll.adjustedContentInset.top], [band.top + 50, band.top + 50])
        XCTAssertEqual(scroll.contentOffset.y + scroll.adjustedContentInset.top, 0, "still at rest")
        // Scrolled 500 into the content, removing the bar keeps the content where it was
        scroll.contentOffset.y = 500 - scroll.adjustedContentInset.top
        ScrollInsetsIOS.write(scroll, in: owner, terms: .init(), rows: never)
        scroll.layoutIfNeeded()
        XCTAssertEqual(scroll.adjustedContentInset.top, band.top)
        XCTAssertEqual(scroll.contentOffset.y + scroll.adjustedContentInset.top, 500)
        // A negative inset inside the root brings the adjusted inset under UIKit's band, to the
        // reduced depth the boxes beside the scroller observe, at least 0 as the kernel gives it
        let reduced = max(0, band.top - 20)
        ScrollInsetsIOS.write(scroll, in: owner, terms: .init(reserved: [reduced, nil, nil, nil]), rows: .init())
        XCTAssertEqual(scroll.adjustedContentInset.top, reduced)
        // Ignoring the container region on the top takes UIKit's adjustment away there
        ScrollInsetsIOS.write(scroll, in: owner, terms: .init(container: [false, true, true, true]), rows: .init())
        XCTAssertEqual(scroll.adjustedContentInset.top, 0)
        // The keyboard's 328 covers the bottom band
        ScrollInsetsIOS.write(scroll, in: owner, terms: .init(keyboard: [0, 0, 328, 0]), rows: .init())
        XCTAssertEqual(scroll.adjustedContentInset.bottom, max(328, band.bottom))
    }

    func testAScrollViewExactMadeNeverKeepsItsScrollTopAsUIKitBeginsToAdjust() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the band is a scene window's safe area") }
        // As Exact makes its scroll views, `never` with no inset, at rest and scrolled 500: the first
        // write turns UIKit's adjustment on, and the content stays where it was, after layout too
        for scrolled: CGFloat in [0, 500] {
            let (scroll, owner) = try scrollView(in: scene)
            scroll.contentInsetAdjustmentBehavior = .never
            scroll.contentOffset.y = scrolled
            ScrollInsetsIOS.write(scroll, in: owner, terms: .init(), rows: .init())
            scroll.layoutIfNeeded()
            XCTAssertEqual(scroll.contentOffset.y + scroll.adjustedContentInset.top, scrolled, accuracy: 0.5, "\(scrolled)")
            XCTAssertEqual(scroll.adjustedContentInset.top, owner.safeAreaInsets.top, "\(scrolled)")
        }
    }

    func testAWriteAfterTheBandOrTheContentMovesKeepsTheScrollTop() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the band is a scene window's safe area") }
        let (scroll, owner) = try scrollView(in: scene, content: 300)
        // Content shorter than the port, then grown past it, written each time
        ScrollInsetsIOS.write(scroll, in: owner, terms: .init(), rows: .init())
        scroll.contentSize.height = 5000
        ScrollInsetsIOS.write(scroll, in: owner, terms: .init(), rows: .init())
        scroll.layoutIfNeeded()
        XCTAssertEqual(scroll.contentOffset.y + scroll.adjustedContentInset.top, 0, accuracy: 0.5, "at rest as the content grows")
        XCTAssertEqual(scroll.adjustedContentInset.top, owner.safeAreaInsets.top, "the band once, as the content grows")
        // The owner's band grows by 30 before a layout, and the writer writes before one
        scroll.contentOffset.y = 500 - scroll.adjustedContentInset.top
        window?.rootViewController?.additionalSafeAreaInsets.top = 30
        ScrollInsetsIOS.write(scroll, in: owner, terms: .init(), rows: .init())
        scroll.layoutIfNeeded()
        XCTAssertEqual(scroll.contentOffset.y + scroll.adjustedContentInset.top, 500, accuracy: 0.5, "scrolled as the band grows")
        XCTAssertEqual(scroll.adjustedContentInset.top, owner.safeAreaInsets.top)
        // At rest under the grown band, the band shrinks back before a layout
        scroll.contentOffset.y = -scroll.adjustedContentInset.top
        window?.rootViewController?.additionalSafeAreaInsets.top = 0
        ScrollInsetsIOS.write(scroll, in: owner, terms: .init(), rows: .init())
        scroll.layoutIfNeeded()
        XCTAssertEqual(scroll.contentOffset.y + scroll.adjustedContentInset.top, 0, accuracy: 0.5, "at rest as the band shrinks")
    }

    func testAWriteAsTheBandAndATermMoveTogetherKeepsTheScrollTop() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the band is a scene window's safe area") }
        // At rest and scrolled 500, the owner's band grows by 30 and a 50-point bar arrives under
        // it, in one write before a layout
        for scrolled: CGFloat in [0, 500] {
            let (scroll, owner) = try scrollView(in: scene)
            ScrollInsetsIOS.write(scroll, in: owner, terms: .init(), rows: .init())
            scroll.contentOffset.y = scrolled - scroll.adjustedContentInset.top
            let band = owner.safeAreaInsets.top + 30
            window?.rootViewController?.additionalSafeAreaInsets.top = 30
            ScrollInsetsIOS.write(scroll, in: owner, terms: .init(reserved: [band + 50, nil, nil, nil]), rows: .init())
            scroll.layoutIfNeeded()
            XCTAssertEqual(scroll.adjustedContentInset.top, band + 50, "\(scrolled)")
            XCTAssertEqual(scroll.contentOffset.y + scroll.adjustedContentInset.top, scrolled, accuracy: 0.5, "\(scrolled)")
        }
    }

    func testTheKeptOffsetStaysInTheRange() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the band is a scene window's safe area") }
        let (scroll, owner) = try scrollView(in: scene)
        // At the end under the keyboard's 328, the keyboard goes: the content's end is the port's
        let keyboard = ScrollInsetsIOS.Terms(keyboard: [0, 0, 328, 0])
        ScrollInsetsIOS.write(scroll, in: owner, terms: keyboard, rows: .init())
        let end = { scroll.contentSize.height + scroll.adjustedContentInset.bottom - scroll.bounds.height }
        scroll.contentOffset.y = end()
        ScrollInsetsIOS.write(scroll, in: owner, terms: .init(), rows: .init())
        XCTAssertEqual(scroll.contentOffset.y, end(), accuracy: 0.5)
        // While the reader drags, the offset past the new range is UIKit's to bring back
        let (dragged, draggedOwner) = try scrollView(in: scene, Dragged())
        ScrollInsetsIOS.write(dragged, in: draggedOwner, terms: keyboard, rows: .init())
        dragged.contentOffset.y = dragged.contentSize.height + dragged.adjustedContentInset.bottom - dragged.bounds.height
        let held = dragged.contentOffset.y
        ScrollInsetsIOS.write(dragged, in: draggedOwner, terms: .init(), rows: .init())
        XCTAssertEqual(dragged.contentOffset.y, held, accuracy: 0.5)
    }

    func testTheSidesTakeTheOwnersSideBands() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the band is a scene window's safe area") }
        // The owner's view adds 44 on the left and 21 on the right: under `never` the writer writes
        // them, under `always` UIKit applies them; the adjusted insets agree
        let (scroll, owner) = try scrollView(in: scene)
        window?.rootViewController?.additionalSafeAreaInsets = UIEdgeInsets(top: 0, left: 44, bottom: 0, right: 21)
        owner.layoutIfNeeded()
        for adjustment in [UIScrollView.ContentInsetAdjustmentBehavior.never, .always] {
            var rows = ScrollInsetsIOS.Rows()
            rows.adjustment = adjustment
            ScrollInsetsIOS.write(scroll, in: owner, terms: .init(), rows: rows)
            XCTAssertEqual(scroll.adjustedContentInset.left, owner.safeAreaInsets.left, "\(adjustment.rawValue)")
            XCTAssertEqual(scroll.adjustedContentInset.right, owner.safeAreaInsets.right, "\(adjustment.rawValue)")
        }
    }

    func testAWriteWithinHalfAPixelLeavesTheScrollerAlone() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the band is a scene window's safe area") }
        let (scroll, owner) = try scrollView(in: scene)
        let bar = ScrollInsetsIOS.Terms(reserved: [owner.safeAreaInsets.top + 50, nil, nil, nil])
        ScrollInsetsIOS.write(scroll, in: owner, terms: bar, rows: .init())
        scroll.contentOffset.y = 300
        let written = scroll.contentInset
        ScrollInsetsIOS.write(scroll, in: owner, terms: .init(reserved: [owner.safeAreaInsets.top + 50.1, nil, nil, nil]), rows: .init())
        XCTAssertEqual(scroll.contentInset, written)
        XCTAssertEqual(scroll.contentOffset.y, 300)
    }

    func testAnOutsetScrollViewsBandIsReadFromItsOwnersSafeArea() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the band is a scene window's safe area") }
        // A box from the top band's inner side, or 100 below it, to the bottom band's, holding a
        // scroll view outset over the whole window, as reach outsets one: UIKit gives the scroll
        // view no band past its box, though it overlaps both bands, so the writer reads the overlap
        // from the owner's view, writes the bands, *P* being 0 there, and the adjusted insets are
        // the bands
        let (_, owner) = try scrollView(in: scene)
        let band = owner.safeAreaInsets
        XCTAssertGreaterThan(band.bottom, 0, "the device has a bottom band to outset into")
        for below: CGFloat in [0, 100] {
            let box = UIView(frame: CGRect(x: 0, y: band.top + below, width: owner.bounds.width,
                                           height: owner.bounds.height - band.top - below - band.bottom))
            owner.addSubview(box)
            let scroll = UIScrollView(frame: CGRect(x: 0, y: -(band.top + below), width: owner.bounds.width, height: owner.bounds.height))
            scroll.contentSize = CGSize(width: owner.bounds.width, height: 5000)
            box.addSubview(scroll)
            owner.layoutIfNeeded()
            XCTAssertEqual([scroll.safeAreaInsets.top, scroll.safeAreaInsets.bottom], [0, 0], "UIKit's, \(below) below")
            let read = ScrollInsetsIOS.band(of: scroll, in: owner)
            XCTAssertEqual([read[0], read[2]], [band.top, band.bottom], "\(below) below")
            ScrollInsetsIOS.write(scroll, in: owner, terms: .init(), rows: .init())
            XCTAssertEqual([scroll.contentInset.top, scroll.contentInset.bottom], [band.top, band.bottom], "\(below) below")
            XCTAssertEqual([scroll.adjustedContentInset.top, scroll.adjustedContentInset.bottom], [band.top, band.bottom], "\(below) below")
            box.removeFromSuperview()
        }
        // One that starts 20 above the owner's view reads the band's depth, no more
        let above = UIScrollView(frame: CGRect(x: 0, y: -20, width: owner.bounds.width, height: 300))
        owner.addSubview(above)
        XCTAssertEqual(ScrollInsetsIOS.band(of: above, in: owner)[0], band.top)
    }

    func testAnOutsetScrollViewsSideBandIsReadFromItsOwnersSafeArea() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the band is a scene window's safe area") }
        // A box between the side bands' inner sides holding a scroll view outset over the window's
        // width, on a device with a side band: UIKit gives it none of the band past its box there
        // either, and the writer reads the overlap from the owner's view
        let (_, owner) = try scrollView(in: scene)
        let band = owner.safeAreaInsets
        try XCTSkipIf(band.left + band.right < 1, "no side band on this device")
        let box = UIView(frame: CGRect(x: band.left, y: 0, width: owner.bounds.width - band.left - band.right, height: owner.bounds.height))
        owner.addSubview(box)
        let scroll = UIScrollView(frame: CGRect(x: -band.left, y: 0, width: owner.bounds.width, height: owner.bounds.height))
        scroll.contentSize = CGSize(width: owner.bounds.width, height: 5000)
        box.addSubview(scroll)
        owner.layoutIfNeeded()
        XCTAssertEqual([scroll.safeAreaInsets.left, scroll.safeAreaInsets.right], [0, 0], "UIKit's")
        let read = ScrollInsetsIOS.band(of: scroll, in: owner)
        XCTAssertEqual([read[3], read[1]], [band.left, band.right])
        ScrollInsetsIOS.write(scroll, in: owner, terms: .init(), rows: .init())
        XCTAssertEqual([scroll.adjustedContentInset.left, scroll.adjustedContentInset.right], [band.left, band.right])
    }

    func testAScrollerInAnotherTakesNoneOfTheBandThatOneTook() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the band is a scene window's safe area") }
        // An inner scroll view at the top of an outer one's content, the outer scrolled 30 under
        // the band: the band does not reach the inner (`bandReaches`), so it takes none, whatever
        // UIKit or the overlap would give it
        let (outer, owner) = try scrollView(in: scene)
        ScrollInsetsIOS.write(outer, in: owner, terms: .init(), rows: .init())
        let inner = UIScrollView(frame: CGRect(x: 0, y: 0, width: outer.bounds.width, height: 200))
        inner.contentSize = CGSize(width: outer.bounds.width, height: 2000)
        outer.addSubview(inner)
        outer.contentOffset.y = 30 - outer.adjustedContentInset.top
        owner.layoutIfNeeded()
        ScrollInsetsIOS.write(inner, in: owner, terms: .init(bandReaches: [false, true, true, true]), rows: .init())
        XCTAssertEqual(inner.adjustedContentInset.top, 0)
    }

    func testAFollowerAtItsEndKeepsItAsTheWriterMovesItsInsets() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the band is a scene window's safe area") }
        // A list following its end, at its end as Exact made it, `never` with no inset: the first
        // write turns UIKit's adjustment on, and a later one raises a 50-point bar under the band
        // and the keyboard's 328 at once; it stays at its end through each
        let (scroll, owner) = try follower(in: scene)
        XCTAssertEqual(scroll.contentInsetAdjustmentBehavior, .never)
        let end = { scroll.end(under: scroll.adjustedContentInset) }
        scroll.contentOffset.y = end()
        ScrollInsetsIOS.write(scroll, in: owner, terms: .init(), rows: .init())
        scroll.layoutIfNeeded()
        XCTAssertNotEqual(scroll.adjustedContentInset, .zero, "UIKit's adjustment began")
        XCTAssertEqual([scroll.contentInset.top, scroll.contentInset.bottom], [0, 0], "UIKit's adjustment covers the scrolling axis's bands")
        XCTAssertEqual(scroll.contentOffset.y, end(), accuracy: 0.5, "as UIKit's adjustment begins")
        let bar = ScrollInsetsIOS.Terms(reserved: [owner.safeAreaInsets.top + 50, nil, nil, nil], keyboard: [0, 0, 328, 0])
        ScrollInsetsIOS.write(scroll, in: owner, terms: bar, rows: .init())
        scroll.layoutIfNeeded()
        XCTAssertEqual(scroll.contentOffset.y, end(), accuracy: 0.5, "as a bar and the keyboard rise")
    }

    func testAFollowerAtItsEndKeepsItAsItsOwnersBandGrows() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the band is a scene window's safe area") }
        // The owner's band grows by 30: the writer writes before a layout, as a commit can, then
        // UIKit's layout gives the list the grown band and the writer writes after it, as activation
        // does after each layout; the list is at its end, UIKit's adjustment holding the band
        let (scroll, owner) = try follower(in: scene)
        ScrollInsetsIOS.write(scroll, in: owner, terms: .init(), rows: .init())
        scroll.contentOffset.y = scroll.end(under: scroll.adjustedContentInset)
        window?.rootViewController?.additionalSafeAreaInsets.top = 30
        ScrollInsetsIOS.write(scroll, in: owner, terms: .init(), rows: .init())
        owner.layoutIfNeeded()
        ScrollInsetsIOS.write(scroll, in: owner, terms: .init(), rows: .init())
        XCTAssertEqual([scroll.adjustedContentInset.top, scroll.contentInset.top], [owner.safeAreaInsets.top, 0], "UIKit's adjustment holds the grown band")
        XCTAssertEqual(scroll.contentOffset.y, scroll.end(under: scroll.adjustedContentInset), accuracy: 0.5)
    }

    func testAFollowerShortOfItsEndKeepsItsScrollTop() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the band is a scene window's safe area") }
        // Under a 50-point bar, 40 above its end: one write drops the bar and raises the keyboard's
        // 328, and the list keeps its place, its own reaction leaving the writer's offset alone
        let (scroll, owner) = try follower(in: scene)
        ScrollInsetsIOS.write(scroll, in: owner, terms: .init(reserved: [owner.safeAreaInsets.top + 50, nil, nil, nil]), rows: .init())
        scroll.contentOffset.y = scroll.end(under: scroll.adjustedContentInset) - 40
        let top = scroll.contentOffset.y + scroll.adjustedContentInset.top
        ScrollInsetsIOS.write(scroll, in: owner, terms: .init(keyboard: [0, 0, 328, 0]), rows: .init())
        scroll.layoutIfNeeded()
        XCTAssertEqual(scroll.contentOffset.y + scroll.adjustedContentInset.top, top, accuracy: 0.5)
    }

    func testTheReactionsToAWriteSeeOnlyItsSettledInsets() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the band is a scene window's safe area") }
        // From `never` with a bar, the policy and the inset change in one write: a reaction held by
        // `SafeAreaSteps` runs once, after both
        let reacting = Reacting()
        let (scroll, owner) = try scrollView(in: scene, reacting)
        scroll.contentInsetAdjustmentBehavior = .never
        reacting.seen = []
        ScrollInsetsIOS.write(scroll, in: owner, terms: .init(reserved: [owner.safeAreaInsets.top + 50, nil, nil, nil]), rows: .init())
        XCTAssertEqual(reacting.seen, [scroll.adjustedContentInset])
    }

    /// A scroll view's indicators take its own safe area while UIKit's adjustment, which adds the top and bottom
    /// itself, is off: the top and bottom, and the horizontal indicator its sides. The vertical one takes no side:
    /// UIKit's adjustment adds the side safe area again to a side inset written, and with none the vertical indicator
    /// lies where UIKit's own scroll view's does, the adjustment on or off, outside the layout margins and never over
    /// content that keeps to them. They follow the safe area, and D14's writer, which writes them whole, takes none.
    func testAScrollViewsIndicatorsTakeTheSafeAreaUIKitsAdjustmentLeavesOut() throws {
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 466, height: 678)), root = UIViewController()
        window.rootViewController = root
        window.isHidden = false
        defer { window.isHidden = true }
        let strip = UIEdgeInsets(top: 20, left: 0, bottom: 34, right: 84)
        root.additionalSafeAreaInsets = strip
        // As Exact's own scrollers are, UIKit adjusting no content inset (`NodeView`), and beside it UIKit's own
        let scroll = ScrollView(frame: root.view.bounds), uikit = UIScrollView(frame: root.view.bounds)
        for s in [scroll, uikit] {
            s.contentInsetAdjustmentBehavior = .never
            s.contentSize = CGSize(width: root.view.bounds.width, height: 3000)
            root.view.addSubview(s)
        }
        window.layoutIfNeeded()
        let safe = scroll.safeAreaInsets
        XCTAssertGreaterThanOrEqual(safe.right, 84)
        XCTAssertEqual(scroll.verticalScrollIndicatorInsets, .zero, "UIKit adds the top and bottom itself")
        XCTAssertEqual(scroll.horizontalScrollIndicatorInsets, .zero)
        /// The vertical indicator as UIKit draws it, shown, in the root view.
        func vertical(_ s: UIScrollView) throws -> CGRect {
            s.flashScrollIndicators()
            window.layoutIfNeeded()
            let bars = s.subviews.filter { String(describing: type(of: $0)) == "_UIScrollViewScrollIndicator" }
            for bar in bars { bar.layer.removeAllAnimations(); bar.alpha = 1 }
            return try XCTUnwrap(bars.map { root.view.convert($0.bounds, from: $0) }.first { $0.height > $0.width }, "\(s.subviews)")
        }
        let margin = root.view.bounds.width - root.view.layoutMargins.right
        for adjusted in [true, false] {
            for s in [scroll, uikit] { s.automaticallyAdjustsScrollIndicatorInsets = adjusted }
            let exact = try vertical(scroll), own = try vertical(uikit)
            XCTAssertEqual(exact.minX, own.minX, accuracy: 0.5, "where UIKit's own lies, adjustment \(adjusted): \(exact), \(own)")
            XCTAssertGreaterThanOrEqual(exact.minX, margin, "outside the layout margins, adjustment \(adjusted): \(exact)")
        }
        uikit.removeFromSuperview()
        XCTAssertEqual(scroll.verticalScrollIndicatorInsets, UIEdgeInsets(top: safe.top, left: 0, bottom: safe.bottom, right: 0),
                       "with UIKit's adjustment off, the safe area's top and bottom")
        XCTAssertEqual(scroll.horizontalScrollIndicatorInsets, safe, "and the horizontal track keeps within its sides")
        root.additionalSafeAreaInsets = .zero
        window.layoutIfNeeded()
        XCTAssertEqual(scroll.horizontalScrollIndicatorInsets, scroll.safeAreaInsets, "it follows the safe area")
        root.additionalSafeAreaInsets = strip
        window.layoutIfNeeded()
        scroll.writtenWhole = true
        XCTAssertEqual(scroll.verticalScrollIndicatorInsets, .zero, "D14's writer writes them whole")
        XCTAssertEqual(scroll.horizontalScrollIndicatorInsets, .zero)
    }

    func testAWriteOutOfAWindowWritesNothing() {
        let owner = UIView(frame: CGRect(x: 0, y: 0, width: 300, height: 600))
        let scroll = UIScrollView(frame: owner.bounds)
        owner.addSubview(scroll)
        var never = ScrollInsetsIOS.Rows()
        never.adjustment = .never
        ScrollInsetsIOS.write(scroll, in: owner, terms: .init(reserved: [112, nil, nil, nil]), rows: never)
        XCTAssertEqual(scroll.contentInsetAdjustmentBehavior, .automatic)
        XCTAssertEqual(scroll.contentInset, .zero)
        XCTAssertTrue(scroll.automaticallyAdjustsScrollIndicatorInsets)
    }

    /// An Exact list following its end, filling the root view of a window of `scene`, 3000 tall,
    /// and the root view, the owner's view here.
    private func follower(in scene: UIWindowScene) throws -> (ScrollView, UIView) {
        let p = Presenter()
        presenter = p
        let (_, owner) = try scrollView(in: scene)
        p.viewport.frame = owner.bounds
        owner.addSubview(p.viewport)
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "props": ["scrollFollowEnd": "true"], "style": ["overflow_y": "scroll"]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": owner.bounds.width, "h": owner.bounds.height],
            ["op": "content", "id": 1, "w": owner.bounds.width, "h": 3000],
        ]))
        owner.layoutIfNeeded()
        let scroll = try XCTUnwrap(p.views[1]?.scroll as? ScrollView)
        XCTAssertEqual(scroll.contentSize.height, 3000, accuracy: 1)
        return (scroll, owner)
    }

    /// `scroll` filling a window of the test host's `scene`, the scene's size, under the window's
    /// safe area, with content `content` tall, and the root view, the owner's view here.
    private func scrollView(in scene: UIWindowScene, content: CGFloat = 5000, _ scroll: UIScrollView = UIScrollView()) throws -> (UIScrollView, UIView) {
        let window = UIWindow(windowScene: scene)
        window.frame = scene.coordinateSpace.bounds
        let root = UIViewController()
        window.rootViewController = root
        window.makeKeyAndVisible()
        self.window = window
        scroll.frame = root.view.bounds
        scroll.contentSize = CGSize(width: root.view.bounds.width, height: content)
        root.view.addSubview(scroll)
        root.view.layoutIfNeeded()
        return (scroll, root.view)
    }
}

/// A scroll view that records the adjusted insets its reactions see, held as Exact's are.
private final class Reacting: UIScrollView {
    var seen: [UIEdgeInsets] = []
    override func adjustedContentInsetDidChange() {
        super.adjustedContentInsetDidChange()
        SafeAreaSteps.react(self) { [unowned self] in seen.append(adjustedContentInset) }
    }
}

/// A scroll view the reader is dragging.
private final class Dragged: UIScrollView {
    override var isTracking: Bool { true }
    override var isDragging: Bool { true }
}
#endif
