#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

final class BoxMotionMacTests: XCTestCase {
    private static let plan: Result<Data, Error> = Result {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let source = """
        keyframes pulse
          from scale=0.5
          to scale=1.5
        keyframes move
          from translate="0px 0px" rotate=0 opacity=1
          to translate="40px 20px" rotate=360 opacity=0.2
        keyframes paint
          from background-color="#ff0000"
          to background-color="#0000ff"
        keyframes sweep
          from translate="0px 0px"
          to translate="100px 0px"
        component App
          state on = false
          state wide = false
          state unsafe = false
          state mounted = false
          state backdrop = false
          action toggle
            on = not on
          action sample
            unsafe = true
          action resize
            wide = not wide
          action reveal
            mounted = true
          action blur
            backdrop = true
          view
            column padding=20 gap=10
              button "Toggle" testId="toggle" press=toggle
              button "Resize" testId="resize" press=resize
              button "Sample" testId="sample" press=sample
              button "Reveal" testId="reveal" press=reveal
              button "Blur" testId="blur" press=blur
              box testId="pulse" width=(wide ? 120 : 80) height=40 background-color="#ff0000" animation=(on ? "pulse 1s linear infinite alternate" : "none")
              box testId="move" width=80 height=40 animation=(on ? "move 1s linear infinite" : "none")
              box testId="transition" width=80 height=40 scale=(on ? 1.5 : 1) translate=(on ? "40px 20px" : "0px 0px") rotate=(on ? 90 : 0) opacity=(on ? 0.2 : 1) transition="scale 1s linear, translate 1s linear, rotate 1s linear, opacity 1s linear"
              box testId="shorthand" width=80 height=40 transform=(on ? "translate(40px, 20px) rotate(90deg) scale(1.5)" : "none") transition="translate 1s linear, rotate 1s linear, scale 1s linear"
              text "Painted text" testId="painted-text" background-color="#ff0000" animation=(unsafe ? "paint 1s linear infinite" : "none")
              box testId="interactive" width=80 height=40 press=toggle animation=(unsafe ? "pulse 1s linear infinite" : "none")
              box testId="moving-parent" width=100 height=40 animation=(on ? "sweep 1s linear infinite" : "none")
                box width=100 height=40
                  if mounted
                    button "Child" testId="child" press=toggle width=80 height=30
              box testId="handler-parent" width=100 height=40 animation=(on ? "sweep 1s linear infinite" : "none")
                box width=100 height=40
                  if mounted
                    box press=toggle width=80 height=30
              box testId="rounded" width=100 height=40 border-radius=10 background-color="#ff0000" animation=(unsafe ? "paint 1s linear infinite" : "none")
              box testId="partial" width=100 height=40 border-top-left-radius=10 overflow="hidden" background-color="#ff0000" animation=(unsafe ? "paint 1s linear infinite" : "none")
              box testId="rounded-transition" width=100 height=40 border-radius=10 background-color=(unsafe ? "#0000ff" : "#ff0000") transition="background-color 1s linear"
              box testId="backdrop-keyframes" width=80 height=40 backdrop-filter="blur(6px)" animation=(backdrop ? "sweep 1s linear infinite" : "none")
              box testId="backdrop-transition" width=80 height=40 backdrop-filter="blur(6px)" translate=(backdrop ? "100px 0px" : "0px 0px") transition="translate 1s linear"

        """
        let entry = directory.appendingPathComponent("app.contract"), output = directory.appendingPathComponent("app.plan")
        try source.write(to: entry, atomically: true, encoding: .utf8)
        let compiler = Process()
        compiler.executableURL = URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_CONTRACT"]))
        compiler.arguments = ["build", entry.path, "-o", output.path]
        try compiler.run(); compiler.waitUntilExit()
        XCTAssertEqual(compiler.terminationStatus, 0)
        return try Data(contentsOf: output)
    }

