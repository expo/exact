#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactGroupedLists

final class GroupedCardIOSTests: GroupedListFixture {
    func testTheCardPaintsItsSectionsHostStateShadow() throws {
        var model = self.model()
        model.sections[0].cardView = 4
        let p = presenter { model }
        defer { window.isHidden = true }
        let section = try XCTUnwrap(p.views[2]), card = try XCTUnwrap(p.views[4])
        let shadow = { (y: Int) -> [[String: Any]] in [["o": [0, y], "b": 0, "s": 0, "c": [255, 0, 0, 255]]] }
        p.apply(wireBatch([
            ["op": "style", "id": 4, "style": ["box_shadow": shadow(4)]],
            ["op": "style", "id": 2, "style": ["looks": [
                "hover": ["box_shadow": shadow(8)], "focus_visible": ["box_shadow": shadow(12)],
            ]]],
        ]))
        section.lookState(hovered: true)
        XCTAssertTrue(section.boxShadows.isEmpty)
        XCTAssertEqual(card.boxShadows.first?.offset.height, 8)
        section.lookState(focus: true)
        XCTAssertEqual(card.boxShadows.first?.offset.height, 12)
        p.apply(wireBatch([["op": "props", "id": 2, "set": ["disabled": "true"], "clear": []]]))
        XCTAssertEqual(card.boxShadows.first?.offset.height, 4)
    }

    /// The collection's sublayers in the order Core Animation composites
    /// them: by zPosition, then by index.
    private func composited(_ collection: UICollectionView) -> [CALayer] {
        (collection.layer.sublayers ?? []).enumerated()
            .sorted { ($0.element.zPosition, $0.offset) < ($1.element.zPosition, $1.offset) }.map(\.element)
    }

    /// Each card's drawn outer shadow: its caster, and the collection's
    /// sublayer that is or holds it.
    private func shadows(_ collection: UICollectionView) -> [(caster: ShadowCaster, holder: CALayer)] {
        (collection.layer.sublayers ?? []).filter { !$0.isHidden }.compactMap { layer in
            guard let caster = layer as? ShadowCaster ?? layer.sublayers?.lazy.compactMap({ $0 as? ShadowCaster }).first,
                  !caster.isHidden else { return nil }
            return (caster, layer)
        }
    }

    /// Every card's shadow composites over the list's grounds and under its
    /// cells, separators, headers and footers. Returns how many shadows it checked.
    @discardableResult
    private func assertStacking(_ collection: UICollectionView, _ message: String, file: StaticString = #filePath, line: UInt = #line) -> Int {
        let order = composited(collection)
        let depth = { (view: UIView) in order.firstIndex(of: view.layer) ?? -1 }
        let views = collection.subviews.filter { !$0.isHidden }
        let grounds = views.filter { NSStringFromClass(type(of: $0)).contains("BackgroundColorDecorationView") }
        XCTAssertFalse(grounds.isEmpty, "\(message): UIKit paints the sections' grounds", file: file, line: line)
        let content = collection.visibleCells
            + [UICollectionView.elementKindSectionHeader, UICollectionView.elementKindSectionFooter].flatMap { collection.visibleSupplementaryViews(ofKind: $0) }
            + views.filter { NSStringFromClass(type(of: $0)).contains("SeparatorView") }
        let drawn = shadows(collection)
        for (_, holder) in drawn {
            let at = order.firstIndex(of: holder) ?? -1
            XCTAssertEqual(grounds.filter { depth($0) > at }.count, 0, "\(message): a section's ground covers the shadow", file: file, line: line)
            XCTAssertEqual(content.filter { depth($0) < at }.map { "\(type(of: $0))" }, [], "\(message): the shadow is drawn over these", file: file, line: line)
        }
        return drawn.count
    }

