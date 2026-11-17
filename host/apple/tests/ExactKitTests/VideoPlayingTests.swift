#if os(macOS) || os(iOS)
import AVKit
import ObjectiveC
import XCTest
@testable import ExactKit

private var clockPlayer: AVPlayer?
private var clockStatus = AVPlayer.TimeControlStatus.paused
private var itemClock: CMTimebase?
private var observedIntervals: [Double] = []

extension AVPlayer {
    @objc fileprivate func playbackClockStatus() -> AVPlayer.TimeControlStatus {
        self === clockPlayer ? clockStatus : playbackClockStatus()
    }
    @objc fileprivate func playbackClockObserver(forInterval interval: CMTime, queue: DispatchQueue?, using block: @escaping (CMTime) -> Void) -> Any {
        observedIntervals.append(interval.seconds)
        return playbackClockObserver(forInterval: interval, queue: queue, using: block)
    }
}
extension AVPlayerItem {
    @objc fileprivate func playbackClockTimebase() -> CMTimebase? { itemClock ?? playbackClockTimebase() }
}

final class VideoPlayingTests: XCTestCase {
    private typealias Callback = @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, Int) -> Void
    private typealias Configuring = @convention(c) (UnsafeMutableRawPointer?, UnsafeMutableRawPointer?) -> Void
    private typealias Create = @convention(c) (
        UnsafeMutableRawPointer?, Callback?, Configuring?, UnsafeMutablePointer<UnsafeMutableRawPointer?>?
    ) -> UnsafeMutableRawPointer?
    private typealias View = @convention(c) (UnsafeMutableRawPointer?) -> UnsafeMutableRawPointer?
    private typealias Update = @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, Int) -> Void
    private typealias Destroy = @convention(c) (UnsafeMutableRawPointer?) -> Void
    private var events: [String] = []
    private var playingTimes: [Double] = []
    private var messages: [[String: Any]] = []

    func testPlayingARefusedPlaylistDoesNotReportAnotherError() throws {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        let library = try XCTUnwrap(VideoArmLibrary.handle)
        let create = unsafeBitCast(try XCTUnwrap(dlsym(library, "exact_video_create")), to: Create.self)
        let update = unsafeBitCast(try XCTUnwrap(dlsym(library, "exact_video_update")), to: Update.self)
        let destroy = unsafeBitCast(try XCTUnwrap(dlsym(library, "exact_video_destroy")), to: Destroy.self)
        let source = Bundle.module.url(forResource: "ColorFixtures", withExtension: nil)!
            .appendingPathComponent("master.m3u8").absoluteString
        for request in ["paused=false", "sessionPlay", "visible autoplay"] {
            let handle = try XCTUnwrap(create(Unmanaged.passUnretained(self).toOpaque(), { context, bytes, count in
                let test = Unmanaged<VideoPlayingTests>.fromOpaque(context!).takeUnretainedValue()
                test.messages.append(try! JSONSerialization.jsonObject(with: Data(bytes: bytes!, count: count)) as! [String: Any])
            }, nil, nil))
            defer { destroy(handle) }
            messages = []
            var props = ["src": source, "muted": "true", "playsinline": "true", "exactListeners": "error"]
            var autoplay = OffscreenAutoplay()
            if request == "visible autoplay" {
                props["autoplay"] = "true"
                autoplay.load(autoplay: true)
                props["paused"] = try XCTUnwrap(autoplay.visible(false)) ? "true" : "false"
            } else {
                props["paused"] = request == "paused=false" ? "false" : "true"
                if request == "sessionPlay" { props["mediaTitle"] = "Local playlist" }
            }
            func send() throws {
                let data = try JSONSerialization.data(withJSONObject: props)
                data.withUnsafeBytes { update(handle, $0.bindMemory(to: UInt8.self).baseAddress, data.count) }
            }
            try send()
            XCTAssertEqual(messages.filter { $0["event"] as? String == "error" }.count, 1, request)
            if request == "sessionPlay" {
                props["exactRemote"] = "1 play"
                try send()
                let result = messages.compactMap { $0["remoteResult"] as? [String: String] }.last
                XCTAssertEqual(result?["status"], "success")
                XCTAssertTrue(messages.contains { $0["remote"] as? String == "play" })
            } else if request == "visible autoplay" {
                props["paused"] = try XCTUnwrap(autoplay.visible(true)) ? "true" : "false"
                XCTAssertEqual(props["paused"], "false")
                try send()
            }
            waitForMainQueue()
            let errors = messages.filter { $0["event"] as? String == "error" }
            XCTAssertEqual(errors.count, 1, request)
            XCTAssertEqual(errors.first?["payload"] as? String, "src-not-supported", request)
        }
    }

    func testLocalPlaylistReportsAnErrorAndCanBeReplaced() throws {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        let library = try XCTUnwrap(VideoArmLibrary.handle)
        let create = unsafeBitCast(try XCTUnwrap(dlsym(library, "exact_video_create")), to: Create.self)
        let update = unsafeBitCast(try XCTUnwrap(dlsym(library, "exact_video_update")), to: Update.self)
        let destroy = unsafeBitCast(try XCTUnwrap(dlsym(library, "exact_video_destroy")), to: Destroy.self)
        let handle = try XCTUnwrap(create(Unmanaged.passUnretained(self).toOpaque(), { context, bytes, count in
            let test = Unmanaged<VideoPlayingTests>.fromOpaque(context!).takeUnretainedValue()
            test.messages.append(try! JSONSerialization.jsonObject(with: Data(bytes: bytes!, count: count)) as! [String: Any])
        }, nil, nil))
        defer { destroy(handle) }
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("local-playlist-\(UUID())", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let playlist = root.appendingPathComponent("master.m3u8")
        try "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=100000,RESOLUTION=64x64\nmedia.m3u8\n"
            .write(to: playlist, atomically: true, encoding: .utf8)
        let movie = Bundle.module.url(forResource: "ColorFixtures", withExtension: nil)!.appendingPathComponent("sdr.mp4")
        var props = ["src": playlist.absoluteString, "paused": "true", "muted": "true", "playsinline": "true",
                     "exactListeners": "error loadedmetadata"]
        func send() throws {
            let data = try JSONSerialization.data(withJSONObject: props)
            data.withUnsafeBytes { update(handle, $0.bindMemory(to: UInt8.self).baseAddress, data.count) }
        }
        for tag in ["video", "audio"] {
            props["semanticTag"] = tag
            props["src"] = playlist.absoluteString
            for (request, name) in ["master.m3u8", "master.m3u8", "uppercase.M3U8", "master.m3u"].enumerated() {
                let url = root.appendingPathComponent(name)
                if !FileManager.default.fileExists(atPath: url.path) { try FileManager.default.copyItem(at: playlist, to: url) }
                props["src"] = url.absoluteString
                messages = []
                if request == 1 { props["exactLoad"] = tag == "video" ? "1" : "2" }
                try send()
                waitFor("a local playlist's error", facts: { "\(self.messages.last ?? [:])" }) {
                    self.messages.contains { $0["event"] as? String == "error" }
                }
                let errors = messages.filter { $0["event"] as? String == "error" }
                let error = try XCTUnwrap(errors.first)
                XCTAssertEqual(errors.count, 1)
                XCTAssertEqual(error["payload"] as? String, "src-not-supported")
                let state = error["state"] as? [String: Any]
                XCTAssertEqual(state?["readyState"] as? Int, 0)
                XCTAssertNotNil(state?["error"] as? String)
            }
            messages = []
            props["src"] = movie.absoluteString
            try send()
            waitFor("the replacement movie's metadata") {
                self.messages.contains { $0["event"] as? String == "loadedmetadata" }
            }
            XCTAssertFalse(messages.contains { $0["event"] as? String == "error" })
        }
    }

    func testARealItemStartsAndResumesWithAnAdvancingClock() throws {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        let library = try XCTUnwrap(VideoArmLibrary.handle)
        let create = unsafeBitCast(try XCTUnwrap(dlsym(library, "exact_video_create")), to: Create.self)
        let update = unsafeBitCast(try XCTUnwrap(dlsym(library, "exact_video_update")), to: Update.self)
        let destroy = unsafeBitCast(try XCTUnwrap(dlsym(library, "exact_video_destroy")), to: Destroy.self)
        let handle = try XCTUnwrap(create(Unmanaged.passUnretained(self).toOpaque(), { context, bytes, count in
            let test = Unmanaged<VideoPlayingTests>.fromOpaque(context!).takeUnretainedValue()
            let message = try! JSONSerialization.jsonObject(with: Data(bytes: bytes!, count: count)) as! [String: Any]
            let event = message["event"] as! String
            test.events.append(event)
            if event == "playing", let state = message["state"] as? [String: Any] {
                test.playingTimes.append(state["currentTime"] as! Double)
            }
        }, nil, nil))
        defer { destroy(handle) }
        let source = Bundle.module.url(forResource: "ColorFixtures", withExtension: nil)!.appendingPathComponent("sdr.mp4").absoluteString
        var props = ["src": source, "paused": "false", "muted": "true", "playsinline": "true",
                     "automaticallyWaitsToMinimizeStalling": "false", "exactListeners": "play playing pause"]
        func send() throws {
            let data = try JSONSerialization.data(withJSONObject: props)
            data.withUnsafeBytes { update(handle, $0.bindMemory(to: UInt8.self).baseAddress, data.count) }
        }
        try send()
        waitFor("the real item's clock starting", facts: { "\(self.events)" }) { self.playingTimes.count == 1 }
        XCTAssertGreaterThan(try XCTUnwrap(playingTimes.first), 0)
        XCTAssertEqual(events.filter { ["play", "playing"].contains($0) }, ["play", "playing"])
        props["paused"] = "true"
        try send()
        waitFor("the real item pausing") { self.events.contains("pause") }
        props["paused"] = "false"
        try send()
        waitFor("the real item's clock resuming", facts: { "\(self.events)" }) { self.playingTimes.count == 2 }
        XCTAssertGreaterThan(playingTimes.last ?? 0, playingTimes.first ?? 0)
    }

    func testPlayingWaitsForTheItemClockAndRearmsAfterPauseOrStall() throws {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
        let library = try XCTUnwrap(VideoArmLibrary.handle)
        func symbol<T>(_ name: String, _: T.Type) throws -> T {
            unsafeBitCast(try XCTUnwrap(dlsym(library, "exact_video_" + name)), to: T.self)
        }
        let create = try symbol("create", Create.self), view = try symbol("view", View.self)
        let update = try symbol("update", Update.self), destroy = try symbol("destroy", Destroy.self)
        let handle = try XCTUnwrap(create(Unmanaged.passUnretained(self).toOpaque(), { context, bytes, count in
            let test = Unmanaged<VideoPlayingTests>.fromOpaque(context!).takeUnretainedValue()
            let message = try! JSONSerialization.jsonObject(with: Data(bytes: bytes!, count: count)) as! [String: Any]
            test.events.append(message["event"] as! String)
        }, nil, nil))
        var destroyed = false
        defer { if !destroyed { destroy(handle) } }
        let rawView = try XCTUnwrap(view(handle))
        #if os(macOS)
        let container = Unmanaged<NSView>.fromOpaque(rawView).takeUnretainedValue()
        let player = try XCTUnwrap(container.subviews.compactMap { ($0 as? AVPlayerView)?.player }.first)
        #else
        let container = Unmanaged<UIView>.fromOpaque(rawView).takeUnretainedValue()
        #endif
        let source = Bundle.module.url(forResource: "ColorFixtures", withExtension: nil)!.appendingPathComponent("sdr.mp4").absoluteString
        var props = ["src": source, "paused": "true", "muted": "true", "playsinline": "true",
                     "automaticallyWaitsToMinimizeStalling": "false", "exactListeners": "play playing pause waiting timeupdate"]
        func send() throws {
            let data = try JSONSerialization.data(withJSONObject: props)
            data.withUnsafeBytes { update(handle, $0.bindMemory(to: UInt8.self).baseAddress, data.count) }
        }
        func exchange(_ cls: AnyClass, _ a: Selector, _ b: Selector) throws -> () -> Void {
            let first = try XCTUnwrap(class_getInstanceMethod(cls, a)), second = try XCTUnwrap(class_getInstanceMethod(cls, b))
            method_exchangeImplementations(first, second)
            return { method_exchangeImplementations(first, second) }
        }
        let restoreObserver = try exchange(AVPlayer.self, #selector(AVPlayer.addPeriodicTimeObserver(forInterval:queue:using:)), #selector(AVPlayer.playbackClockObserver(forInterval:queue:using:)))
        defer { restoreObserver() }
        observedIntervals = []
        var timebase: CMTimebase?
        XCTAssertEqual(CMTimebaseCreateWithSourceClock(allocator: kCFAllocatorDefault, sourceClock: CMClockGetHostTimeClock(), timebaseOut: &timebase), noErr)
        let clock = try XCTUnwrap(timebase)
        itemClock = clock
        let restoreTimebase = try exchange(AVPlayerItem.self, #selector(getter: AVPlayerItem.timebase), #selector(AVPlayerItem.playbackClockTimebase))
        defer { restoreTimebase(); itemClock = nil }
        try send()
        #if os(iOS)
        let player = try XCTUnwrap(container.subviews.compactMap { ($0.layer as? AVPlayerLayer)?.player }.first)
        #endif
        waitFor("the item metadata") { player.currentItem?.status == .readyToPlay }
        clockPlayer = player; clockStatus = .paused
        let restoreStatus = try exchange(AVPlayer.self, #selector(getter: AVPlayer.timeControlStatus), #selector(AVPlayer.playbackClockStatus))
        defer { restoreStatus(); clockPlayer = nil }
        func status(_ value: AVPlayer.TimeControlStatus) {
            player.willChangeValue(forKey: "timeControlStatus")
            clockStatus = value
            player.didChangeValue(forKey: "timeControlStatus")
        }
        func rate(_ value: Double) {
            XCTAssertEqual(CMTimebaseSetRate(clock, rate: value), noErr)
            XCTAssertEqual(CMTimebaseNotificationBarrier(clock), noErr)
            waitForMainQueue()
        }
        events = []
        status(.playing) // AVPlayer's play request precedes the item clock.
        XCTAssertEqual(events.filter { $0 == "play" }, ["play"])
        XCTAssertFalse(events.contains("playing"), "play() must not report a stationary item as playing")
        XCTAssertEqual(CMTimebaseSetTime(clock, time: CMTime(seconds: 1, preferredTimescale: 600)), noErr)
        XCTAssertEqual(CMTimebaseNotificationBarrier(clock), noErr)
        waitForMainQueue()
        XCTAssertFalse(events.contains("playing"), "a time jump at zero rate is not playback")
        // Posting must return even while main is occupied in an AVKit call.
        let posted = DispatchSemaphore(value: 0)
        DispatchQueue.global().async {
            CMTimebaseSetRate(clock, rate: 1)
            CMTimebaseNotificationBarrier(clock)
            posted.signal()
        }
        XCTAssertEqual(posted.wait(timeout: .now() + 10), .success, "clock notifications must not wait for main")
        waitForMainQueue()
        XCTAssertEqual(events.filter { $0 == "playing" }.count, 1)
        XCTAssertEqual(CMTimebaseSetRate(clock, rate: 0), noErr)
        XCTAssertEqual(CMTimebaseNotificationBarrier(clock), noErr)
        XCTAssertEqual(CMTimebaseSetRate(clock, rate: 1), noErr)
        XCTAssertEqual(CMTimebaseNotificationBarrier(clock), noErr)
        waitForMainQueue()
        XCTAssertEqual(events.filter { $0 == "playing" }.count, 2, "a stop and resume queued before main drains must rearm playing")
        rate(2)
        XCTAssertEqual(events.filter { $0 == "playing" }.count, 2, "a speed change is not another start")
        let statusPosted = DispatchSemaphore(value: 0)
        DispatchQueue.global().async {
            status(.waitingToPlayAtSpecifiedRate)
            status(.playing)
            statusPosted.signal()
        }
        XCTAssertEqual(statusPosted.wait(timeout: .now() + 10), .success, "status notifications must not wait for main")
        waitForMainQueue()
        XCTAssertEqual(events.filter { $0 == "waiting" }.count, 1, "a queued status transition must retain its observed value")
        XCTAssertEqual(events.filter { $0 == "playing" }.count, 3)
        for stopped in [AVPlayer.TimeControlStatus.paused, .waitingToPlayAtSpecifiedRate] {
            rate(0); status(stopped)
            let before = events.filter { $0 == "playing" }.count
            status(.playing)
            XCTAssertEqual(events.filter { $0 == "playing" }.count, before)
            rate(1)
            XCTAssertEqual(events.filter { $0 == "playing" }.count, before + 1)
        }
        rate(0); status(.paused); status(.playing); status(.paused)
        let before = events.filter { $0 == "playing" }.count
        rate(1)
        XCTAssertEqual(events.filter { $0 == "playing" }.count, before, "pause cancels a pending start")
        XCTAssertEqual(observedIntervals, [0.25], "startup must not increase timeupdate's cadence")

        var replacement: CMTimebase?
        XCTAssertEqual(CMTimebaseCreateWithSourceClock(allocator: kCFAllocatorDefault, sourceClock: CMClockGetHostTimeClock(), timebaseOut: &replacement), noErr)
        let nextClock = try XCTUnwrap(replacement)
        itemClock = nextClock
        props["exactLoad"] = "1"
        try send()
        waitFor("the replacement item's metadata") { player.currentItem?.status == .readyToPlay }
        waitForMainQueue()
        events = []
        status(.playing)
        rate(0); rate(1)
        XCTAssertFalse(events.contains("playing"), "the replaced item's clock cannot start the new source")
        XCTAssertEqual(CMTimebaseSetRate(nextClock, rate: 1), noErr)
        XCTAssertEqual(CMTimebaseNotificationBarrier(nextClock), noErr)
        waitForMainQueue()
        XCTAssertEqual(events.filter { $0 == "playing" }, ["playing"])
        destroy(handle); destroyed = true
        events = []
        rate(0); rate(1)
        XCTAssertEqual(CMTimebaseSetRate(nextClock, rate: 0), noErr)
        XCTAssertEqual(CMTimebaseSetRate(nextClock, rate: 1), noErr)
        XCTAssertEqual(CMTimebaseNotificationBarrier(nextClock), noErr)
        waitForMainQueue()
        XCTAssertTrue(events.isEmpty, "a retired player cannot report a late clock change")
    }
}
#endif
