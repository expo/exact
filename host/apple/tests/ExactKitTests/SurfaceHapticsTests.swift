// @ref LLP 1009 D10: a GPU surface's haptic messages are consumed, never
// dispatched; a texture starts, moves and stops by its surface's messages,
// stops when its surface is destroyed and while the canvases are hidden or
// interrupted, and starts again at the next message; a pattern plays beside
// it. On iOS the Core Haptics patterns they make are well formed.
#if os(iOS)
import CoreHaptics
#endif
import XCTest
@testable import ExactKit
@testable import ExactSurfaces

private final class Recorder: HapticPlayer {
    var heard: [String] = []
    func texture(_ surface: UInt32, intensity: Float, sharpness: Float) { heard.append("texture \(surface) \(intensity) \(sharpness)") }
    func stopTexture(_ surface: UInt32) { heard.append("stop \(surface)") }
    func play(_ events: [SurfaceHaptic.Event], _ curves: [SurfaceHaptic.Curve]) { heard.append("pattern \(events.count) \(curves.count)") }
}

final class SurfaceHapticsTests: XCTestCase {
    override class func setUp() { super.setUp(); ExactSurfaces.install() }
    private let texture = #"exact:haptics {"texture":{"intensity":0.5,"sharpness":0.25}}"#
    private let stop = #"exact:haptics {"texture":null}"#
    private let pattern = #"exact:haptics {"pattern":{"events":[{"type":"transient","time":0,"intensity":1,"sharpness":0.75},{"type":"continuous","time":0.05,"duration":0.2,"intensity":0.5,"sharpness":0}],"curves":[{"parameter":"intensity","points":[[0,1],[0.25,0]]}]}}"#

    func testMessagesParseAsExactGpuWritesThem() {
        XCTAssertEqual(SurfaceHaptic.parse(texture), .texture(intensity: 0.5, sharpness: 0.25))
        XCTAssertEqual(SurfaceHaptic.parse(stop), .stop)
        XCTAssertEqual(SurfaceHaptic.parse(pattern), .pattern(events: [
            .init(transient: true, time: 0, duration: 0, intensity: 1, sharpness: 0.75),
            .init(transient: false, time: 0.05, duration: 0.2, intensity: 0.5, sharpness: 0),
        ], curves: [.init(sharpness: false, points: [[0, 1], [0.25, 0]])]))
        XCTAssertNil(SurfaceHaptic.parse("exact:audio"))
    }