    func testACardScrolledJustOffTheTopDrawsUnderTheNextSection() throws {
        var model = self.model()
        model.sections[0].cardView = 4
        model.sections[0].footer = nil
        model.sections[1].header = "Next section"
        let p = presenter(height: 160) { model }
        defer { window.isHidden = true }
        let collection = try XCTUnwrap(try list(p).collection as? GroupedCollectionView)
        p.apply(wireBatch([["op": "style", "id": 4, "style": ["box_shadow": [
            ["o": [0, 4], "b": 14, "s": 0, "c": [0, 0, 0, 128]],
        ]]]]))
        collection.layoutIfNeeded()
        let first = try XCTUnwrap(collection.layoutAttributesForItem(at: IndexPath(item: 0, section: 0)))
        let last = try XCTUnwrap(collection.layoutAttributesForItem(at: IndexPath(item: 2, section: 0)))
        collection.contentOffset.y = last.frame.maxY + 1
        collection.layoutIfNeeded()
        XCTAssertFalse(collection.indexPathsForVisibleItems.contains { $0.section == 0 })
        XCTAssertTrue(collection.indexPathsForVisibleItems.contains { $0.section == 1 })
        XCTAssertNotNil(collection.supplementaryView(forElementKind: UICollectionView.elementKindSectionHeader, at: IndexPath(item: 0, section: 1)))
        let shadow = try XCTUnwrap(shadows(collection).first, "the card's shadow still reaches into view")
        let extent = shadow.caster.convert(shadow.caster.bounds, to: collection.layer)
        XCTAssertTrue(extent.contains(first.frame.union(last.frame)), "the offscreen card's shadow")
        XCTAssertTrue(extent.intersects(collection.bounds))
        XCTAssertEqual(assertStacking(collection, "card off the top"), 1)
    }

    func testAfterCellReuseTheShadowStaysUnderTheNextSectionsHeader() throws {
        var model = self.model()
        model.sections[0].cardView = 4
        model.sections[1].cardView = 5
        model.sections[1].header = "Next section"
        let p = presenter(height: 120) { model }
        defer { window.isHidden = true }
        let collection = try XCTUnwrap(try list(p).collection as? GroupedCollectionView)
        p.apply(wireBatch([4, 5].map { id in ["op": "style", "id": id, "style": ["box_shadow": [
            ["o": [0, 4], "b": 14, "s": 0, "c": [0, 0, 0, 128]],
        ]]] }))
        collection.layoutIfNeeded()
        let top = -collection.adjustedContentInset.top
        let bottom = collection.contentSize.height + collection.adjustedContentInset.bottom - collection.bounds.height
        XCTAssertGreaterThan(bottom, top + 100, "the list scrolls")
        var shown: [ObjectIdentifier: Set<IndexPath>] = [:]
        var underHeader = 0
        let header = IndexPath(item: 0, section: 1)
        for y in Array(stride(from: top, through: bottom, by: 15)) + Array(stride(from: bottom, through: top, by: -15)) {
            collection.contentOffset.y = y
            collection.layoutIfNeeded()
            for cell in collection.visibleCells {
                if let path = collection.indexPath(for: cell) { shown[ObjectIdentifier(cell), default: []].insert(path) }
            }
            let checked = assertStacking(collection, "at \(y)")
            if checked > 0, collection.supplementaryView(forElementKind: UICollectionView.elementKindSectionHeader, at: header) != nil { underHeader += 1 }
        }
        XCTAssertTrue(shown.values.contains { $0.count > 1 }, "UIKit reused a cell for another row")
        XCTAssertGreaterThan(underHeader, 0, "a shadow was drawn with the next section's header in view")
    }

    func testTheShadowStaysBehindTheSectionsHeaderAndFooter() throws {
        var model = self.model()
        model.sections[0].cardView = 4
        let p = presenter { model }
        defer { window.isHidden = true }
        let collection = try list(p).collection
        p.apply(wireBatch([["op": "style", "id": 4, "style": ["box_shadow": [
            ["o": [0, 4], "b": 14, "s": 0, "c": [0, 0, 0, 128]],
        ]]]]))
        collection.layoutIfNeeded()
        for kind in [UICollectionView.elementKindSectionHeader, UICollectionView.elementKindSectionFooter] {
            XCTAssertNotNil(collection.supplementaryView(forElementKind: kind, at: IndexPath(item: 0, section: 0)), kind)
        }
        XCTAssertEqual(assertStacking(collection, "its own header and footer"), 1)
    }

