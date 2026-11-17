// The core's side of a GPU surface's input and of the agent's world seams
// (LLP 1046.002 §3), over the `Canvases` protocol (LLP 1047.001 D4): a
// view's surface controls, its focus, the agent's taps and keys. Without
// the Surfaces module every question here answers as an app with no canvas.
import Foundation
#if os(macOS)
import AppKit
#else
import UIKit
#endif

extension NodeView {
    var returnsPointerFocusToCanvas: Bool {
        kind == "button" && props["action"] == nil && props["accessibilityRole"] != "slider" && inputCanvas != nil
    }
    #if os(macOS)
    /// Pointer-completed HUD presses return the keyboard to the enclosing world.
    func finishPointerPress() {
        guard returnsPointerFocusToCanvas, let canvas = inputCanvas else { return }
        finishPress(canvas: canvas, window: window, pointer: true)
    }
    #endif

    #if os(macOS)
    /// Complete an activation with the input owner captured before dispatch.
    func finishPress(canvas: NodeView?, window ownerWindow: NSWindow?, pointer: Bool) {
        guard kind == "button", props["action"] == nil, props["accessibilityRole"] != "slider",
              let canvas, let ownerWindow else { return }
        let current = ownerWindow.firstResponder
        let presenter = canvas.presenter
        let unclaimed = current == nil || current === ownerWindow || current === ownerWindow.contentView
            || current === presenter?.viewport || current === presenter?.session?.view
        let vacated = window == nil && (current === self || unclaimed)
        guard (pointer && current === self) || vacated,
              canvas.window === ownerWindow, canvas.canvasInput != nil,
              presenter?.views[canvas.id] === canvas else { return }
        _ = canvas.focusCanvas()
        presenter?.syncAccessibility()
    }
    #endif

    package var inputCanvas: NodeView? {
        #if os(macOS)
        var ancestor: NSView? = self
        #else
        var ancestor: UIView? = self
        #endif
        while let view = ancestor {
            if let node = view as? NodeView, node.canvasInput != nil { return node }
            ancestor = view.superview
        }
        return nil
    }
    package var isSurfaceControl: Bool { props["action"] != nil && inputCanvas != nil }
    /// The contact of a control pressed without a pointer, by a declared
    /// shortcut, as the web's assistive click (gpu-glue.js): it moves no
    /// focus, so a focused world keeps its keyboard and held keys.
    package static let assistiveContact = 4_294_967_292
    package var ownsSurfaceControl: Bool { presenter?.session?.canvases.ownsControl(id) == true }
    func cancelSurfaceControls() { presenter?.session?.canvases.cancelControls(of: id) }
    @discardableResult
    package func focusSurfacePointer() -> Bool {
        #if os(macOS)
        if (window?.firstResponder as? NSTextView)?.isEditable == true { return true }
        if presenter?.mouseChain.keepingFocus == true { return true } // the down's `pointerdown` was prevented: the focus stays (LLP 1005 §3)
        return isSurfaceControl ? window?.makeFirstResponder(self) == true : focusCanvas()
        #else
        if presenter?.editing != nil { return true }
        if keepsFocus(from: self) { return true } // its touch's `pointerdown` was prevented (LLP 1005 §3)
        if isSurfaceControl { return isFirstResponder || takeSurfaceTouchFocus() }
        return focusCanvas()
        #endif
    }
    @discardableResult
    package func control(_ phase: String, id contact: Int = 1, point: CGPoint = .zero, timestamp: Double? = nil) -> Bool {
        presenter?.session?.canvases.control(self, phase, id: contact, point: point, timestamp: timestamp) ?? false
    }
    package func controlKey(_ code: String, down: Bool, timestamp: Double? = nil) -> Bool {
        guard ["Space", "Enter", "NumpadEnter"].contains(code) else { return false }
        if presenter?.session?.canvases.pressedControlKey(code, down:down, canvas:inputCanvas?.id, timestamp:timestamp) == true { return true }
        if down {
            #if os(macOS)
            guard window?.firstResponder === self else { return false }
            #else
            guard isFirstResponder else { return false }
            #endif
        }
        return control(down ? "down" : "up", id:code == "Space" ? 4294967294 : 4294967293, timestamp:timestamp)
    }
    package func forwardsCanvasKey(_ code: String, command: Bool = false) -> Bool {
        guard !disabled, !inert, field == nil, textArea == nil, !command, code != "Tab" else { return false }
        return !(["Space", "Enter", "NumpadEnter"].contains(code)
            && (kind == "button" || ["button", "link"].contains(props["accessibilityRole"] ?? "")))
    }
}

