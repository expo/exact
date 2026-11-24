#if os(iOS)
import UIKit
import XCTest
import CExact
@testable import ExactKit
@testable import ExactImages

final class RasterInputIOSTests: XCTestCase {
    func testBundledPictureRemountsInTheMountBatchAfterRest() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the JPEG is installed inside the hosted test app") }
        let source = try bundledPicture()
        let session = ExactApp.shared.makeSession(label: "bundled-raster-remount")
        let presenter = session.presenter
        let window = show(presenter, in: scene)
        defer { session.destroy(); window.isHidden = true }
        let loader = try XCTUnwrap(session.rasters as? RasterLoader)

        mountPictures([(1, 64)], source: source, in: presenter)
        waitFor("the bundled JPEG decodes") {
            presenter.views[1]?.raster != nil && loader.diagnostics["decoded"] as? Int == 1
        }
        let picture = ObjectIdentifier(try XCTUnwrap(presenter.views[1]?.raster?.image.image))
        XCTAssertEqual(loader.diagnostics["metadataReads"] as? Int, 1)
        presenter.apply(wireBatch([["op": "destroy", "id": 1]]))
        XCTAssertEqual(exact_raster_stats(loader.id).cold_entries, 1)
        loader.trimCold()

        mountPictures([(2, 64)], source: source, in: presenter)
        let contents = try XCTUnwrap(presenter.views[2]?.imageContentLayer?.contents as AnyObject?,
                                     "the mount batch paints the picture before yielding")
        XCTAssertEqual(ObjectIdentifier(contents), picture)
        XCTAssertEqual(loader.diagnostics["metadataReads"] as? Int, 1)
        XCTAssertEqual(loader.diagnostics["decoded"] as? Int, 1)
    }

    func testRestKeepsBundledPicturesWithinCapAndPressureEvictsThem() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "the JPEG is installed inside the hosted test app") }
        let source = try bundledPicture()
        let session = ExactApp.shared.makeSession(label: "bundled-raster-cap")
        let presenter = session.presenter
        let window = show(presenter, in: scene)
        defer { session.destroy(); window.isHidden = true }
        let loader = try XCTUnwrap(session.rasters as? RasterLoader)

        mountPictures([(1, 16), (2, 80), (3, 160)], source: source, in: presenter)
        waitFor("three sizes of the bundled JPEG decode") {
            (1...3).allSatisfy { presenter.views[UInt32($0)]?.raster != nil }
        }
        let older = try XCTUnwrap(presenter.views[1]?.raster as? NativeRasterLease).decoded.residentBytes
        let newer = try XCTUnwrap(presenter.views[2]?.raster as? NativeRasterLease).decoded.residentBytes
        XCTAssertLessThan(older, newer)
        presenter.apply(wireBatch([["op": "destroy", "id": 1], ["op": "destroy", "id": 2]]))
        let before = exact_raster_stats(loader.id)
        XCTAssertEqual(before.cold_entries, 2)
        XCTAssertEqual(before.cold_bytes, UInt64(older + newer))
        XCTAssertGreaterThan(before.pinned_bytes, 0)

        loader.trimCold()
        let atRest = exact_raster_stats(loader.id)
        XCTAssertEqual(atRest.cold_entries, 2, "both cold sizes remain reusable while another size is live")
        XCTAssertEqual(atRest.cold_bytes, before.cold_bytes)
        XCTAssertLessThanOrEqual(atRest.cold_bytes, loader.budget * 3 / 8)

        loader.trimCold(keeping: UInt64(newer))
        let capped = exact_raster_stats(loader.id)
        XCTAssertEqual(capped.cold_entries, 1)
        XCTAssertEqual(capped.cold_bytes, UInt64(newer), "the byte cap evicts the older size first")
        XCTAssertLessThanOrEqual(capped.resident_bytes + capped.reserved_bytes, loader.budget)

        loader.trimCold(keeping: 0)
        let pressured = exact_raster_stats(loader.id)
        XCTAssertEqual(pressured.cold_entries, 0)
        XCTAssertEqual(pressured.cold_bytes, 0)
        XCTAssertEqual(pressured.pinned_bytes, before.pinned_bytes)
        XCTAssertNotNil(presenter.views[3]?.imageContentLayer?.contents)

        presenter.apply(wireBatch([["op": "destroy", "id": 3]]))
        loader.trimCold(keeping: 0)
        XCTAssertEqual(exact_raster_stats(loader.id).cold_entries, 0)
        XCTAssertEqual(loader.diagnostics["sources"] as? Int, 0)
    }

    private func bundledPicture() throws -> String {
        let resolver = try XCTUnwrap(ExactApp.shared.resolver)
        let file = try XCTUnwrap(Bundle.module.url(forResource: "shop-c2ci", withExtension: "jpg", subdirectory: "ColorFixtures"))
            .standardizedFileURL.resolvingSymlinksInPath()
        let prefix = resolver.root.standardizedFileURL.resolvingSymlinksInPath().path + "/"
        XCTAssertTrue(file.path.hasPrefix(prefix), "the test resource is inside the asset root")
        let source = String(file.path.dropFirst(prefix.count))
        XCTAssertEqual(resolver.url(source), file)
        return source
    }

    private func show(_ presenter: Presenter, in scene: UIWindowScene) -> UIWindow {
        let window = UIWindow(windowScene: scene)
        let controller = UIViewController()
        window.rootViewController = controller
        window.makeKeyAndVisible()
        presenter.viewport.frame = window.bounds
        controller.view.addSubview(presenter.viewport)
        return window
    }

    private func mountPictures(_ pictures: [(id: Int, size: Int)], source: String, in presenter: Presenter) {
        let create: [[String: Any]] = pictures.map { ["op": "create", "id": $0.id, "kind": "image", "props": ["imageSource": source]] }
        let roots: [String: Any] = ["op": "roots", "ids": pictures.map(\.id)]
        let frames: [[String: Any]] = pictures.map { ["op": "frame", "id": $0.id, "x": 0, "y": 0, "w": $0.size, "h": $0.size] }
        presenter.apply(wireBatch(create + [roots] + frames))
    }

    func testAPictureSurvivesAPopAndPushUntilMemoryPressure() throws {
        let presenter = Presenter(), loader = RasterLoader()
        let resolver = AssetResolver(root: Bundle.main.bundleURL)
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 300, height: 300))
        presenter.viewport.frame = window.bounds
        window.addSubview(presenter.viewport); window.makeKeyAndVisible()
        defer { loader.shutdown(); window.isHidden = true }
        let format = UIGraphicsImageRendererFormat(); format.scale = 1
        let data = UIGraphicsImageRenderer(size: CGSize(width: 32, height: 24), format: format).pngData { context in
            UIColor.red.setFill(); context.fill(CGRect(x: 0, y: 0, width: 32, height: 24))
        }
        let name = "data:image/png;base64," + data.base64EncodedString()
        var picture: ObjectIdentifier?
        for generation in 1...3 {
            let node = NodeView(id: 1, kind: "image", presenter: presenter)
            presenter.views[node.id] = node; presenter.viewport.addSubview(node)
            node.frame = CGRect(x: 0, y: 0, width: 32, height: 24)
            node.loadGeneration = generation
            loader.load(node, source: name, resolver: resolver)
            waitFor("picture decoded or reused") { node.raster != nil }
            XCTAssertEqual(node.raster?.image.naturalSize, CGSize(width: 32, height: 24))
            let identity = ObjectIdentifier(try XCTUnwrap(node.raster?.image))
            if generation == 2 { XCTAssertEqual(identity, picture) }
            picture = identity
            XCTAssertEqual(loader.diagnostics["decoded"] as? Int, generation == 3 ? 2 : 1)
            XCTAssertEqual(loader.testAnnounced.count, generation)
            XCTAssertTrue(loader.testAnnounced.allSatisfy { $0.failure == nil })
            loader.cancel(node.id); node.raster = nil
            node.removeFromSuperview(); presenter.views.removeValue(forKey: node.id)
            if generation == 2 {
                loader.trimCold(keeping: 0)
                XCTAssertEqual(exact_raster_stats(loader.id).cold_entries, 0)
            } else if generation == 1 {
                loader.trimCold()
                XCTAssertEqual(exact_raster_stats(loader.id).cold_entries, 1)
                XCTAssertEqual(exact_raster_stats(loader.id).evicted, 0)
            }
        }
    }

    func testMemoryPressureRemovesColdFilesOffMainAndKeepsLiveInputs() throws {
        XCTAssertTrue(Thread.isMainThread)
        let presenter = Presenter(), loader = RasterLoader()
        let resolver = AssetResolver(root: Bundle.main.bundleURL)
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 300, height: 300))
        presenter.viewport.frame = window.bounds
        window.addSubview(presenter.viewport); window.makeKeyAndVisible()
        defer {
            RasterInput.testDidRemove = nil
            loader.shutdown(); window.isHidden = true
        }
        let data = UIGraphicsImageRenderer(size: CGSize(width: 8, height: 8)).pngData { context in
            UIColor.red.setFill(); context.fill(CGRect(x: 0, y: 0, width: 8, height: 8))
        }
        let name = "data:image/png;base64," + data.base64EncodedString()
        let live = try RasterInput.open(name, resolver: resolver)
        var nodes: [NodeView] = []
        for id in 1...3 {
            let node = NodeView(id: UInt32(id), kind: "image", presenter: presenter)
            presenter.views[node.id] = node; presenter.viewport.addSubview(node)
            node.frame = CGRect(x: 0, y: id * 20, width: 8, height: 8)
            node.loadGeneration = 1
            loader.load(node, source: name + String(repeating: " ", count: id), resolver: resolver)
            nodes.append(node)
        }
        waitFor("all three images decoded") { nodes.allSatisfy { $0.raster != nil } }
        let removed = expectation(description: "all cold spool files removed")
        removed.expectedFulfillmentCount = nodes.count
        RasterInput.testDidRemove = { url in
            XCTAssertFalse(Thread.isMainThread, "trim must not unlink files on the UI thread")
            XCTAssertNotEqual(url, live.url)
            XCTAssertFalse(FileManager.default.fileExists(atPath: url.path))
            removed.fulfill()
        }
        for node in nodes { loader.cancel(node.id); node.raster = nil }
        loader.trimCold(keeping: 0)
        wait(for: [removed], timeout: 10)
        RasterInput.testDidRemove = nil
        XCTAssertEqual(try live.bytes(), data)
        let replacement = try RasterInput.open(name, resolver: resolver)
        XCTAssertNotEqual(replacement.url, live.url, "each spool owns a unique path")
        XCTAssertEqual(try replacement.bytes(), data)
        withExtendedLifetime((live, replacement)) {}
    }
}
#endif