    func testAStandalonePresenterReleasesItsGroupedList() throws {
        weak var released: GroupedListHost?
        try autoreleasepool {
            let model = self.model()
            let p = presenter { model }
            released = host(p)
            XCTAssertNotNil(try list(p).collection)
            p.viewport.removeFromSuperview()
            window.isHidden = true
            window = nil
        }
        XCTAssertNil(released, "the collection's card callback must not retain its host")
    }

    func testTheShadowFollowsTheRoundedCardThroughRowChanges() throws {
        var model = self.model()
        model.sections[1].cardView = 5
        let p = presenter { model }
        defer { window.isHidden = true }
        let collection = try list(p).collection
        let style: [String: Any] = ["border_radius": 26, "box_shadow": [
            ["o": [0, 4], "b": 14, "s": 0, "c": [0, 0, 0, 128]],
            ["o": [0, 2], "b": 4, "s": 3, "c": [255, 0, 0, 255], "i": true],
        ]]
        p.apply(wireBatch([["op": "style", "id": 5, "style": style]]))
        func check(_ count: Int) throws -> CGRect {
            collection.layoutIfNeeded()
            XCTAssertEqual(collection.numberOfItems(inSection: 1), count)
            let casters = shadows(collection)
            XCTAssertEqual(casters.count, count == 0 ? 0 : 1)
            guard count > 0 else { return .zero }
            let caster = try XCTUnwrap(casters.first).caster
            let first = try XCTUnwrap(collection.layoutAttributesForItem(at: IndexPath(item: 0, section: 1)))
            let last = try XCTUnwrap(collection.layoutAttributesForItem(at: IndexPath(item: count - 1, section: 1)))
            let box = first.frame.union(last.frame)
            let path = try XCTUnwrap(caster.shadowPath)
            let drawn = caster.convert(path.boundingBoxOfPath, to: collection.layer)
            for (actual, expected) in [(drawn.minX, box.minX), (drawn.minY, box.minY), (drawn.width, box.width), (drawn.height, box.height)] {
                XCTAssertEqual(actual, expected, accuracy: 0.001)
            }
            XCTAssertTrue(path.contains(caster.convert(CGPoint(x: box.minX + 26, y: box.minY + 0.1), from: collection.layer)))
            XCTAssertFalse(path.contains(caster.convert(CGPoint(x: box.minX + 1, y: box.minY + 1), from: collection.layer)), "rounded, not rectangular")
            XCTAssertEqual(caster.shadowRadius, 7)
            XCTAssertEqual(caster.shadowOffset, CGSize(width: 0, height: 4))
            XCTAssertNotNil(caster.mask, "an outer shadow never fills the card's interior")
            XCTAssertEqual(assertStacking(collection, "\(count) rows"), 1)
            for cell in collection.visibleCells where collection.indexPath(for: cell)?.section == 1 {
                XCTAssertTrue(cell is UICollectionViewListCell, "rows remain UIKit list cells")
                let clip = try XCTUnwrap(cell.contentView.layer.sublayers?.first)
                XCTAssertTrue(clip.masksToBounds)
                XCTAssertTrue(clip.sublayers?.first is InsetShadowCaster, "the cell's slice of the inset shadow lies under its content")
            }
            return box
        }
        let two = try check(2)
        let removed = model.sections[1].rows.removeLast()
        host(p).sync()
        let one = try check(1)
        XCTAssertLessThan(one.height, two.height)
        model.sections[1].rows.append(removed)
        host(p).sync()
        XCTAssertEqual(try check(2), two)
        model.sections[1].rows.removeAll()
        host(p).sync()
        _ = try check(0)
        model = self.model(); model.sections[1].cardView = 5
        host(p).sync()
        _ = try check(2)
        p.apply(wireBatch([["op": "style", "id": 5, "style": ["border_radius": 26]]]))
        collection.layoutIfNeeded()
        XCTAssertTrue(shadows(collection).isEmpty, "removed with the authored shadow")
    }

