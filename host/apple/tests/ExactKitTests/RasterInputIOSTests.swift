#if os(iOS)
import UIKit
import XCTest
import CExact
@testable import ExactKit
@testable import ExactImages

final class RasterInputIOSTests: XCTestCase {
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
