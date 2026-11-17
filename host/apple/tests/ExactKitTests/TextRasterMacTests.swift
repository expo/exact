#if os(macOS)
import AppKit
import IOSurface
import XCTest
@testable import ExactKit

final class TextRasterMacTests: XCTestCase {
    private var session: ExactSession!
    private var window: NSWindow!

    private func fixture() -> Presenter {
        _ = NSApplication.shared
        session = ExactApp.shared.makeSession(label: "text-kept-mac")
        let p = session.presenter
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 300),
                          styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.viewport
        let scale = p.viewport.pixelScale ?? 1
        let width = 480 / Double(scale), height = 48 / Double(scale)
        RegionTextExecutor.queue.isSuspended = true
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "text", "props": ["text": "Repeated label"]],
            ["op": "create", "id": 3, "kind": "text", "props": ["text": "Repeated label"]],
            ["op": "children", "id": 1, "ids": [2, 3]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0.0, "y": 0.0, "w": 400.0, "h": 2000.0],
            ["op": "frame", "id": 2, "x": 0.0, "y": 500.0, "w": width, "h": height],
            ["op": "frame", "id": 3, "x": 0.0, "y": 600.0, "w": width, "h": height],
        ]))
        p.textRasters.slotFreed = nil
        return p
    }

    override func tearDown() {
        session?.presenter.reset()
        RegionTextExecutor.queue.isSuspended = false
        RegionTextExecutor.queue.waitUntilAllOperationsAreFinished()
        waitForMainQueue()
        window?.close()
        session?.destroy()
        window = nil; session = nil
        super.tearDown()
    }

    func testRepeatedLabelsReuseACompletedSurfaceWithoutAWorker() throws {
        let p = fixture()
        let first = try XCTUnwrap(p.views[2]), next = try XCTUnwrap(p.views[3])
        XCTAssertTrue(first.canRasterText)
        XCTAssertTrue(p.textRasters.ensure(first, urgent: true))
        let pixels = try XCTUnwrap(first.textRaster)
        XCTAssertTrue(p.textRasters.ensure(next, urgent: false))
        XCTAssertTrue(next.textRasterReady, "a completed label needs no worker")
        XCTAssertTrue(next.textRaster === pixels, "layers share immutable pixels")
    }

    private func pixels(_ surface: IOSurface) -> Data {
        CapturePixels.bytes(TextRasterImage(surface, frame: .zero, covered: .zero, headroom: 0))
    }

    private func paint(_ node: NodeView, with rasters: TextRasterizer, spec: Spec? = nil) throws -> IOSurface {
        node.dropTextRaster()
        node.textLayoutValid = false
        if let spec { node.cachedTextSpec = spec }
        XCTAssertTrue(rasters.ensure(node, urgent: true))
        return try XCTUnwrap(node.textRaster)
    }

    func testRenderingInputsAndFontNamespacesCannotReuseStalePixels() throws {
        let p = fixture(), rasters = TextRasterizer()
        let node = try XCTUnwrap(p.views[2]), engine = try XCTUnwrap(node.text)
        let original = node.paragraphSpec(), size = node.frame.size
        let first = try paint(node, with: rasters)
        var variants: [Spec] = []
        var changed = original; changed.runs[0].text = "Different label"; variants.append(changed)
        changed = original; changed.runs[0].size += 2; variants.append(changed)
        changed = original; changed.runs[0].weight = 700; variants.append(changed)
        changed = original; changed.runs[0].family = 1; variants.append(changed)
        changed = original; changed.color = [255, 0, 0, 255]; variants.append(changed)
        changed = original; changed.align = 2; variants.append(changed)
        changed = original; changed.lineClamp = 1; variants.append(changed)
        changed = original; changed.ellipsis = true; variants.append(changed)
        changed = original; changed.antialiased.toggle(); variants.append(changed)
        for spec in variants {
            let image = try paint(node, with: rasters, spec: spec)
            XCTAssertFalse(image === first)
        }
        XCTAssertTrue(try paint(node, with: rasters, spec: original) === first)
        node.frame.size.width += 10
        XCTAssertFalse(try paint(node, with: rasters) === first, "wrapping width")
        node.frame.size = size
        node.kernelContentBox = node.bounds.insetBy(dx: 4, dy: 0)
        XCTAssertFalse(try paint(node, with: rasters, spec: original) === first, "content box")
        node.kernelContentBox = nil
        p.viewport.magnification = 1.25
        XCTAssertFalse(try paint(node, with: rasters, spec: original) === first, "pixel scale")
        p.viewport.magnification = 1
        engine.install(nil)
        let catalog = try paint(node, with: rasters, spec: original)
        XCTAssertFalse(catalog === first, "a family index may now name another font")
        engine.setLineScale(engine.lineScale == 1 ? 2 : 1)
        XCTAssertFalse(try paint(node, with: rasters, spec: original) === catalog, "line-box rounding")
    }

    func testWorkersKeepCompletedPixelsAndCannotReseedAnOldNamespace() throws {
        let p = fixture(), rasters = p.textRasters
        let first = try XCTUnwrap(p.views[2]), next = try XCTUnwrap(p.views[3])
        XCTAssertTrue(rasters.ensure(first, urgent: false))
        RegionTextExecutor.queue.isSuspended = false
        RegionTextExecutor.queue.waitUntilAllOperationsAreFinished()
        waitForMainQueue()
        RegionTextExecutor.queue.isSuspended = true
        let rendered = try XCTUnwrap(first.textRaster)
        next.dropTextRaster()
        XCTAssertTrue(rasters.ensure(next, urgent: false))
        XCTAssertTrue(next.textRasterReady)
        XCTAssertTrue(next.textRaster === rendered)

        let old = rasters
        let engine = try XCTUnwrap(first.text)
        engine.install(nil)
        first.dropTextRaster()
        XCTAssertTrue(old.ensure(first, urgent: false))
        engine.install(nil)
        let current = try paint(next, with: old)
        RegionTextExecutor.queue.isSuspended = false
        RegionTextExecutor.queue.waitUntilAllOperationsAreFinished()
        waitForMainQueue()
        RegionTextExecutor.queue.isSuspended = true
        XCTAssertNil(first.textRaster, "an obsolete font worker cannot publish")
        XCTAssertTrue(old.ensure(first, urgent: false))
        XCTAssertTrue(first.textRaster === current, "the old worker cannot replace current cached pixels")
    }

    func testVisibleReplacementCanReusePixelsWhileWorkersAreBusy() throws {
        let p = fixture(), rasters = TextRasterizer()
        let first = try XCTUnwrap(p.views[2]), next = try XCTUnwrap(p.views[3])
        let original = first.paragraphSpec()
        let image = try paint(first, with: rasters)
        var changed = original; changed.runs[0].text = "Another label"
        _ = try paint(next, with: rasters, spec: changed)
        first.dropTextRaster()
        changed.runs[0].text = "Busy worker"
        first.cachedTextSpec = changed
        XCTAssertTrue(rasters.ensure(first, urgent: false))
        next.cachedTextSpec = original
        next.textRasterKey = nil
        next.frame.origin.y = 80
        next.textRasterGeometryChanged()
        let queued = RegionTextExecutor.queue.operationCount
        XCTAssertFalse(rasters.replaceVisible([next], wait: 0))
        XCTAssertTrue(next.textRaster === image)
        XCTAssertTrue(next.textRasterReady)
        XCTAssertEqual(RegionTextExecutor.queue.operationCount, queued)
    }

    func testKeptSurfacesEvictColdEntriesAndRetainFrequentlyUsedOnes() throws {
        let p = fixture(), rasters = TextRasterizer()
        let node = try XCTUnwrap(p.views[2]), hot = try XCTUnwrap(p.views[3])
        let original = node.paragraphSpec()
        let hotPixels = try paint(hot, with: rasters)
        var coldSpec = original; coldSpec.runs[0].text = "Cold label"
        _ = try paint(node, with: rasters, spec: coldSpec)
        var total = 0
        for i in 0..<128 {
            var spec = original; spec.runs[0].text = "Unique label \(i)"
            total += try paint(node, with: rasters, spec: spec).allocationSize
            hot.dropTextRaster()
            XCTAssertTrue(rasters.ensure(hot, urgent: false))
            XCTAssertTrue(hot.textRaster === hotPixels)
        }
        XCTAssertGreaterThan(total, 4 * 1024 * 1024)
        node.dropTextRaster(); node.cachedTextSpec = coldSpec
        XCTAssertTrue(rasters.ensure(node, urgent: false))
        XCTAssertFalse(node.textRasterReady, "the cold entry was evicted")
        hot.frame.origin.y = 40
        hot.presentTextRaster()
        XCTAssertTrue(hot.layer?.contents as? IOSurface === hotPixels, "eviction cannot mutate live pixels")
    }

    func testLargeRastersAndLongKeysAreNotKeptAndSmallTextStillDraws() throws {
        let p = fixture(), rasters = TextRasterizer()
        let node = try XCTUnwrap(p.views[2])
        let spec = node.paragraphSpec(), size = node.frame.size
        node.frame.size.height *= 16
        let large = try paint(node, with: rasters)
        XCTAssertGreaterThan(large.allocationSize, 256 * 1024)
        XCTAssertFalse(try paint(node, with: rasters) === large)
        node.frame.size = size
        var long = spec
        long.runs[0].text = String(repeating: "word ", count: 60_000)
        long.lineClamp = 1
        let clipped = try paint(node, with: rasters, spec: long)
        XCTAssertFalse(try paint(node, with: rasters, spec: long) === clipped, "the key's text counts too")
        node.dropTextRaster(); node.cachedTextSpec = spec
        node.frame.size = CGSize(width: 20, height: 20)
        XCTAssertTrue(node.textIsSmall)
        XCTAssertFalse(node.canRasterText)
        XCTAssertTrue(rasters.ensure(node, urgent: true))
        XCTAssertNil(node.textRaster)
    }

    func testScrollingRepeatedLabelsReusesPixelsWithoutQueuingRasters() throws {
        let p = fixture()
        let size = try XCTUnwrap(p.views[2]).bounds.size
        let ids = Array(10..<110), step = size.height + 10
        let rows: [[String: Any]] = ids.enumerated().flatMap { index, id in [
            ["op": "create", "id": id, "kind": "text",
             "props": ["text": index % 2 == 0 ? "Repeated label" : "Another label"],
             "style": ["text_color": [0.0, 0, 0, 255]]],
            ["op": "frame", "id": id, "x": 0.0, "y": Double(index) * Double(step),
             "w": Double(size.width), "h": Double(size.height)],
        ] }
        p.apply(wireBatch([
            ["op": "create", "id": 4, "kind": "list", "style": ["overflow_y": "scroll"]],
        ] + rows + [
            ["op": "children", "id": 4, "ids": ids],
            ["op": "children", "id": 1, "ids": [4]],
            ["op": "frame", "id": 4, "x": 0.0, "y": 0.0, "w": 380.0, "h": 280.0],
        ]))
        let rasters = p.textRasters
        let nodes = try ids.map { try XCTUnwrap(p.views[UInt32($0)]) }
        let references = try nodes.prefix(2).map { try paint($0, with: rasters) }
        let expected = references.map(pixels)
        XCTAssertTrue(expected.allSatisfy { CapturePixels.hasPaint($0) })
        let queued = RegionTextExecutor.queue.operationCount
        let scroll = try XCTUnwrap(p.views[4]?.scroll)
        var reused = 0
        for (index, node) in nodes.enumerated().dropFirst(2) {
            scroll.contentView.scroll(to: CGPoint(x: 0, y: CGFloat(index) * step))
            scroll.reflectScrolledClipView(scroll.contentView)
            XCTAssertTrue(rasters.ensure(node, urgent: false))
            let surface = try XCTUnwrap(node.textRaster)
            if surface === references[index % 2] { reused += 1 }
            node.presentTextRaster()
            let shown = node.textRasterOverflowLayer?.contents ?? node.layer?.contents
            XCTAssertTrue(shown as? IOSurface === surface)
            XCTAssertEqual(pixels(surface), expected[index % 2])
        }
        XCTAssertEqual(reused, 98, "only the first two labels need rasterization")
        XCTAssertEqual(RegionTextExecutor.queue.operationCount, queued, "cache hits submit no worker jobs")
        XCTAssertGreaterThan(scroll.contentView.bounds.minY, 0)
    }
}
#endif