    func testAnAuthoredSectionShadowPaintsOutsideItsNativeCard() throws {
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_GROUPED_CARD_PLAN"])
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "capturing the composited shadow needs a hosted key window")
        }
        window = UIWindow(windowScene: scene)
        window.frame = window.screen.bounds
        window.overrideUserInterfaceStyle = .light
        let root = UIViewController()
        window.rootViewController = root
        window.makeKeyAndVisible()
        ExactGroupedLists.install()
        let session = ExactApp.shared.makeSession(label: "grouped-card-shadow")
        defer { session.destroy(); window.isHidden = true }
        let view = ExactView(session: session)
        root.view.addSubview(view)
        view.frame = root.view.bounds
        XCTAssertNil(session.boot(plan: try Data(contentsOf: URL(fileURLWithPath: path)), size: view.bounds.size).error)
        waitForActivation(session)
        let p = session.presenter, agent = Agent(session: session)
        func node(_ name: String) throws -> NodeView {
            try XCTUnwrap(p.views.values.first { $0.props["testId"] == name }, name)
        }
        let collection = try XCTUnwrap(host(p).lists[try node("list").id]?.collection)
        func settle() {
            XCTAssertEqual(agent.clock(["settle": true])["settled"] as? Bool, true)
            window.layoutIfNeeded()
            collection.layoutIfNeeded()
        }
        func tap(_ name: String) throws {
            let reply = agent.tap(["id": Int(try node(name).id)])
            XCTAssertNil(reply["error"], "\(reply)")
            settle()
        }
        /// The window's composited pixels at points in the collection's coordinates.
        func pixels(_ points: [CGPoint]) throws -> [[UInt8]] {
            // CALayer.render omits the masked shadow. Capture UIKit's composited
            // hierarchy in its key window, including the space outside the card.
            let format = UIGraphicsImageRendererFormat()
            format.scale = 1; format.preferredRange = .standard
            let image = UIGraphicsImageRenderer(bounds: window.bounds, format: format).image { _ in
                XCTAssertTrue(window.drawHierarchy(in: window.bounds, afterScreenUpdates: true))
            }
            let cg = try XCTUnwrap(image.cgImage)
            var bytes = [UInt8](repeating: 0, count: cg.width * cg.height * 4)
            let context = try XCTUnwrap(CGContext(data: &bytes, width: cg.width, height: cg.height,
                bitsPerComponent: 8, bytesPerRow: cg.width * 4,
                space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
            context.draw(cg, in: CGRect(x: 0, y: 0, width: cg.width, height: cg.height))
            return points.map { local in
                let point = collection.convert(local, to: window)
                XCTAssertTrue(window.bounds.contains(point) && collection.bounds.contains(local))
                let at = (Int(point.y) * cg.width + Int(point.x)) * 4
                XCTAssertEqual(bytes[at + 3], 255, "sample a painted pixel")
                return Array(bytes[at..<at + 3])
            }
        }
        func pixelBelowCard() throws -> [UInt8] {
            let last = IndexPath(item: collection.numberOfItems(inSection: 0) - 1, section: 0)
            let box = try XCTUnwrap(collection.layoutAttributesForItem(at: last)).frame
            return try pixels([CGPoint(x: box.midX, y: box.maxY + 2)])[0]
        }
        settle()
        for (step, count) in [2, 3, 2].enumerated() {
            XCTAssertEqual(collection.numberOfItems(inSection: 0), count)
            let shaded = try pixelBelowCard()
            try tap("toggle-shadow")
            let unshaded = try pixelBelowCard()
            XCTAssertTrue(zip(shaded, unshaded).allSatisfy { Int($0) + 10 < Int($1) },
                "\(count) rows: shadow below the card: \(shaded) versus \(unshaded)")
            try tap("toggle-shadow")
            if step < 2 { try tap("toggle-row") }
        }
        // The hard red shadow's card scrolled just off the top: its shadow
        // still shows in the gap below it, and the next card covers it.
        XCTAssertEqual(collection.numberOfSections, 4)
        let card = try XCTUnwrap(collection.layoutAttributesForItem(at: IndexPath(item: 0, section: 2))).frame
        let next = try XCTUnwrap(collection.layoutAttributesForItem(at: IndexPath(item: 0, section: 3))).frame
        // The hard shadow is the card grown by its 40-point spread, 176 points lower.
        let cast = card.insetBy(dx: -40, dy: -40).offsetBy(dx: 0, dy: 176)
        XCTAssertTrue(cast.minY < next.minY - 8 && cast.maxY > next.minY + 8, "the shadow spans the gap's end and the next card's start")
        collection.contentOffset.y = card.maxY + 1
        window.layoutIfNeeded()
        collection.layoutIfNeeded()
        XCTAssertFalse(collection.indexPathsForVisibleItems.contains { $0.section == 2 }, "the card is off the top")
        // Below the status bar, whose edge effect blurs what scrolls under it.
        let gap = CGPoint(x: card.midX, y: next.minY - 8), row = CGPoint(x: next.midX, y: next.minY + 8)
        XCTAssertGreaterThan(collection.convert(gap, to: window).y, window.safeAreaInsets.top + 40)
        let sampled = try pixels([gap, row])
        XCTAssertTrue(sampled[0][0] > 240 && sampled[0][1] < 20 && sampled[0][2] < 20, "the shadow shows below the offscreen card: \(sampled[0])")
        XCTAssertTrue(sampled[1].allSatisfy { $0 > 245 }, "the next card covers the shadow: \(sampled[1])")
    }
}
#endif