    func testATextureFollowsItsMessagesAndItsSurface() {
        let recorder = Recorder(), haptics = SurfaceHaptics(player: recorder)
        XCTAssertFalse(haptics.take(7, "hello"), "an app message is the app's")
        XCTAssertTrue(haptics.take(7, texture))
        XCTAssertTrue(haptics.take(7, #"exact:haptics {"texture":{"intensity":1,"sharpness":0}}"#))
        XCTAssertTrue(haptics.take(8, pattern))
        XCTAssertTrue(haptics.take(7, stop))
        XCTAssertTrue(haptics.take(7, stop), "a second stop is nothing")
        XCTAssertTrue(haptics.take(7, "exact:haptics {not json"), "a malformed one is consumed too")
        XCTAssertEqual(recorder.heard, ["texture 7 0.5 0.25", "texture 7 1.0 0.0", "pattern 2 1", "stop 7"])
        recorder.heard = []
        XCTAssertTrue(haptics.take(7, texture))
        haptics.remove(7)
        haptics.remove(7)
        XCTAssertEqual(recorder.heard, ["texture 7 0.5 0.25", "stop 7"])
    }

    func testNothingPlaysWhileSuspendedAndTheNextMessageStartsItAgain() {
        let recorder = Recorder(), haptics = SurfaceHaptics(player: recorder)
        _ = haptics.take(1, texture)
        _ = haptics.take(2, texture)
        recorder.heard = []
        haptics.suspended = true
        XCTAssertEqual(Set(recorder.heard), ["stop 1", "stop 2"])
        recorder.heard = []
        _ = haptics.take(1, texture)
        _ = haptics.take(1, pattern)
        XCTAssertEqual(recorder.heard, [], "hidden or interrupted: no texture, no pattern")
        haptics.suspended = false
        _ = haptics.take(1, texture)
        XCTAssertEqual(recorder.heard, ["texture 1 0.5 0.25"])
    }

    func testAnInterruptionOfTheCanvasesStopsTheirTextures() {
        let appSession = ExactApp.shared.makeSession(label: "surface-haptics")
        defer { appSession.destroy() }
        let host = appSession.surfaceHost
        let recorder = Recorder()
        host.haptics = SurfaceHaptics(player: recorder)
        let lifecycle = host.lifecycle
        _ = host.haptics.take(3, texture)
        lifecycle.interruption(began: true, shouldResume: false)
        XCTAssertEqual(recorder.heard, ["texture 3 0.5 0.25", "stop 3"])
        _ = host.haptics.take(3, texture)
        XCTAssertEqual(recorder.heard.count, 2, "nothing starts while interrupted")
        // Its end leaves them stopped only while the canvases are hidden (a test host may have no window).
        lifecycle.interruption(began: false, shouldResume: true)
        XCTAssertEqual(host.haptics.suspended, lifecycle.hidden)
    }

    /// Two modules' first canvases both have surface id 1: each canvas's
    /// haptics go by its view, so one going stops only its own texture.
    func testTwoModulesCanvasesKeepTheirOwnTextures() {
        let appSession = ExactApp.shared.makeSession(label: "surface-haptics-modules")
        defer { appSession.destroy() }
        let host = appSession.surfaceHost
        let recorder = Recorder()
        host.haptics = SurfaceHaptics(player: recorder)
        host.haptics.suspended = false
        for view in [UInt32(100), 101] {
            let e = CanvasesHost.Entry(view: NodeView(id: view, kind: "canvas", presenter: appSession.presenter),
                                       name: view == 100 ? "sky" : "game", values: [Any]())
            e.id = 1
            host.entries[view] = e
            _ = host.haptics.take(view, texture)
        }
        host.destroy(view: 100)
        XCTAssertEqual(recorder.heard, ["texture 100 0.5 0.25", "texture 101 0.5 0.25", "stop 100"])
        XCTAssertEqual(host.haptics.playing, [101])
        host.destroy(view: 101)
    }

    #if os(iOS)
    func testAStoppedOrResetEngineStartsAgainAtTheNextRequest() {
        final class Engine: HapticEngine {
            var stoppedHandler: CHHapticEngine.StoppedHandler = { _ in }
            var resetHandler: CHHapticEngine.ResetHandler = {}
            var starts = 0, textures = 0, patterns = 0
            func start() throws { starts += 1 }
            func texture() throws -> HapticTexture { textures += 1; return Texture() }
            func play(_ pattern: CHHapticPattern) throws { patterns += 1 }
        }
        final class Texture: HapticTexture {
            func send(_ parameters: [CHHapticDynamicParameter]) throws {}
            func stop() {}
        }
        func settle() { RunLoop.main.run(until: Date().addingTimeInterval(0.05)) }
        let engine = Engine(), player = CoreHapticsPlayer(make: { engine })
        player.texture(1, intensity: 0.5, sharpness: 0.5)
        player.texture(1, intensity: 0.7, sharpness: 0.5)
        XCTAssertEqual([engine.starts, engine.textures], [1, 1], "one start, one texture while it runs")
        // An audio session interruption stops the engine; its players are gone.
        engine.stoppedHandler(.audioSessionInterrupt)
        settle()
        player.texture(1, intensity: 0.5, sharpness: 0.5)
        XCTAssertEqual([engine.starts, engine.textures], [2, 2], "started again, its texture made again")
        engine.resetHandler()
        settle()
        player.play([.init(transient: true, time: 0, duration: 0, intensity: 1, sharpness: 1)], [])
        XCTAssertEqual([engine.starts, engine.patterns], [3, 1], "a reset engine starts again too")
    }

    func testCoreHapticsPatternsAreWellFormed() throws {
        guard case let .pattern(events, curves)? = SurfaceHaptic.parse(pattern) else { return XCTFail("not a pattern") }
        let made = try CoreHapticsPlayer.pattern(events, curves)
        XCTAssertEqual(made.duration, 0.25, accuracy: 1e-6)
        XCTAssertNoThrow(try CoreHapticsPlayer.texturePattern())
        let controls = CoreHapticsPlayer.controls(intensity: 0.5, sharpness: 0.25)
        XCTAssertEqual(controls.map(\.value), [0.5, -0.25])
    }
    #endif
}