#if EXACT_AGENT
extension Agent {
    package static func worldRequest(_ request: [String: Any]) -> Bool { request["entity"] != nil || request["world"] as? Bool == true }

    func world(_ request: [String: Any]) -> [String: Any] { session.canvases.world(self, request) }

    /// A held key releases its original surface without resolving or focusing a view.
    func releaseCanvasKey(_ request: [String: Any]) -> [String: Any]? {
        guard request["phase"] as? String == "up", let token = request["releaseKey"] as? String else { return nil }
        // A failed down may never have installed its closure; release is still safe.
        return keyReleases.removeValue(forKey: token)?() ?? ["phase": "up", "delivery": "recognized"]
    }

    func canvasType(_ view: NodeView, _ request: [String: Any]) -> [String: Any] { session.canvases.type(self, view, request) }
}
#endif

#if os(iOS) || os(tvOS)
/// Whether a hit view's touches are a surface's (a scroll or a modal lets them go).
enum CanvasInputs {
    static func owns(_ hit: UIView?) -> Bool {
        guard let view = hit as? NodeView else { return false }
        return !view.disabled && !view.inert && (view.canvases?.wantsInput(view.id) == true || view.isSurfaceControl)
    }
}

extension NodeView {
    package var canvasScale: CGFloat { metal?.metalLayer.contentsScale ?? window?.screen.scale ?? traitCollection.displayScale }
    package func focusCanvas() -> Bool {
        guard canvases?.wantsInput(id) == true, canBecomeFirstResponder, window != nil else { return false }
        var ancestor: UIView? = self
        while let view = ancestor {
            if view.isHidden { return false }
            ancestor = view.superview
        }
        if !isFirstResponder { _ = becomeFirstResponder() }
        return isFirstResponder
    }
}