#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

final class GroupedCardMacTests: XCTestCase {
    func testAHoverOnlyShadowLiftsAndRestoresTheCardsClip() throws {
        _ = NSApplication.shared
        let p = Presenter()
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "props": ["listStyle": "inset-grouped"]],
            ["op": "create", "id": 2, "kind": "view", "props": ["semanticTag": "section"],
             "style": ["looks": ["hover": ["box_shadow": [["o": [0, 8], "b": 0, "s": 0, "c": [255, 0, 0, 255]]]]]]],
            ["op": "create", "id": 3, "kind": "view",
             "style": ["overflow_x": "hidden", "overflow_y": "hidden", "border_radius": 10]],
            ["op": "create", "id": 4, "kind": "view"],
            ["op": "children", "id": 3, "ids": [4]],
            ["op": "children", "id": 2, "ids": [3]],
            ["op": "children", "id": 1, "ids": [2]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 3, "x": 16.0, "y": 20.0, "w": 300.0, "h": 80.0],
        ]))
        let section = try XCTUnwrap(p.views[2]), card = try XCTUnwrap(p.views[3])
        XCTAssertTrue(card.clipsToBounds)
        XCTAssertNil(card.shadowCaster)
        for _ in 0..<2 {
            section.lookState(hovered: true)
            XCTAssertEqual(card.shadowCaster?.shadowOffset.height, 8)
            XCTAssertFalse(card.clipsToBounds)
            XCTAssertFalse(try XCTUnwrap(card.layer).masksToBounds)
            let clip = try XCTUnwrap(card.clipBox)
            XCTAssertTrue(p.views[4]?.superview === clip)
            section.lookState(hovered: false)
            XCTAssertNil(card.shadowCaster)
            XCTAssertNil(card.clipBox)
            XCTAssertTrue(card.clipsToBounds)
            XCTAssertTrue(p.views[4]?.superview === card)
        }
    }
}
#endif
