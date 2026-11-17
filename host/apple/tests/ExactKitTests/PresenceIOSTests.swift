#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// The presence ops on UIKit (LLP 1063): `exit` keeps a leaving view where
/// it was, above its old siblings, inert and out of every lookup, while the
/// engine's values still reach it, until the host's `destroy` ends it; a
/// `layout` present places the box outermost, from its top-left corner.
///   bun host/apple/build.mjs --test --ios
final class PresenceIOSTests: XCTestCase {
    private var window: UIWindow!

    /// 1 → [2 → [3], 4], each row 50 points tall.
    private func fixture() -> Presenter {
        let p = Presenter()
        window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 400))
        p.viewport.frame = window.bounds
        window.addSubview(p.viewport)
        window.makeKeyAndVisible()
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "view"],
            // Named, so they are views and not flat leaves (LLP 1068 §6.1).
            ["op": "create", "id": 3, "kind": "view", "props": ["testId": "inner"]],
            ["op": "create", "id": 4, "kind": "view", "props": ["testId": "sibling"]],
            ["op": "children", "id": 1, "ids": [2, 4]],
            ["op": "children", "id": 2, "ids": [3]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 400.0, "h": 400.0],
            ["op": "frame", "id": 2, "x": 0.0, "y": 0.0, "w": 400.0, "h": 50.0],
            ["op": "frame", "id": 3, "x": 0.0, "y": 0.0, "w": 100.0, "h": 50.0],
            ["op": "frame", "id": 4, "x": 0.0, "y": 50.0, "w": 400.0, "h": 50.0],
        ]))
        return p
    }

    private func tabBarFixture() -> Presenter {
        let p = fixture()
        p.apply(tabBar(y: 0, height: 20))
        return p
    }

    /// A tablist of two symbol-over-label tabs, a root at `y`, `height` tall.
    private func tabBar(y: Double, height: Double) -> Batch {
        wireBatch([
            ["op": "create", "id": 10, "kind": "view", "props": ["accessibilityRole": "tablist"]],
            ["op": "create", "id": 11, "kind": "button", "props": ["accessibilityRole": "tab"], "handlers": ["press"]],
            ["op": "create", "id": 12, "kind": "button", "props": ["accessibilityRole": "tab"], "handlers": ["press"]],
            ["op": "create", "id": 13, "kind": "image", "props": ["imageSource": "symbol:sf/house", "symbolName": "house"]],
            ["op": "create", "id": 14, "kind": "text", "props": ["text": "Home"]],
            ["op": "create", "id": 15, "kind": "image", "props": ["imageSource": "symbol:sf/star", "symbolName": "star"]],
            ["op": "create", "id": 16, "kind": "text", "props": ["text": "Saved"]],
            ["op": "children", "id": 11, "ids": [13, 14]],
            ["op": "children", "id": 12, "ids": [15, 16]],
            ["op": "children", "id": 10, "ids": [11, 12]],
            ["op": "roots", "ids": [1, 10]],
            ["op": "frame", "id": 10, "x": 0.0, "y": y, "w": 400.0, "h": height],
        ])
    }

    private func drainIntrinsicSizes() {
        let delivered = expectation(description: "intrinsic size delivered after the batch")
        DispatchQueue.main.async { delivered.fulfill() }
        wait(for: [delivered], timeout: 2)
    }

    func testARIAUnavailableProjectedTabsStayEnabledForInput() throws {
        let p = tabBarFixture()
        defer { p.reset(); window.isHidden = true }
        p.apply(wireBatch([["op": "props", "id": 11, "set": ["accessibilityDisabled": "true"]]]))
        let owner = try XCTUnwrap(p.views[10])
        let bar = try XCTUnwrap(owner.subviews.compactMap { $0 as? UITabBar }.first)
        let item = try XCTUnwrap(bar.items?.first)
        XCTAssertTrue(item.isEnabled)
        XCTAssertTrue(item.accessibilityTraits.contains(.notEnabled), "the projected item speaks as disabled")

        p.apply(wireBatch([
            ["op": "children", "id": 11, "ids": [14]],
            ["op": "children", "id": 12, "ids": [16]],
        ]))
        let control = try XCTUnwrap(p.segments.control(of: 10) as? LabelledSegmentedControl)
        control.layoutIfNeeded()
        XCTAssertTrue(control.isEnabledForSegment(at: 0))
        XCTAssertTrue(try XCTUnwrap(control.segmentViews.first).accessibilityTraits.contains(.notEnabled), "the projected segment speaks as disabled")
        p.apply(wireBatch([["op": "props", "id": 11, "set": ["accessibilityDisabled": "false"]]]))
        control.layoutIfNeeded()
        XCTAssertFalse(try XCTUnwrap(control.segmentViews.first).accessibilityTraits.contains(.notEnabled))
    }

    func testAnUnstyledTabSymbolLeavesTheBarsNativeTint() throws {
        let p = tabBarFixture()
        defer { p.reset(); window.isHidden = true }
        p.apply(wireBatch([["op": "props", "id": 11, "set": ["accessibilitySelected": "true"]]]))
        let bar = try XCTUnwrap(p.views[10]?.subviews.first { $0 is UITabBar } as? UITabBar)
        let native = UITabBar()
        window.addSubview(native)
        XCTAssertEqual(bar.tintColor, native.tintColor)
        let pink: [Double] = [240, 40, 120, 255]
        p.apply(wireBatch([["op": "style", "id": 13, "style": ["text_color": pink]]]))
        XCTAssertEqual(bar.tintColor, native.tintColor, "inherited page ink leaves UIKit's accent")
        p.apply(wireBatch([["op": "style", "id": 13, "style": ["text_color": pink, "written": ["text_color"]]]]))
        XCTAssertEqual(bar.tintColor?.cgColor.components, TextEngine.color(pink).cgColor.components)
        p.apply(wireBatch([["op": "style", "id": 13, "style": ["text_color": pink]]]))
        XCTAssertEqual(bar.tintColor, native.tintColor, "removing authored ink restores UIKit's accent")
    }

    func testTabBarReportsItsHeightAndAlwaysFillsTheKernelBox() throws {
        let p = tabBarFixture()
        var reports: [CGSize?] = []
        p.onIntrinsic = { sizes in
            for (id, size) in sizes where id == 10 { reports.append(size) }
        }
        let owner = try XCTUnwrap(p.views[10])
        let bar = try XCTUnwrap(owner.subviews.first { $0 is UITabBar })
        XCTAssertEqual(bar.frame, owner.bounds, "the native bar never escapes the kernel box")
        drainIntrinsicSizes()
        let size = try XCTUnwrap(reports.last ?? nil)
        XCTAssertGreaterThan(size.height, 20)
        p.apply(wireBatch([["op": "frame", "id": 10, "x": 0.0, "y": 0.0,
                            "w": Double(size.width), "h": Double(size.height)]]))
        XCTAssertEqual(bar.frame, owner.bounds)
        drainIntrinsicSizes()
        XCTAssertEqual(reports.count, 1, "applying the measured box does not remeasure forever")
        // Its width is its narrowest, none (LLP 1116.004 D2): a narrower box
        // keeps the one report.
        XCTAssertEqual(size.width, 1)
        p.apply(wireBatch([["op": "frame", "id": 10, "x": 0.0, "y": 0.0,
                            "w": 300.0, "h": Double(size.height)]]))
        drainIntrinsicSizes()
        XCTAssertEqual(reports.count, 1, "the bar's narrowest does not follow its box")
        XCTAssertEqual(bar.frame, owner.bounds)
        p.apply(wireBatch([["op": "props", "id": 10,
                            "set": ["accessibilityRole": "tablist", "accessibilityOrientation": "vertical"]]]))
        drainIntrinsicSizes()
        XCTAssertEqual(reports.count, 2)
        XCTAssertNil(reports.last ?? nil, "leaving the projection clears its native minimum")
        XCTAssertNil(bar.superview)
        XCTAssertFalse(try XCTUnwrap(p.views[11]).isHidden)
        p.reset()
    }

    /// A tablist padded at a side keeps its bar inside the padding, the
    /// bar's edges on the device's pixel grid (LLP 1059 D2): duo-lab's tabs
    /// on a book fold's trailing segment, from 495.5 points.
    func testTabBarSpansAPaddedTablistsContentBoxOnThePixelGrid() throws {
        let p = tabBarFixture()
        defer { p.reset() }
        p.apply(wireBatch([["op": "frame", "id": 10, "x": 0.0, "y": 0.0, "w": 951.0, "h": 83.0],
                           ["op": "fieldContent", "id": 10, "rect": [495.5, 0.0, 455.5, 83.0], "padding": [0.0, 0.0, 0.0, 495.5]]]))
        let owner = try XCTUnwrap(p.views[10])
        let bar = try XCTUnwrap(owner.subviews.first { $0 is UITabBar })
        XCTAssertEqual(bar.frame.minX, PixelGrid.round(495.5, scale: p.gridScale), accuracy: 0.0001, "the left edge on the grid, inside the padding")
        XCTAssertEqual(bar.frame.maxX, owner.bounds.maxX, accuracy: 0.0001)
        XCTAssertEqual(bar.frame.height, owner.bounds.height)
        p.apply(wireBatch([["op": "fieldContent", "id": 10]]))
        XCTAssertEqual(bar.frame, owner.bounds, "unpadded, the bar fills the box again")
    }

    /// A bar first laid out inside the home indicator's inset, before its box
    /// took its measured height, has its labels sized again as it grows:
    /// UIKit sizes them in the room the bar has then (duo-lab under the
    /// agent's chrome). The selected tab's glyph is its symbol filled.
    func testATabBarFirstLaidOutInTheBottomInsetSizesItsLabelsAgainAsItGrows() throws {
        let p = fixture()
        defer { p.reset(); window.isHidden = true }
        let host = UIViewController()
        host.additionalSafeAreaInsets.bottom = 34
        window.rootViewController = host
        host.view.addSubview(p.viewport)
        p.apply(tabBar(y: 366, height: 34))
        p.apply(wireBatch([["op": "props", "id": 11, "set": ["accessibilitySelected": "true"]]]))
        let bar = try XCTUnwrap(p.views[10]?.subviews.first { $0 is UITabBar } as? UITabBar)
        window.layoutIfNeeded()
        XCTAssertEqual(bar.safeAreaInsets.bottom, host.view.safeAreaInsets.bottom, "laid out inside the inset, including the device's own safe area")
        let height = bar.sizeThatFits(CGSize(width: 400, height: 0)).height
        p.apply(wireBatch([["op": "frame", "id": 10, "x": 0.0, "y": 400 - Double(height), "w": 400.0, "h": Double(height)]]))
        window.layoutIfNeeded()
        drainIntrinsicSizes()
        window.layoutIfNeeded()
        func labels(_ view: UIView) -> [UILabel] { view.subviews.flatMap { ($0 as? UILabel).map { [$0] } ?? labels($0) } }
        XCTAssertFalse(labels(bar).isEmpty)
        for label in labels(bar) {
            XCTAssertGreaterThanOrEqual(label.bounds.height, label.intrinsicContentSize.height - 0.5, "\(label.text ?? "") is cut short")
        }
        XCTAssertEqual(bar.items?.first?.selectedImage?.pngData(), UIImage(systemName: "house.fill")?.pngData())
        // A symbol written filled stays filled when unselected.
        p.apply(wireBatch([["op": "props", "id": 15, "set": ["imageSource": "symbol:sf/star.fill", "symbolName": "star.fill"]]]))
        drainIntrinsicSizes()
        XCTAssertEqual(bar.items?.last?.image?.pngData(), UIImage(systemName: "star.fill")?.pngData())
    }

    /// Measured again when what sizes it changes: a segment's title, the
    /// text size, a hatch's background image; not otherwise.
    func testASegmentedTablistIsMeasuredAgainWhenWhatSizesItChanges() throws {
        let p = tabBarFixture()
        defer { p.reset() }
        var reports: [CGSize?] = []
        p.onIntrinsic = { sizes in
            for (id, size) in sizes where id == 10 { reports.append(size) }
        }
        p.apply(wireBatch([
            ["op": "children", "id": 11, "ids": [14]],
            ["op": "children", "id": 12, "ids": [16]],
        ]))
        let control = try XCTUnwrap(p.segments.control(of: 10))
        drainIntrinsicSizes()
        XCTAssertEqual(reports.last ?? nil, CGSize(width: 1, height: control.intrinsicContentSize.height))
        // A hatch's background image, taller than the control.
        let tall = UIGraphicsImageRenderer(size: CGSize(width: 4, height: 60)).image { _ in }
        control.setBackgroundImage(tall, for: .normal, barMetrics: .default)
        p.segments.sync()
        drainIntrinsicSizes()
        XCTAssertEqual((reports.last ?? nil)?.height, control.intrinsicContentSize.height, "a hatch's look is measured")
        control.setBackgroundImage(nil, for: .normal, barMetrics: .default)
        p.segments.sync()
        drainIntrinsicSizes()
        XCTAssertEqual((reports.last ?? nil)?.height, control.intrinsicContentSize.height, "and again when it goes")
        // A larger text size.
        control.traitOverrides.preferredContentSizeCategory = .accessibilityExtraExtraExtraLarge
        control.updateTraitsIfNeeded()
        p.segments.sync()
        drainIntrinsicSizes()
        XCTAssertEqual((reports.last ?? nil)?.height, control.intrinsicContentSize.height, "the text size is in what it is measured from")
    }

    func testSegmentsReportNativeHeightWithoutRepeatedOrStaleMeasurements() throws {
        let p = tabBarFixture()
        defer { p.reset() }
        var reports: [CGSize?] = []
        p.onIntrinsic = { sizes in
            for (id, size) in sizes where id == 10 { reports.append(size) }
        }
        p.apply(wireBatch([
            ["op": "children", "id": 11, "ids": [14]],
            ["op": "children", "id": 12, "ids": [16]],
        ]))
        let owner = try XCTUnwrap(p.views[10])
        let control = try XCTUnwrap(p.segments.control(of: 10))
        drainIntrinsicSizes()
        let size = try XCTUnwrap(reports.last ?? nil)
        XCTAssertEqual(size, CGSize(width: 1, height: control.intrinsicContentSize.height),
                       "the control's own height, not the box's; its narrowest none (LLP 1116.004 D2)")
        XCTAssertGreaterThan(size.height, 20, "an unsized text row must not squash the native control")
        XCTAssertEqual(control.frame, owner.contentBox(), "the kernel still owns the final box")
        p.apply(wireBatch([["op": "frame", "id": 10, "x": 0.0, "y": 0.0,
                            "w": 400.0, "h": Double(size.height)]]))
        drainIntrinsicSizes()
        let count = reports.count
        p.segments.sync()
        drainIntrinsicSizes()
        XCTAssertEqual(reports.count, count, "unchanged native sizes are not published again")
        p.apply(wireBatch([["op": "frame", "id": 10, "x": 0.0, "y": 0.0,
                            "w": 300.0, "h": Double(size.height)]]))
        drainIntrinsicSizes()
        XCTAssertEqual(reports.count, count, "its height does not follow the box's width, so a resize does not remeasure")
        XCTAssertEqual(control.frame, owner.contentBox())
        // The control fills the content box: a border-box minimum also holds
        // the padding and border around it; a content-box one does not.
        p.apply(wireBatch([["op": "style", "id": 10,
                            "style": ["box_sizing": "border-box", "padding_top": 6.0, "padding_bottom": 4.0]],
                           contentOp(10, 300, Double(size.height), padding: [6, 0, 4, 0])]))
        drainIntrinsicSizes()
        XCTAssertEqual((reports.last ?? nil)?.height, size.height + 10)
        p.apply(wireBatch([["op": "frame", "id": 10, "x": 0.0, "y": 0.0,
                            "w": 300.0, "h": Double(size.height + 10)],
                           contentOp(10, 300, Double(size.height + 10), padding: [6, 0, 4, 0])]))
        XCTAssertEqual(control.frame, owner.contentBox())
        XCTAssertEqual(control.frame.height, size.height, "the padding does not squash the control")
        p.apply(wireBatch([["op": "style", "id": 10,
                            "style": ["box_sizing": "content-box", "padding_top": 6.0, "padding_bottom": 4.0]]]))
        drainIntrinsicSizes()
        XCTAssertEqual((reports.last ?? nil)?.height, size.height)
        // Measured at zero width too: tabs replaced there still reserve it.
        p.apply(wireBatch([["op": "frame", "id": 10, "x": 0.0, "y": 0.0, "w": 0.0, "h": 0.0],
                           ["op": "children", "id": 10, "ids": [12, 11]]]))
        drainIntrinsicSizes()
        XCTAssertEqual((reports.last ?? nil)?.height, size.height, "a zero-width tablist keeps its native minimum")
        let replaced = try XCTUnwrap(p.segments.control(of: 10))
        p.apply(wireBatch([["op": "props", "id": 10,
                            "set": ["accessibilityRole": "tablist", "accessibilityOrientation": "vertical"]]]))
        drainIntrinsicSizes()
        XCTAssertNil(reports.last ?? nil, "leaving the projection clears its native minimum")
        XCTAssertNil(control.superview)
        XCTAssertNil(replaced.superview)
        XCTAssertFalse(try XCTUnwrap(p.views[11]).isHidden)
    }

    func testMissingRawSymbolsKeepTabBarsAndClearSegmentImages() throws {
        let p = tabBarFixture()
        defer { p.reset() }
        let owner = try XCTUnwrap(p.views[10])
        let icon = try XCTUnwrap(p.views[13])
        let bar = try XCTUnwrap(owner.subviews.first { $0 is UITabBar } as? UITabBar)
        p.apply(wireBatch([["op": "props", "id": 11, "set": ["accessibilitySelected": "true"]]]))
        var sizes: [CGSize?] = []
        p.onIntrinsic = { reports in for (id, size) in reports where id == 13 { sizes.append(size) } }
        for name in ["airpodsmax", "exact.nonexistent.symbol", "", "airpodsmax"] {
            p.apply(wireBatch([["op": "props", "id": 13,
                "set": ["imageSource": "symbol:sf/\(name)", "symbolName": name]]]))
            drainIntrinsicSizes()
            XCTAssertTrue(bar.superview === owner, "lookup never changes projection")
            XCTAssertEqual(bar.selectedItem?.tag, 0)
            XCTAssertEqual(bar.items?.first?.title, "Home")
            let found = name == "airpodsmax"
            XCTAssertEqual(icon.image != nil, found)
            XCTAssertEqual(bar.items?.first?.image != nil, found)
            XCTAssertNil(icon.symbolRefusal)
            // Unsized, a symbol is the platform body size (LLP 1115 D3).
            let body = PageFacts.defaultRootFontSize
            if !found { XCTAssertEqual(sizes.last ?? nil, CGSize(width: body, height: body)) }
        }
        p.apply(wireBatch([
            ["op": "props", "id": 11, "set": ["accessibilityLabel": "Home"]],
            ["op": "children", "id": 11, "ids": [13]],
            ["op": "children", "id": 12, "ids": [15]],
            ["op": "frame", "id": 13, "x": 0.0, "y": 0.0, "w": 24.0, "h": 24.0],
        ]))
        let segments = try XCTUnwrap(owner.subviews.first { $0 is UISegmentedControl } as? UISegmentedControl)
        XCTAssertNotNil(segments.imageForSegment(at: 0))
        for name in ["exact.nonexistent.symbol", "", "airpodsmax"] {
            p.apply(wireBatch([["op": "props", "id": 13,
                "set": ["imageSource": "symbol:sf/\(name)", "symbolName": name]]]))
            XCTAssertEqual(icon.image != nil, name == "airpodsmax")
            XCTAssertEqual(segments.imageForSegment(at: 0)?.accessibilityLabel, "Home")
            XCTAssertNil(segments.titleForSegment(at: 0))
            XCTAssertEqual(segments.selectedSegmentIndex, 0)
        }
    }

    /// LLP 1116.004 D2 (unit-converter, round 6): four text tabs in a
    /// grouped row overflowed it by the hidden tabs' own widths. The control
    /// reports no narrowest width, so the kernel fits the tablist to the row;
    /// in a box narrower than its equal segments need, each segment takes
    /// its content's share, so the longest title keeps its letters.
    func testASegmentedControlTooWideForItsBoxApportionsItsSegmentsByContent() throws {
        let p = tabBarFixture()
        defer { p.reset() }
        var reports: [CGSize?] = []
        p.onIntrinsic = { sizes in for (id, size) in sizes where id == 10 { reports.append(size) } }
        p.apply(wireBatch([
            ["op": "props", "id": 14, "set": ["text": "Length"]],
            ["op": "props", "id": 16, "set": ["text": "Temperature and pressure"]],
            ["op": "children", "id": 11, "ids": [14]],
            ["op": "children", "id": 12, "ids": [16]],
        ]))
        let control = try XCTUnwrap(p.segments.control(of: 10))
        drainIntrinsicSizes()
        XCTAssertEqual((reports.last ?? nil)?.width, 1, "the tablist's automatic minimum width: none")
        XCTAssertFalse(control.apportionsSegmentWidthsByContent, "wide enough: equal segments")
        let equal = control.intrinsicContentSize.width
        XCTAssertLessThan(equal, 400)
        // Short of two equal segments, room for the two titles side by side.
        let narrow = (equal * 0.75).rounded()
        p.apply(wireBatch([["op": "frame", "id": 10, "x": 0.0, "y": 0.0, "w": Double(narrow), "h": 32.0]]))
        XCTAssertEqual(control.frame.width, narrow, "the control fills the kernel's box")
        XCTAssertTrue(control.apportionsSegmentWidthsByContent, "narrower than its equal segments: by content")
        control.layoutIfNeeded()
        let views = (control as? LabelledSegmentedControl)?.segmentViews ?? []
        XCTAssertEqual(views.count, 2)
        if views.count == 2 { XCTAssertGreaterThan(views[1].bounds.width, views[0].bounds.width, "the long title its share") }
        p.apply(wireBatch([["op": "frame", "id": 10, "x": 0.0, "y": 0.0, "w": 400.0, "h": 32.0]]))
        XCTAssertFalse(control.apportionsSegmentWidthsByContent, "wide again: equal")
    }

    /// A long press runs the `contextmenu` of the tab whose segment is under
    /// it, also where UIKit sizes the segments by their content.
    func testALongPressNamesTheSegmentUnderItWhenSegmentsAreSizedByContent() throws {
        let p = fixture()
        defer { p.reset() }
        p.apply(wireBatch([
            ["op": "create", "id": 30, "kind": "view", "props": ["accessibilityRole": "tablist"]],
            ["op": "create", "id": 31, "kind": "button", "props": ["accessibilityRole": "tab", "accessibilitySelected": "true"], "handlers": ["press", "contextmenu"]],
            ["op": "create", "id": 32, "kind": "button", "props": ["accessibilityRole": "tab"], "handlers": ["press", "contextmenu"]],
            ["op": "create", "id": 33, "kind": "text", "props": ["text": "Temperature"]],
            ["op": "create", "id": 34, "kind": "text", "props": ["text": "Length"]],
            ["op": "children", "id": 31, "ids": [33]],
            ["op": "children", "id": 32, "ids": [34]],
            ["op": "children", "id": 30, "ids": [31, 32]],
            ["op": "roots", "ids": [1, 30]],
            ["op": "frame", "id": 30, "x": 0.0, "y": 0.0, "w": 160.0, "h": 32.0],
        ]))
        let control = try XCTUnwrap(p.segments.control(of: 30))
        control.layoutIfNeeded()
        XCTAssertTrue(control.apportionsSegmentWidthsByContent, "too narrow for equal segments")
        let views = control.segmentViews
        XCTAssertEqual(views.count, 2)
        let boundary = try XCTUnwrap(views.first).convert(try XCTUnwrap(views.first).bounds, to: control).maxX
        XCTAssertGreaterThan(boundary, 85, "Temperature takes more than its half")
        for x in stride(from: 2.0, to: 158.0, by: 4.0) where abs(x - boundary) > 1 {
            let tab = p.segments.contextTab(at: CGPoint(x: x, y: control.bounds.midY), in: control)
            XCTAssertEqual(tab?.id, x < boundary ? 31 : 32, "at x = \(x), the segments meeting at \(boundary)")
        }
    }

    /// LLP 1116 D6: a text tab whose `aria-label` says more than its words
    /// (`Temp`, named `Temperature`) keeps the native segmented control —
    /// the words its title, the label its segment's accessibility label —
    /// where it fell back to the authored pills (unit-converter). LLP 1116
    /// D2: each segment, and a tab bar's item, carries its tab's `testId`.
    func testASegmentsAriaLabelIsItsAccessibilityLabelAndItsTestIdItsIdentifier() throws {
        let p = tabBarFixture()
        defer { p.reset() }
        let owner = try XCTUnwrap(p.views[10])
        let bar = try XCTUnwrap(owner.subviews.first { $0 is UITabBar } as? UITabBar)
        p.apply(wireBatch([
            ["op": "props", "id": 11, "set": ["testId": "tab-home"]],
            ["op": "props", "id": 12, "set": ["testId": "tab-saved"]],
        ]))
        XCTAssertEqual(bar.items?.map(\.accessibilityIdentifier), ["tab-home", "tab-saved"], "a tab bar item's identifier is its tab's testId")
        XCTAssertEqual(bar.items?.map(\.title), ["Home", "Saved"])
        p.apply(wireBatch([
            ["op": "props", "id": 10, "set": ["testId": "units"]],
            ["op": "props", "id": 12, "set": ["accessibilityLabel": "Saved items"]],
            ["op": "children", "id": 11, "ids": [14]],
            ["op": "children", "id": 12, "ids": [16]],
        ]))
        let control = try XCTUnwrap(p.segments.control(of: 10) as? LabelledSegmentedControl, "still the native control")
        XCTAssertTrue(try XCTUnwrap(p.views[12]).isHidden, "the authored pill is not drawn")
        XCTAssertEqual([control.titleForSegment(at: 0), control.titleForSegment(at: 1)], ["Home", "Saved"], "the words are the titles")
        XCTAssertEqual(control.accessibilityIdentifier, "units")
        control.frame = CGRect(x: 0, y: 0, width: 300, height: 32)
        control.layoutIfNeeded()
        let segments = control.segmentViews
        XCTAssertEqual(segments.count, 2)
        XCTAssertEqual(segments.last?.accessibilityLabel, "Saved items", "the aria-label is the segment's name")
        XCTAssertEqual(segments.map(\.accessibilityIdentifier), ["tab-home", "tab-saved"])
        // The label taken away: the segment's name is its words again.
        p.apply(wireBatch([["op": "props", "id": 12, "set": [:], "clear": ["accessibilityLabel"]]]))
        control.layoutIfNeeded()
        XCTAssertNotEqual(control.segmentViews.last?.accessibilityLabel, "Saved items")
    }

    func testResetRestoresTabBarMembersAndDropsTheProjection() throws {
        let p = tabBarFixture()
        let owner = try XCTUnwrap(p.views[10])
        let first = try XCTUnwrap(p.views[11])
        let second = try XCTUnwrap(p.views[12])
        let old = try XCTUnwrap(owner.subviews.first { $0 is UITabBar })
        XCTAssertTrue(first.isHidden)
        XCTAssertTrue(second.isHidden)
        p.segments.reset()
        XCTAssertNil(old.superview)
        XCTAssertFalse(first.isHidden)
        XCTAssertFalse(second.isHidden)
        p.segments.sync()
        let fresh = try XCTUnwrap(owner.subviews.first { $0 is UITabBar })
        XCTAssertFalse(fresh === old)
        p.reset()
        XCTAssertNil(fresh.superview)
    }

    func testResetEndsExitsBeforeAnIDIsReused() throws {
        let p = fixture()
        let old = try XCTUnwrap(p.views[2])
        let child = try XCTUnwrap(p.views[3])
        p.apply(wireBatch([["op": "exit", "id": 2]]))
        p.reset()
        XCTAssertTrue(p.leaving.isEmpty)
        XCTAssertNil(old.superview)
        XCTAssertNil(child.superview?.superview)
        p.apply(wireBatch([
            ["op": "create", "id": 2, "kind": "view"],
            ["op": "roots", "ids": [2]],
        ]))
        let fresh = try XCTUnwrap(p.views[2])
        p.apply(wireBatch([["op": "destroy", "id": 2]]))
        XCTAssertNil(p.views[2])
        XCTAssertNil(fresh.superview)
    }

    func testALayoutSpringCannotGiveTheSurfaceANegativeSize() throws {
        let p = fixture()
        let v = try XCTUnwrap(p.views[3])
        p.apply(wireBatch([["op": "present", "id": 3, "property": "layout", "x": 4.0, "y": 10.0, "w": -150.0, "h": -100.0]]))
        let surface = try XCTUnwrap(v.surface)
        XCTAssertEqual(surface.bounds.size, .zero)
        XCTAssertEqual(surface.frame.origin, .zero)
        XCTAssertEqual(v.bounds.size, CGSize(width: 100, height: 50))
    }

    func testABoxClosingToZeroWidthShowsTheWidthItClosesFrom() throws {
        let p = fixture()
        let v = try XCTUnwrap(p.views[3])
        p.apply(wireBatch([["op": "style", "id": 3, "style": ["overflow_x": "hidden", "overflow_y": "hidden"]]]))
        // Laid out closed, 200 wide on screen: the difference carries what
        // no ratio of a zero width could (GitHub #940).
        p.apply(wireBatch([
            ["op": "frame", "id": 3, "x": 0.0, "y": 0.0, "w": 0.0, "h": 50.0],
            ["op": "present", "id": 3, "property": "layout", "x": 0.0, "y": 0.0, "w": 200.0, "h": 0.0],
        ]))
        let surface = try XCTUnwrap(v.surface)
        XCTAssertEqual(surface.frame, CGRect(x: 0, y: 0, width: 200, height: 50))
        XCTAssertEqual(surface.clip.path?.boundingBox.width ?? 0, 200, accuracy: 1e-9)
        p.apply(wireBatch([["op": "present", "id": 3, "property": "layout", "x": 0.0, "y": 0.0, "w": 100.0, "h": 0.0]]))
        XCTAssertEqual(surface.frame.width, 100)
        XCTAssertEqual(surface.clip.path?.boundingBox.width ?? 0, 100, accuracy: 1e-9)
        p.apply(wireBatch([["op": "present", "id": 3, "property": "layout", "x": 0.0, "y": 0.0, "w": 0.0, "h": 0.0]]))
        XCTAssertNil(v.surface)
        // A frame after the layout op in its batch resizes the surface.
        p.apply(wireBatch([["op": "frame", "id": 3, "x": 0.0, "y": 0.0, "w": 200.0, "h": 50.0]]))
        p.apply(wireBatch([
            ["op": "present", "id": 3, "property": "layout", "x": 0.0, "y": 0.0, "w": 200.0, "h": 0.0],
            ["op": "frame", "id": 3, "x": 0.0, "y": 0.0, "w": 0.0, "h": 50.0],
        ]))
        XCTAssertEqual(v.surface?.frame.width, 200)
        // From the size layout gave, not the one drawn on the pixel grid.
        p.apply(wireBatch([
            ["op": "frame", "id": 3, "x": 0.0, "y": 0.0, "w": 100.25, "h": 50.0],
            ["op": "present", "id": 3, "property": "layout", "x": 0.0, "y": 0.0, "w": -100.25, "h": 0.0],
        ]))
        XCTAssertEqual(v.surface?.frame.width, 0)
    }

    func testAnExitRetiresAPopoverInItsLiftLayer() throws {
        let p = fixture()
        defer { p.menus.reset(); window.isHidden = true }
        let leaving = try XCTUnwrap(p.views[2]), pop = try XCTUnwrap(p.views[3])
        p.apply(wireBatch([["op": "props", "id": 3, "set": ["popover": "auto", "id": "lifted"]]]))
        p.menus.agentShow(pop, named: "lifted", from: 4)
        XCTAssertFalse(pop.isDescendant(of: leaving))
        XCTAssertTrue(p.menus.isOpen(pop))
        p.apply(wireBatch([["op": "exit", "id": 2], ["op": "children", "id": 1, "ids": [4]]]))
        XCTAssertNil(p.views[3], "the lifted descendant retires with its authored parent")
        XCTAssertFalse(p.menus.isOpen(pop))
        XCTAssertNil(pop.superview)
        XCTAssertEqual(Set(try XCTUnwrap(p.leaving[2]).members.map(\.id)), [2, 3])
        p.apply(wireBatch([["op": "destroy", "id": 2]]))
        XCTAssertNil(pop.presenter)
    }

    func testAnExitKeepsTheViewWhereItWasInertAndUnnamedUntilItsDestroy() throws {
        let p = fixture()
        let (parent, leaving, inner, sibling) = try (XCTUnwrap(p.views[1]), XCTUnwrap(p.views[2]), XCTUnwrap(p.views[3]), XCTUnwrap(p.views[4]))
        // The host's batch: the exit first, then the tree without it; its
        // destroy is withheld until the engine passes the exit's end.
        p.apply(wireBatch([
            ["op": "exit", "id": 2],
            ["op": "children", "id": 1, "ids": [4]],
            ["op": "frame", "id": 4, "x": 0.0, "y": 0.0, "w": 400.0, "h": 50.0],
        ]))
        XCTAssertNil(p.views[2], "out of every lookup by id")
        XCTAssertNil(p.views[3], "its subtree too")
        XCTAssertTrue(leaving.superview === parent, "still in the window")
        XCTAssertTrue(inner.superview === leaving)
        XCTAssertTrue(parent.subviews.first === leaving, "the ghost keeps its tree slot")
        XCTAssertGreaterThan(leaving.paintZPosition, sibling.paintZPosition)
        XCTAssertEqual(parent.paintRank, 1)
        XCTAssertEqual(leaving.frame, CGRect(x: 0, y: 0, width: 400, height: 50), "its last box")
        XCTAssertEqual(sibling.frame.minY, 0, "its sibling takes its place")
        XCTAssertFalse(leaving.isUserInteractionEnabled)
        XCTAssertTrue(leaving.accessibilityElementsHidden)
        p.apply(wireBatch([["op": "present", "id": 2, "property": "opacity", "x": 0.25, "y": 0.0]]))
        XCTAssertEqual(leaving.alpha, 0.25, accuracy: 1e-6, "the engine animates it")
        p.apply(wireBatch([["op": "destroy", "id": 2]]))
        XCTAssertNil(leaving.superview, "the exit ended")
        XCTAssertEqual(parent.paintRank, parent.rank)
        XCTAssertNil(inner.superview?.superview)
        XCTAssertTrue(p.views[4] === sibling)
    }

    func testALayoutPresentMovesTheBoxAndSizesOnlyItsSurface() throws {
        let p = fixture()
        let v = try XCTUnwrap(p.views[3])
        let clips = { v.clipsToBounds }
        let blue = [0, 0, 255, 255]
        p.apply(wireBatch([["op": "style", "id": 3, "style": ["background_color": blue, "border_radius_top_left": 6, "border_radius_top_right": 6, "border_radius_bottom_left": 6, "border_radius_bottom_right": 6, "overflow_x": "hidden", "overflow_y": "hidden"]]]))
        XCTAssertTrue(clips())
        // Grown from 25 to 50 high and moved down 10: it starts where it was.
        p.apply(wireBatch([["op": "present", "id": 3, "property": "layout", "x": 4.0, "y": 10.0, "w": 0.0, "h": -25.0]]))
        // About the center, as UIKit applies a transform.
        let corner = { (x: CGFloat, y: CGFloat) -> CGPoint in
            let c = CGPoint(x: 50, y: 25)
            let q = CGPoint(x: x - c.x, y: y - c.y).applying(v.transform)
            return CGPoint(x: q.x + c.x, y: q.y + c.y)
        }
        let own = { v.layer.backgroundColor }
        let mask = { v.layer.mask }
        v.layer.displayIfNeeded()
        // Moved, never scaled: its content keeps its laid-out size.
        XCTAssertEqual(corner(0, 0).x, 4, accuracy: 1e-9)
        XCTAssertEqual(corner(0, 0).y, 10, accuracy: 1e-9)
        XCTAssertEqual(corner(100, 50).x, 104, accuracy: 1e-9)
        XCTAssertEqual(corner(100, 50).y, 60, accuracy: 1e-9)
        XCTAssertEqual(v.bounds.size, CGSize(width: 100, height: 50), "laid out at its final size")
        // Its surface is the shown size, from its top-left corner, and its
        // own is off; its children are clipped to the shown box.
        let surface = try XCTUnwrap(v.surface)
        XCTAssertEqual(surface.frame, CGRect(x: 0, y: 0, width: 100, height: 25))
        XCTAssertEqual(surface.backgroundColor?.components?.map { Double($0) }, [0, 0, 1, 1])
        XCTAssertEqual(surface.cornerRadius, 6)
        XCTAssertNil(own())
        XCTAssertFalse(clips(), "the mask clips instead: the surface may outgrow the frame")
        XCTAssertTrue(mask() === surface.clip)
        XCTAssertEqual(surface.clip.path?.boundingBox.height ?? 0, 25, accuracy: 1e-9)
        XCTAssertEqual(surface.clip.path?.boundingBox.width ?? 0, 100, accuracy: 1e-9)
        // Shrinking past the frame: the surface is larger than the view.
        p.apply(wireBatch([["op": "present", "id": 3, "property": "layout", "x": 0.0, "y": 0.0, "w": 0.0, "h": 25.0]]))
        XCTAssertEqual(surface.frame.height, 75)
        // At rest the node paints its own surface and clip again.
        p.apply(wireBatch([["op": "present", "id": 3, "property": "layout", "x": 0.0, "y": 0.0, "w": 0.0, "h": 0.0]]))
        XCTAssertNil(v.surface)
        XCTAssertNil(surface.superlayer)
        XCTAssertTrue(clips())
        XCTAssertNil(mask())
        XCTAssertTrue(v.transform.isIdentity)
    }

    func testAResizingBorderedBoxClipsChildrenInsideItsBorder() throws {
        let p = fixture()
        let card = try XCTUnwrap(p.views[2])
        p.apply(wireBatch([
            ["op": "create", "id": 5, "kind": "view", "style": ["background_color": [0, 0, 255, 255]]],
            ["op": "children", "id": 2, "ids": [3, 5]],
            ["op": "frame", "id": 5, "x": 6.0, "y": 6.0, "w": 188.0, "h": 100.0],
            ["op": "style", "id": 2, "style": ["background_color": [255, 255, 255, 255],
                "border_width": 6, "border_color_top": [255, 0, 0, 255], "border_radius": 40,
                "corner_shape": ["apple", "apple", "apple", "apple"],
                "overflow_x": "hidden", "overflow_y": "hidden"]],
            ["op": "style", "id": 3, "style": ["background_color": [0, 255, 0, 255]]],
            ["op": "frame", "id": 2, "x": 0.0, "y": 0.0, "w": 200.0, "h": 200.0],
            ["op": "frame", "id": 3, "x": 6.0, "y": 6.0, "w": 188.0, "h": 288.0],
        ]))
        let flat = try XCTUnwrap(p.flats.leaves[5])
        for height in [100.0, 300.0] {
            p.apply(wireBatch([["op": "present", "id": 2, "property": "layout",
                               "x": 0.0, "y": 0.0, "w": 0.0, "h": height - 200]]))
            let surface = try XCTUnwrap(card.surface)
            XCTAssertTrue(flat.layer.superlayer === card.container.layer, "flat children follow the animated clip")
            let clip = try XCTUnwrap(surface.clip.path)
            XCTAssertEqual(clip.boundingBoxOfPath, CGRect(x: 6, y: 6, width: 188, height: height - 12))
            XCTAssertFalse(clip.contains(CGPoint(x: 100, y: height - 3)), "children cannot cover the bottom border")
            XCTAssertFalse(clip.contains(CGPoint(x: 7, y: 7)), "children follow the inner corner")
            XCTAssertFalse(surface.clipped === card.layer, "the child mask must not cut off the surface's border")
            let outer = try XCTUnwrap(card.layer.mask as? CAShapeLayer)
            XCTAssertEqual(outer.path?.boundingBoxOfPath, CGRect(x: 0, y: 0, width: 200, height: height), "the node's own paint still clips")
            XCTAssertFalse(surface.clipped?.masksToBounds ?? true, "a shrinking clip can exceed the laid-out bounds")
        }
        p.apply(wireBatch([["op": "present", "id": 2, "property": "layout",
                           "x": 0.0, "y": 0.0, "w": 0.0, "h": 0.0]]))
        XCTAssertNil(card.surface)
        XCTAssertTrue(card.clipsToBounds)
        XCTAssertNil(card.clipBox)
        XCTAssertTrue(p.views[3]?.superview === card)
        XCTAssertTrue(flat.layer.superlayer === card.layer, "flat children return with the views")
    }

    func testAPlacedBoxKeepsItsSurfaceClipsWhileResizingAndChangingTargets() throws {
        for border in [0, 6] {
            let p = fixture(), card = try XCTUnwrap(p.views[2])
            defer { p.reset(); window.isHidden = true }
            var style: [String: Any] = ["border_width": border, "border_radius": 20,
                "overflow_x": "hidden", "overflow_y": "hidden"]
            p.apply(wireBatch([["op": "style", "id": 2, "style": style]]))
            card.placement = [1, 0, 100, 0, 1, 0, 0, 0, 1, 0]
            let placementMask = try XCTUnwrap(card.layer.mask)
            for height in [100.0, 150.0] {
                p.apply(wireBatch([["op": "present", "id": 2, "property": "layout", "h": height - 50]]))
                let surface = try XCTUnwrap(card.surface)
                XCTAssertTrue(card.layer.mask === placementMask, "the empty placement mask stays installed")
                XCTAssertEqual(card.alpha, 1)
                XCTAssertTrue(surface.clipped?.boxMask === surface.clip, "the requested clip survives the next frame")
                XCTAssertTrue(card.layer.boxMask === (border == 0 ? surface.clip : surface.ownClip))
                XCTAssertEqual(surface.clip.path?.boundingBoxOfPath.height ?? 0, height - CGFloat(2 * border), accuracy: 1e-9)
            }
            let surface = try XCTUnwrap(card.surface), oldTarget = try XCTUnwrap(surface.clipped)
            style["overflow_y"] = "auto"
            p.apply(wireBatch([["op": "style", "id": 2, "style": style]]))
            let scroll = try XCTUnwrap(card.scroll)
            XCTAssertFalse(oldTarget.boxMask === surface.clip, "the old target releases the requested clip")
            XCTAssertTrue(scroll.layer.mask === surface.clip)
            XCTAssertTrue(card.layer.mask === placementMask)
            p.apply(wireBatch([["op": "present", "id": 2, "property": "layout", "h": 0.0]]))
            XCTAssertNil(card.surface)
            XCTAssertFalse(card.layer.boxMask === surface.clip || card.layer.boxMask === surface.ownClip)
            XCTAssertNil(scroll.layer.mask)
            XCTAssertTrue(scroll.clipsToBounds)
            XCTAssertTrue(card.layer.mask === placementMask, "ending the resize does not end placement")
            let restored = card.layer.boxMask
            card.placement = nil
            XCTAssertTrue(card.layer.mask === restored, "unplacing restores the box's current mask")
        }
    }

    func testALayoutBatchGivesTheSiblingAndSurfaceMatchingModelEdges() throws {
        let p = fixture(), card = try XCTUnwrap(p.views[2]), sibling = try XCTUnwrap(p.views[4])
        for offset in [-30.0, 100.0, 60.0, 0.0] {
            p.apply(wireBatch([
                ["op": "present", "id": 2, "property": "layout", "h": offset],
                ["op": "present", "id": 4, "property": "layout", "y": offset],
            ]))
            let height = card.surface?.bounds.height ?? card.bounds.height
            let bottom = card.convert(CGPoint(x: 0, y: height), to: p.root).y
            XCTAssertEqual(sibling.convert(.zero, to: p.root).y, bottom, accuracy: 1e-6)
        }
    }

    func testAResizingClipHandsItsChildrenToANewScroller() throws {
        let p = fixture(), card = try XCTUnwrap(p.views[2])
        var style: [String: Any] = ["border_width": 6, "border_color_top": [255, 0, 0, 255], "border_radius": 40,
            "overflow_x": "hidden", "overflow_y": "hidden"]
        p.apply(wireBatch([["op": "style", "id": 2, "style": style],
            ["op": "present", "id": 2, "property": "layout", "h": 100.0]]))
        let oldClip = try XCTUnwrap(card.clipBox)
        style["overflow_y"] = "auto"
        p.apply(wireBatch([["op": "style", "id": 2, "style": style]]))
        let scroll = try XCTUnwrap(card.scroll), child = try XCTUnwrap(p.views[3])
        XCTAssertTrue(child.superview === scroll, "the child must scroll during the transition")
        XCTAssertNil(card.clipBox)
        XCTAssertNil(oldClip.layer.mask, "the former clip releases its surface mask")
        XCTAssertTrue(oldClip.clipsToBounds, "the former clip restores its bounds clipping")
        XCTAssertTrue(card.surface?.clipped === scroll.layer)
        XCTAssertFalse(card.clipsToBounds, "the shrinking surface extends beyond the laid-out box")
        let surface = try XCTUnwrap(card.surface)
        XCTAssertEqual(surface.bounds.height, 150)
        let ctx = try XCTUnwrap(CGContext(data: nil, width: 400, height: 160, bitsPerComponent: 8,
            bytesPerRow: 1600, space: CGColorSpace(name: CGColorSpace.sRGB)!,
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        let format = UIGraphicsImageRendererFormat(); format.scale = 1
        let image = UIGraphicsImageRenderer(size: CGSize(width: 400, height: 160), format: format).image {
            p.root.layer.render(in: $0.cgContext)
        }
        ctx.draw(try XCTUnwrap(image.cgImage), in: CGRect(x: 0, y: 0, width: 400, height: 160))
        let bytes = try XCTUnwrap(ctx.data).assumingMemoryBound(to: UInt8.self)
        XCTAssertEqual((0..<4).map { Int(bytes[(147 * 400 + 200) * 4 + $0]) }, [255, 0, 0, 255],
                       "the bottom border draws beyond the node's 50-point bounds")
        p.apply(wireBatch([["op": "present", "id": 2, "property": "layout", "h": 0.0]]))
        XCTAssertTrue(child.superview === scroll, "the child must still scroll after the transition")
        XCTAssertTrue(scroll.clipsToBounds)
        XCTAssertNil(scroll.layer.mask)
        XCTAssertTrue(card.clipsToBounds, "the laid-out clip returns when the surface ends")
    }

    func testRemovingScrollingDuringAResizeRestoresVisibleOverflow() throws {
        let p = fixture(), card = try XCTUnwrap(p.views[2]), child = try XCTUnwrap(p.views[3])
        p.apply(wireBatch([
            ["op": "style", "id": 2, "style": ["border_radius": 20,
                "overflow_x": "auto", "overflow_y": "auto"]],
            ["op": "frame", "id": 3, "x": 380.0, "y": 0.0, "w": 100.0, "h": 50.0],
            ["op": "present", "id": 2, "property": "layout", "h": 100.0],
        ]))
        let surface = try XCTUnwrap(card.surface), scroll = try XCTUnwrap(card.scroll)
        XCTAssertTrue(scroll.layer.mask === surface.clip)
        p.apply(wireBatch([["op": "style", "id": 2, "style": ["border_radius": 20,
            "overflow_x": "visible", "overflow_y": "visible"]]]))
        p.apply(wireBatch([["op": "present", "id": 2, "property": "layout", "h": 75.0]]))
        XCTAssertNotNil(card.surface, "the resize is still active")
        XCTAssertNil(card.scroll)
        XCTAssertTrue(child.superview === card)
        XCTAssertGreaterThan(child.frame.maxX, card.bounds.maxX)
        XCTAssertNil(scroll.layer.mask, "the detached scroller releases the mask")
        XCTAssertNil(surface.clipped, "visible overflow has no clipping target")
        XCTAssertNil(card.layer.mask, "the surface must not fall back to masking the host")
        XCTAssertFalse(card.clipsToBounds)
        p.apply(wireBatch([["op": "present", "id": 2, "property": "layout", "h": 0.0]]))
        XCTAssertNil(card.surface)
        XCTAssertNil(card.layer.mask)
        XCTAssertFalse(card.clipsToBounds)
    }

    func testNativeContentClipsInsideAResizingBorder() throws {
        let p = fixture()
        for (id, kind, props) in [(10, "textarea", ["value": "Content"]),
                                  (12, "input", ["value": "Content"]),
                                  (11, "image", ["imageSource": "symbol:sf/square.fill", "symbolName": "square.fill"])] {
            p.apply(wireBatch([
                ["op": "create", "id": id, "kind": kind, "props": props,
                 "style": ["appearance": "none", "border_width": 6, "border_radius": 40,
                           "overflow_x": "hidden", "overflow_y": "hidden"]],
                ["op": "roots", "ids": [1, id]],
                ["op": "frame", "id": id, "x": 0.0, "y": 0.0, "w": 200.0, "h": 200.0],
                ["op": "present", "id": id, "property": "layout", "h": -100.0],
            ]))
            let node = try XCTUnwrap(p.views[UInt32(id)])
            let native = try XCTUnwrap(kind == "input" ? node.field : (kind == "textarea" ? node.textArea : node.symbolView))
            let surface = try XCTUnwrap(node.surface)
            XCTAssertTrue(native.superview === node.clipBox, "\(kind) belongs inside the padding-edge clip")
            XCTAssertTrue(native.superview?.layer.mask === surface.clip)
            XCTAssertFalse(try XCTUnwrap(surface.clip.path).contains(CGPoint(x: 7, y: 7)))
            if kind == "input" {
                node.field?.removeFromSuperview()
                node.makeField()
                XCTAssertTrue(node.field?.superview === node.clipBox, "a field created during the resize enters the clip")
                node.props["type"] = "search"
                p.apply(wireBatch([["op": "style", "id": id, "style": ["appearance": "auto",
                    "border_width": 6, "border_radius": 40, "overflow_x": "hidden", "overflow_y": "hidden"]]]))
                node.styleNativeField()
                XCTAssertTrue(try XCTUnwrap(node.searchChrome).superview === node.clipBox,
                              "search chrome stays beside the field")
            }
            let replacement = try XCTUnwrap(kind == "input" ? node.field : native)
            p.apply(wireBatch([["op": "present", "id": id, "property": "layout", "h": 0.0]]))
            XCTAssertTrue(replacement.superview === node, "native content returns when the surface ends")
            if kind == "input" {
                let chrome = try XCTUnwrap(node.searchChrome)
                XCTAssertTrue(chrome.superview === node)
                p.apply(wireBatch([["op": "present", "id": id, "property": "layout", "h": -100.0]]))
                XCTAssertTrue(chrome.superview === node.clipBox, "existing search chrome enters the clip with its field")
                XCTAssertTrue(replacement.superview === node.clipBox)
                p.apply(wireBatch([["op": "present", "id": id, "property": "layout", "h": 0.0]]))
                XCTAssertTrue(chrome.superview === node)
                XCTAssertTrue(replacement.superview === node)
            }
        }
    }

    func testASurfaceTheLayerCannotSayIsDrawnUprightAtTheShownSize() throws {
        let p = fixture()
        let v = try XCTUnwrap(p.views[3])
        let (red, blue) = ([255, 0, 0, 255], [0, 0, 255, 255])
        // Sides that differ: drawn, as the node's own `draw(_:)` draws them.
        p.apply(wireBatch([["op": "style", "id": 3, "style": ["border_width": 4, "border_color_top": red, "border_color_right": red, "border_color_bottom": blue, "border_color_left": red]]]))
        p.apply(wireBatch([["op": "present", "id": 3, "property": "layout", "x": 0.0, "y": 0.0, "w": 0.0, "h": -25.0]]))
        let surface = try XCTUnwrap(v.surface)
        XCTAssertNotNil(surface.drawn)
        let (w, h) = (100, 25)
        // Its backing store, drawn as on screen, then snapshot as UIKit does.
        surface.displayIfNeeded()
        XCTAssertNotNil(surface.contents)
        let format = UIGraphicsImageRendererFormat(); format.scale = 1
        let image = UIGraphicsImageRenderer(size: CGSize(width: w, height: h), format: format).image { surface.render(in: $0.cgContext) }
        let ctx = try XCTUnwrap(CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4, space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        ctx.draw(try XCTUnwrap(image.cgImage), in: CGRect(x: 0, y: 0, width: w, height: h))
        let bytes = try XCTUnwrap(ctx.data).assumingMemoryBound(to: UInt8.self)
        let pixel = { (x: Int, y: Int) in (0..<4).map { Int(bytes[(y * w + x) * 4 + $0]) } }
        // The first row in memory is the top: red there, blue at the bottom.
        XCTAssertEqual(pixel(50, 1), red)
        XCTAssertEqual(pixel(50, h - 2), blue)
    }
}
#endif
