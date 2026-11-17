import AVFoundation
import XCTest
@testable import ExactKit
#if os(macOS)
import AppKit
#endif

final class VideoHatchTests: XCTestCase {
    private var supplied: [AVURLAsset] = []
    private var sources: [URL] = []
    private var moments: [UInt32] = []
    private var player: AVPlayer?
    private var firstPlayerMoment: Int?

    private func connect(_ session: ExactSession) {
        session.natives.install(module: Unmanaged.passUnretained(self).toOpaque())
        session.natives.hatchesConnected = true
        session.natives.elementCall = { context, event, _, platform, _, _ in
            let test = Unmanaged<VideoHatchTests>.fromOpaque(context!).takeUnretainedValue()
            test.moments.append(event)
            guard let platform else { return 0 }
            let object = Unmanaged<AnyObject>.fromOpaque(platform).takeUnretainedValue()
            if event == 3, let configuration = object as? NSMutableDictionary,
               let url = configuration["url"] as? URL {
                XCTAssertTrue(Thread.isMainThread)
                XCTAssertNil(test.player?.currentItem, "configuring precedes item construction")
                test.sources.append(url)
                let movie = Bundle.module.url(forResource: "ColorFixtures", withExtension: nil)!.appendingPathComponent("sdr.mp4")
                let asset = AVURLAsset(url: movie)
                test.supplied.append(asset)
                configuration["asset"] = asset
            } else if event != 2 {
                if test.firstPlayerMoment == nil, object is AVPlayer { test.firstPlayerMoment = test.moments.count - 1 }
                test.player = object as? AVPlayer
            }
            return 0
        }
    }

    func testConfiguringSuppliesEachItemsAssetBeforePlayback() throws {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        NativeViews.uninstallTable()
        let session = ExactApp.shared.makeSession(label: "video-configuring")
        defer { session.destroy(); NativeViews.uninstallTable() }
        connect(session)
        let p = session.presenter
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "video", "props": [
                "hatch": "clip", "src": "https://example.invalid/clip.m3u8", "paused": "false", "muted": "true",
                "playsinline": "true", "preferredMaximumResolution": "640x360", "preferredPeakBitRate": "900000",
                "startsOnFirstEligibleVariant": "true",
            ], "style": ["width": 200, "height": 100]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 200, "h": 100],
        ]))
        p.leaves.settle()
        XCTAssertEqual(sources.map(\.absoluteString), ["https://example.invalid/clip.m3u8"])
        let player = try XCTUnwrap(player, "the hatch receives the arm's player")
        waitFor("the supplied movie playing") { player.timeControlStatus == .playing }
        let item = try XCTUnwrap(player.currentItem)
        XCTAssertTrue(item.asset === supplied.first)
        XCTAssertEqual(item.preferredMaximumResolution, CGSize(width: 640, height: 360))
        XCTAssertEqual(item.preferredPeakBitRate, 900000)
        let node = try XCTUnwrap(p.views[1])
        node.video?.update()
        node.video?.layout()
        XCTAssertEqual(supplied.count, 1, "updates and layout keep the item")
        node.video?.command("load", seconds: 0)
        XCTAssertEqual(supplied.count, 2, "load configures a new item")
        waitFor("the reloaded item") { player.currentItem?.asset === self.supplied.last }
        p.apply(wireBatch([["op": "props", "id": 1, "set": ["src": "https://example.invalid/next.m3u8"]]]))
        XCTAssertEqual(sources.map(\.absoluteString), [
            "https://example.invalid/clip.m3u8", "https://example.invalid/clip.m3u8", "https://example.invalid/next.m3u8",
        ])
        waitFor("the changed source's supplied item") { player.currentItem?.asset === self.supplied.last }
        XCTAssertEqual(moments.filter { $0 == 3 }.count, 3)
        XCTAssertLessThan(try XCTUnwrap(moments.firstIndex(of: 3)), try XCTUnwrap(firstPlayerMoment))
    }

    func testARowsItemWaitsForHatchesAndKeepsItsAssetThroughFirstLayout() throws {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        NativeViews.uninstallTable()
        let session = ExactApp.shared.makeSession(label: "video-configuring-row")
        defer { session.destroy(); NativeViews.uninstallTable() }
        session.natives.hasAppModule = true
        let p = session.presenter
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "list", "style": ["appearance": "none"]],
            ["op": "create", "id": 2, "kind": "video", "props": [
                "hatch": "clip", "src": "https://example.invalid/row.m3u8", "muted": "true", "playsinline": "true",
                "startsOnFirstEligibleVariant": "true",
            ], "style": ["width": 200, "height": 100]],
            ["op": "children", "id": 1, "ids": [2]],
            ["op": "roots", "ids": [1]],
        ]))
        p.leaves.settle()
        let node = try XCTUnwrap(p.views[2])
        let armPlayer = try XCTUnwrap(ElementHatches.platform(of: node, p) as? AVPlayer)
        XCTAssertNil(armPlayer.currentItem, "the item waits for the app's configuring hatch")
        connect(session)
        p.elements.replay()
        XCTAssertEqual(supplied.count, 1)
        node.frame = CGRect(x: 0, y: 0, width: 200, height: 100)
        node.video?.layout()
        waitFor("the row's supplied asset after first layout") { armPlayer.currentItem != nil }
        XCTAssertTrue(armPlayer.currentItem?.asset === supplied.first)
        XCTAssertTrue(player === armPlayer)
        waitForMainQueue()
        XCTAssertEqual(supplied.count, 1, "first layout and its fallback make only one item")
    }
}