#if EXACT_AGENT
extension Agent {
    /// UIKit exposes no touch constructor. This is the raw seam, labelled recognized.
    func canvasTap(_ request: [String: Any]) -> [String: Any]? {
        let phase = request["phase"] as? String
        let continuing = phase != nil && phase != "down"
        if continuing, canvasContact == nil, contact != nil {
            contact = nil
            return ["error": "the contact's canvas is gone"]
        }
        guard let node = continuing ? canvasContact : view(request), (session.canvases.wantsInput(node.id) || node.isSurfaceControl),
              session.presenter.views[node.id] === node, continuing || (!node.disabled && !node.inert), let window = node.window else { return nil }
        if request["contextmenu"] != nil || request["dblclick"] != nil { return nil }
        let vp = session.presenter.viewport
        let b = box(node)
        // `at`: a point in the canvas from its top left (`tap <canvas> at <x> <y>`).
        let offset = continuing ? nil : (request["at"] as? [Double]).flatMap { $0.count == 2 && $0.allSatisfy(\.isFinite) ? CGPoint(x: b.minX + $0[0], y: b.minY + $0[1]) : nil }
        if !continuing, request["at"] != nil, offset == nil { return ["error": "tap at needs two finite numbers"] }
        let start = offset ?? contact ?? CGPoint(x: b.midX, y: b.midY)
        if let scale = request["pinch"] as? Double, phase == nil {
            return node.isSurfaceControl ? nil : canvasPinch(node, scale: scale, focal: start, in: window)
        }
        let point = CGPoint(x: request["x"] as? Double ?? start.x + (request["dx"] as? Double ?? 0),
                            y: request["y"] as? Double ?? start.y + (request["dy"] as? Double ?? 0))
        guard point.x.isFinite, point.y.isFinite else { return ["error": "pointer needs finite coordinates"] }
        let inWindow = vp.convert(CGPoint(x: point.x + vp.pageOrigin.x, y: point.y + vp.pageOrigin.y), to: nil)
        if !continuing {
            guard let hit = window.hitTest(inWindow, with: nil), hit === node || (node.isSurfaceControl && hit.isDescendant(of: node)) else { return ["error":"control or canvas is covered"] }
        }
        let p = node.local(inWindow)
        let at = [InspectionValue.r2(point.x), InspectionValue.r2(point.y)]
        // A sample stamped `time`, the contact's (a set agent clock still wins,
        // LLP 1133.013 D7). Its point and time become the contact's only once
        // the surface takes it, so a refused one leaves the last delivered.
        func send(_ phase: String, at p: CGPoint = p, time: Double? = nil) -> Bool {
            let ok: Bool
            if node.isSurfaceControl {
                ok = node.control(phase, point: p, timestamp: time)
            } else {
                // A move's motion is its position's change; a down or up has none.
                let from = phase == "move" ? canvasPoint ?? p : p
                ok = session.canvases.input(node, ["t": "pointer", "phase": phase, "id": 1, "x": p.x, "y": p.y, "dx": p.x - from.x, "dy": p.y - from.y, "kind": "touch", "buttons": phase == "up" || phase == "cancel" ? 0 : 1], timestamp: time)
                if ok { canvasPoint = p }
            }
            if ok, let time { canvasTime = time }
            return ok
        }
        if let wheel = request["wheel"] as? [Double], wheel.count == 2 {
            guard wheel.allSatisfy(\.isFinite) else { return ["error": "wheel deltas must be finite"] }
            let ok = session.canvases.input(node, ["t": "wheel", "dx": wheel[0], "dy": wheel[1], "x": p.x, "y": p.y])
            return ok ? ["tapped": node.id, "wheel": wheel, "at": at, "delivery": "recognized"] : ["error": "surface refused wheel"]
        }
        if request["hover"] as? Bool == true {
            let from = canvasPoint ?? p
            canvasPoint = p
            let ok = session.canvases.input(node, ["t": "pointer", "phase": "move", "id": 1, "kind": "mouse", "buttons": 0, "x": p.x, "y": p.y, "dx": p.x - from.x, "dy": p.y - from.y])
            return ok ? ["tapped": node.id, "hover": true, "at": at, "delivery": "recognized"] : ["error": "surface refused pointer"]
        }
        if phase == nil {
            guard node.focusSurfacePointer(), send("down"), send("up") else { return ["error": "surface refused pointer"] }
            return ["tapped": node.id, "at": at, "delivery": "recognized"]
        }
        switch phase {
        case "down":
            guard contact == nil else { return ["error": "a contact is already down; up or cancel it first"] }
            guard node.focusSurfacePointer(), send("down", time: CACurrentMediaTime()) else { return ["error": "surface refused pointer"] }
            contact = point; canvasContact = node
            return ["contact": node.id, "phase": "down", "at": at, "delivery": "recognized"]
        case "move":
            guard let from = contact else { return ["error": "no contact is down"] }
            let ms = max(0, request["ms"] as? Double ?? 0)
            // A drag's bound on `over` (`DRAG_BOUNDS.over`), so no duration makes an unbounded loop.
            guard ms <= 2000 else { return ["error": "tap move: over \(ms) ms is past its bound, 2000 ms"] }
            // One move per 16 ms, each stamped at its pace on the contact's time,
            // as the web, AppKit, Linux and pan carriers deliver a timed move
            // (LLP 1035.003 D1). On the platform's clock the run loop turns
            // between them, so frames are drawn during the move; on the agent's
            // nothing waits, and the samples share the clock's time (LLP 1133.013 D7).
            let steps = max(1, Int((ms / 16).rounded()))
            for i in 1...steps {
                let f = CGFloat(i) / CGFloat(steps)
                let q = CGPoint(x: from.x + (point.x - from.x) * f, y: from.y + (point.y - from.y) * f)
                let time = (canvasTime ?? CACurrentMediaTime()) + ms / 1000 / Double(steps)
                guard send("move", at: node.local(vp.convert(CGPoint(x: q.x + vp.pageOrigin.x, y: q.y + vp.pageOrigin.y), to: nil)), time: time) else {
                    // The samples before it were delivered: the contact is where they left it.
                    return ["error": "surface refused pointer", "at": [InspectionValue.r2(contact?.x ?? from.x), InspectionValue.r2(contact?.y ?? from.y)]]
                }
                contact = q
                if ms > 0, session.clock == nil { RunLoop.main.run(until: Date(timeIntervalSinceNow: ms / 1000 / Double(steps))) }
            }
        case "up", "cancel":
            guard contact != nil else { return ["error": "no contact is down"] }
            // The lift is one frame after the last sample, as on the other carriers.
            // On the platform's clock it is delivered no earlier than its stamp,
            // so the next contact's down, stamped on arrival, comes after it.
            let time = (canvasTime ?? CACurrentMediaTime()) + 1.0 / 60
            if session.clock == nil, time > CACurrentMediaTime() { RunLoop.main.run(until: Date(timeIntervalSinceNow: time - CACurrentMediaTime())) }
            guard send(phase!, time: time) else { return ["error": "surface refused pointer"] }
            contact = nil; canvasContact = nil; canvasTime = nil
        case "hold":
            guard let previous = contact else { return ["error": "no accepted contact is down"] }
            if point != previous {
                guard send("move", time: canvasTime) else { return ["error": "surface refused pointer"] }
                contact = point
            }
            let ms = max(0, request["ms"] as? Double ?? 0)
            if ms.isFinite { canvasTime = (canvasTime ?? CACurrentMediaTime()) + ms / 1000 }
            holdTime(ms, request)
        default: return ["error": "unknown pointer phase \(phase!)"]
        }
        return ["phase": phase!, "at": at, "delivery": "recognized"]
    }