    func testBoxKeyframesRunOnTheLayerAndSeekOnTheAgentClock() throws {
        _ = NSApplication.shared
        let session = ExactApp.shared.makeSession(label: "box-motion")
        defer { session.destroy() }
        XCTAssertNil(session.boot(plan: try Self.plan.get(), size: CGSize(width: 400, height: 500)).error)
        func view(_ name: String) throws -> NodeView {
            try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == name })
        }
        let pulse = try view("pulse"), move = try view("move")
        let start = session.now()
        session.apply(session.runtime.press(try view("toggle").id, now: start))
        let frame = start + 16
        session.apply(session.runtime.tick(now: frame, frame: frame))
        let layer = try XCTUnwrap(pulse.layer)
        let animation = try XCTUnwrap(layer.animation(forKey: "pulse#0#transform.scale.x") as? CAKeyframeAnimation)
        XCTAssertEqual(animation.keyPath, "transform.scale.x")
        XCTAssertEqual(animation.speed, 1)
        XCTAssertEqual(animation.repeatCount, .infinity)
        XCTAssertEqual(layer.anchorPoint, CGPoint(x: 0.5, y: 0.5))
        XCTAssertEqual(layer.position, CGPoint(x: pulse.frame.midX, y: pulse.frame.midY))
        XCTAssertEqual(layer.affineTransform(), .identity)
        for key in ["transform.translation.x", "transform.translation.y", "transform.rotation.z", "opacity"] {
            XCTAssertNotNil(move.layer?.animation(forKey: "move#0#\(key)"), key)
        }
        for name in ["transition", "shorthand"] {
            let layer = try XCTUnwrap(view(name).layer)
            let paths = Set((layer.animationKeys() ?? []).compactMap { (layer.animation(forKey: $0) as? CAKeyframeAnimation)?.keyPath })
            var expected: Set<String> = ["transform.scale.x", "transform.scale.y", "transform.translation.x", "transform.translation.y", "transform.rotation.z"]
            if name == "transition" { expected.insert("opacity") }
            XCTAssertEqual(paths, expected, name)
        }
        XCTAssertFalse(session.frames.motion, "Core Animation owns every playing track; no engine frame is due")

        session.clock = frame + 500
        session.apply(session.runtime.tick(now: session.now()))
        session.presenter.svg?.seek(clock: session.clock); session.presenter.boxAnimations.seek(clock: session.clock)
        let held = try XCTUnwrap(layer.animation(forKey: "pulse#0#transform.scale.x"))
        XCTAssertEqual(held.speed, 0)
        XCTAssertEqual(held.timeOffset, 0.5, accuracy: 0.001)
        let transition = try view("transition")
        XCTAssertEqual(transition.scale.x, 1.25, accuracy: 0.001)
        XCTAssertEqual(transition.scale.y, 1.25, accuracy: 0.001)
        XCTAssertEqual(transition.translate.x, 20, accuracy: 0.001)
        XCTAssertEqual(transition.rotate, 45, accuracy: 0.001)
        XCTAssertFalse((transition.layer?.animationKeys() ?? []).contains { $0.hasPrefix("transition#") })
        session.apply(session.runtime.press(try view("sample").id, now: session.now()))
        XCTAssertNil(try view("interactive").layer?.animation(forKey: "pulse#0#transform.scale.x"))
        XCTAssertTrue(session.frames.motion, "the interactive box stays sampled for hit testing")
        XCTAssertNil(try view("painted-text").layer?.animation(forKey: "paint#0#backgroundColor"), "AppKit draws a paragraph's background into its raster")
        session.apply(session.runtime.press(try view("resize").id, now: session.now()))
        pulse.layoutSubtreeIfNeeded()
        XCTAssertEqual(layer.anchorPoint, CGPoint(x: 0.5, y: 0.5))
        XCTAssertEqual(layer.position, CGPoint(x: pulse.frame.midX, y: pulse.frame.midY))
        session.apply(session.runtime.press(try view("toggle").id, now: session.now()))
        XCTAssertNil(layer.animation(forKey: "pulse#0#transform.scale.x"))
        XCTAssertEqual(layer.anchorPoint, .zero)
        XCTAssertEqual(layer.position, pulse.frame.origin)
        XCTAssertEqual(layer.affineTransform(), .identity)
    }

    func testMountingInputWithdrawsAncestorTransformAnimations() throws {
        _ = NSApplication.shared
        let session = ExactApp.shared.makeSession(label: "box-motion-mount")
        defer { session.destroy() }
        XCTAssertNil(session.boot(plan: try Self.plan.get(), size: CGSize(width: 400, height: 900)).error)
        func view(_ name: String) throws -> NodeView {
            try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == name })
        }
        let start = session.now()
        session.apply(session.runtime.press(try view("toggle").id, now: start))
        let frame = start + 16
        session.apply(session.runtime.tick(now: frame, frame: frame))
        let parents = try [view("moving-parent"), view("handler-parent")]
        for parent in parents {
            XCTAssertNotNil(parent.layer?.animation(forKey: "sweep#0#transform.translation.x"))
        }
        XCTAssertFalse(session.frames.motion)
        session.apply(session.runtime.press(try view("reveal").id, now: frame + 250))
        XCTAssertTrue(session.frames.motion, "new descendants need model transforms for hit testing")
        for parent in parents {
            XCTAssertNil(parent.layer?.animation(forKey: "sweep#0#transform.translation.x"))
            XCTAssertEqual(parent.translate.x, 25, accuracy: 0.001, "withdraw at the current animation phase")
        }
        session.clock = frame + 500
        session.apply(session.runtime.tick(now: session.now()))
        for parent in parents {
            XCTAssertEqual(parent.translate.x, 50, accuracy: 0.001)
        }
        let child = try view("child"), parent = parents[0]
        let base = child.convert(CGPoint(x: child.bounds.midX, y: child.bounds.midY), to: parent.superview)
        let hit = parent.hitTest(CGPoint(x: base.x + 50, y: base.y))
        XCTAssertTrue(hit === child || hit?.isDescendant(of: child) == true)
    }

    func testRoundedBackgroundMotionReachesTheFill() throws {
        _ = NSApplication.shared
        let session = ExactApp.shared.makeSession(label: "box-motion-fill")
        defer { session.destroy() }
        XCTAssertNil(session.boot(plan: try Self.plan.get(), size: CGSize(width: 400, height: 900)).error)
        let sample = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "sample" })
        let start = session.now(), frame = start + 16
        session.apply(session.runtime.press(sample.id, now: start))
        session.apply(session.runtime.tick(now: frame, frame: frame))
        let names = ["rounded", "partial", "rounded-transition"]
        let views = try names.map { name in
            try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == name })
        }
        for view in views {
            let layer = try XCTUnwrap(view.layer)
            let paths = (layer.animationKeys() ?? []).compactMap { (layer.animation(forKey: $0) as? CAKeyframeAnimation)?.keyPath }
            XCTAssertFalse(paths.contains("backgroundColor"), "the backing layer does not hold this fill")
        }
        session.clock = frame + 500
        session.apply(session.runtime.tick(now: session.now()))
        for view in views {
            view.applyBoxLayer()
            XCTAssertNil(view.layer?.backgroundColor)
            let fill = try XCTUnwrap(view.boxFill)
            let color = try XCTUnwrap((fill as? CAShapeLayer)?.fillColor ?? fill.backgroundColor)
            let rgb = try XCTUnwrap(NSColor(cgColor: color)?.usingColorSpace(.deviceRGB))
            XCTAssertEqual(rgb.redComponent, 0.5, accuracy: 0.01)
            XCTAssertEqual(rgb.blueComponent, 0.5, accuracy: 0.01)
        }
    }

    func testBackdropSamplingRegionFollowsKeyframesAndTransitions() throws {
        _ = NSApplication.shared
        let session = ExactApp.shared.makeSession(label: "box-motion-backdrop")
        defer { session.destroy() }
        XCTAssertNil(session.boot(plan: try Self.plan.get(), size: CGSize(width: 400, height: 1200)).error)
        func view(_ name: String) throws -> NodeView {
            try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == name })
        }
        let views = try [view("backdrop-keyframes"), view("backdrop-transition")]
        let regions = try views.map { try XCTUnwrap($0.backdropDrawn?.box) }
        let start = session.now(), frame = start + 16
        session.apply(session.runtime.press(try view("blur").id, now: start))
        session.apply(session.runtime.tick(now: frame, frame: frame))
        XCTAssertTrue(session.frames.motion, "backdrop filters read the sampled model transform")
        for view in views {
            let layer = try XCTUnwrap(view.layer)
            let paths = (layer.animationKeys() ?? []).compactMap { (layer.animation(forKey: $0) as? CAKeyframeAnimation)?.keyPath }
            XCTAssertFalse(paths.contains("transform.translation.x"))
        }
        for elapsed in [250.0, 500.0] {
            // Keep the platform clock: taking the agent clock would reclaim a lowered transition.
            session.apply(session.runtime.tick(now: frame + elapsed, frame: frame + elapsed))
            for (view, initial) in zip(views, regions) {
                XCTAssertEqual(view.translate.x, elapsed / 10, accuracy: 0.001)
                let region = try XCTUnwrap(view.backdropDrawn?.box)
                XCTAssertEqual(region.minX, initial.minX + elapsed / 10, accuracy: 0.001)
                XCTAssertEqual(region.minY, initial.minY, accuracy: 0.001)
                XCTAssertEqual(region.size, initial.size)
            }
        }
    }
}
#endif