    /// `tap <canvas> pinch <scale> [at <x> <y>]`: two fingers, as UIKit reports
    /// a real pinch to `CanvasInputHost`: both contacts go down together, both
    /// move in every event, both lift. They lie level about the focal point and
    /// spread from 0.3 of the canvas's shorter side to that times the scale in
    /// 8 moves 16 ms apart, the pinch the web driver gives Chrome (agent-web.mjs).
    private func canvasPinch(_ node: NodeView, scale: Double, focal: CGPoint, in window: UIWindow) -> [String: Any] {
        guard scale.isFinite, scale > 0 else { return ["malformed": true, "error": "pinch: expected a positive finite scale"] }
        guard contact == nil else { return ["error": "a contact is already down; up or cancel it first"] }
        let vp = session.presenter.viewport
        let b = box(node), d = max(8, min(b.width, b.height) * 0.3)
        func fingers(_ k: Double) -> [CGPoint] {
            [-1.0, 1.0].map { vp.convert(CGPoint(x: focal.x + $0 * d * k / 2 + vp.pageOrigin.x, y: focal.y + vp.pageOrigin.y), to: nil) }
        }
        for (q, page) in zip(fingers(1), [-1.0, 1.0].map { CGPoint(x: focal.x + $0 * d / 2, y: focal.y) }) where window.hitTest(q, with: nil) !== node {
            return ["error": "pinch: a finger at (\(InspectionValue.r2(page.x)), \(InspectionValue.r2(page.y))) lands beside the canvas or on what covers it"]
        }
        guard node.focusSurfacePointer() else { return ["error": "surface refused pointer"] }
        var last = fingers(1).map(node.local), time = CACurrentMediaTime()
        func send(_ phase: String, _ points: [CGPoint]) -> Bool {
            let ok = points.indices.allSatisfy { i in
                let p = points[i], from = phase == "move" ? last[i] : p
                return session.canvases.input(node, ["t": "pointer", "phase": phase, "id": i + 1, "x": p.x, "y": p.y, "dx": p.x - from.x, "dy": p.y - from.y,
                                                     "kind": "touch", "buttons": phase == "up" || phase == "cancel" ? 0 : 1], timestamp: time)
            }
            if ok { last = points }
            return ok
        }
        // On the platform's clock the run loop turns between samples, as for a timed move.
        func pace() {
            time += 0.016
            if session.clock == nil { RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.016)) }
        }
        var delivered = send("down", last)
        for i in 1...8 where delivered {
            pace()
            delivered = send("move", fingers(1 + (scale - 1) * Double(i) / 8).map(node.local))
        }
        if delivered { pace(); delivered = send("up", last) }
        guard delivered else {
            _ = send("cancel", last)
            return ["error": "surface refused pointer"]
        }
        return ["tapped": node.id, "pinch": scale, "at": [InspectionValue.r2(focal.x), InspectionValue.r2(focal.y)], "delivery": "recognized"]
    }
}
#endif
#endif

#if os(iOS) || os(tvOS)
extension NodeView {
    func pressedControls(_ presses: Set<UIPress>, down: Bool) -> Set<UIPress> {
        Set(presses.filter { press in
            guard let key=press.key, key.modifierFlags.intersection([.command,.control]).isEmpty else {return true}
            return presenter?.session?.canvases.pressedControlKey(KeyCodes.hid(key.keyCode.rawValue),down:down,timestamp:press.timestamp) != true
        })
    }
}
#endif

#if os(macOS)
extension NodeView {
    package var canvasScale: CGFloat { metal?.layer?.contentsScale ?? pixelScale ?? 1 }
    package func focusCanvas() -> Bool {
        guard canvases?.wantsInput(id) == true, acceptsFirstResponder, let window else { return false }
        if window.firstResponder !== self { window.makeFirstResponder(self) }
        return window.firstResponder === self
    }
}

#endif
