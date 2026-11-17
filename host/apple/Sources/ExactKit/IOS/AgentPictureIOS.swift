// A check prepares guest snapshots before hiding menu substitutes, then keeps
// them hidden until the carrier has composed the simulator display.
#if os(iOS) || os(tvOS)
import UIKit

struct AgentPicturePreparation {
    let loading: Int
    let moving: Bool
    let guests: [UInt32: UIImage]
}

extension Agent {
    func finishCheckPicture() {
        checkPicture = nil
        presenter.menus.endCheck()
    }

    func preparePicture(_ req: [String: Any]) -> AgentPicturePreparation {
        let loading = settleForPicture()
        // UIKit's own motion lands first, unless the picture is a film's frame (LLP 1116.003 D8).
        let moving = req["film"] as? Bool != true && !settleMotion()
        // A canvas painting its children through its surface (LLP 1014)
        // shows its last capture: what is pending is captured and rendered
        // at the agent's clock first, as `clock` leaves it.
        if let now = session.clock, session.canvases.waitUntilReady() { session.canvases.settle(now: now) }
        presenter.canvas2d.waitForReplays()
        SvgFilterLive.waitForDraws()
        let guests = session.webviews.snapshots().merging(session.natives.snapshots()) { web, _ in web }
        return AgentPicturePreparation(loading: loading, moving: moving, guests: guests)
    }

    func screenshot(_ req: [String: Any]) -> [String: Any] {
        // The simulator driver's check, after it read the display, that the display was still this app's.
        if req["probe"] as? Bool == true { return ["front": displayFront()] }
        guard let path = req["path"] as? String else { return ["malformed": true, "error": "screenshot needs a path"] }
        let preparation = checkPicture ?? preparePicture(req)
        let loading = preparation.loading, moving = preparation.moving
        let vp = presenter.viewport
        let scale = vp.window?.screen.scale ?? vp.traitCollection.displayScale
        let format = UIGraphicsImageRendererFormat()
        format.scale = scale
        format.opaque = true
        // 8-bit sRGB: on a wide-color screen the renderer would write a
        // 16-bit PNG, which nothing downstream (scripts/png.mjs) reads.
        format.preferredRange = .standard
        // What UIKit presents over the app (an alert, an action sheet, a popover, a presented route's sheet) is a view of
        // the window beside the viewport, never in the viewport's picture: with one up, the capture is the window's, as
        // `window` draws it, and says so (LLP 1116.012 D3; grocery-list's first two captures missed its clear alert).
        // A film's frames keep one size.
        let presented = req["film"] as? Bool == true ? nil : presentedOverApp()
        let wholeWindow = req["window"] as? Bool == true || presented != nil
        let page = presenter.documentHost
        let captureView: UIView = wholeWindow ? (vp.window ?? page) : page
        // The page's picture is its area, whose top-left is a box's (0, 0), not the bands it is drawn past (LLP 1130).
        let margins = wholeWindow ? .zero : presenter.pageMargins
        let size = captureView.bounds.inset(by: margins).size
        #if targetEnvironment(simulator)
        // The simulator needs both paths: takeSnapshot alone omits guest
        // text, and drawHierarchy alone rasterizes a blank remote layer —
        // unless a takeSnapshot just flushed it. So the snapshot is taken
        // (the flush), composed as the underlay, and the live view stays
        // visible on top with the real pixels (measured 2026-08-30; the
        // 0-guest-pixel failure returns if either half is dropped).
        Capture.web = preparation.guests
        #else
        // A device capture has the macOS shape: hide every live WKWebView
        // and compose only the arm's takeSnapshot at the owning node.
        Capture.web = preparation.guests
        let hidden = (presenter.views.values.compactMap(\.web) + session.natives.snapshotViews).map { ($0, $0.isHidden) }
        hidden.forEach { $0.0.isHidden = true }
        #endif
        Capture.capturing = true
        // drawHierarchy can reuse a clean backing layer without calling draw.
        // Materialize this turn's web/GPU pictures before composing the hierarchy.
        let captured = presenter.views.values.filter { Capture.web[$0.id] != nil || $0.kind == "canvas" }
        for node in captured { node.setNeedsDisplay(); node.layer.displayIfNeeded() }
        session.natives.redrawForCapture()
        // Header nodes the app's navigation bar leaves out, as the agent's
        // chrome paints them: struck through in red (LLP 1116 D2).
        let unshown = presenter.navigation.unshownByBar().keys.sorted().compactMap { presenter.views[$0] }.filter {
            $0.window != nil && !$0.isHidden && $0.bounds.width > 0 && $0.bounds.height > 0
        }
        let png = UIGraphicsImageRenderer(size: size, format: format).pngData { _ in
            captureView.drawHierarchy(in: CGRect(origin: CGPoint(x: -margins.left, y: -margins.top), size: captureView.bounds.size), afterScreenUpdates: true)
            UIColor.systemRed.setStroke()
            for node in unshown {
                let r = node.convert(node.bounds, to: captureView).insetBy(dx: -1, dy: -1).offsetBy(dx: -captureView.bounds.minX - margins.left, dy: -captureView.bounds.minY - margins.top)
                let outline = UIBezierPath(rect: r)
                outline.lineWidth = 2
                outline.setLineDash([4, 3], count: 2, phase: 0)
                outline.stroke()
                let strike = UIBezierPath()
                strike.move(to: CGPoint(x: r.minX, y: r.maxY))
                strike.addLine(to: CGPoint(x: r.maxX, y: r.minY))
                strike.lineWidth = 1.5
                strike.stroke()
            }
        }
        Capture.capturing = false
        #if !targetEnvironment(simulator)
        hidden.forEach { $0.0.isHidden = $0.1 }
        #endif
        Capture.web = [:]
        // Snapshot pixels belong to this capture, not the live backing layers.
        for node in captured { node.setNeedsDisplay() }
        do { try png.write(to: URL(fileURLWithPath: path)) } catch { return ["error": "write \(path): \(error)"] }
        var r: [String: Any] = ["screenshot": path, "w": Agent.r2(size.width), "h": Agent.r2(size.height), "scale": Agent.r2(scale)]
        if wholeWindow { r["window"] = true }
        // Whether the simulator driver may take the display's pixels for this picture (devices.mjs `withDisplay`).
        if wholeWindow { r["front"] = displayFront() }
        if wholeWindow, let window = vp.window { r["opaque"] = displayOwnPoints(window) }
        // Glass or a blur in the page's picture is drawHierarchy's approximation: the picture's area in window
        // points, for the simulator driver to put the display's pixels there (devices.mjs `withMaterials`).
        if !wholeWindow, req["film"] as? Bool != true, let window = vp.window {
            let area = captureView.convert(captureView.bounds.inset(by: margins), to: window)
            if backdropRects(window).contains(where: { $0.insetBy(dx: 12, dy: 12).intersects(area) }) {
                r["materials"] = [Agent.r2(area.minX), Agent.r2(area.minY), Agent.r2(area.width), Agent.r2(area.height)]
            }
        }
        if moving { r["moving"] = true }
        #if os(iOS)
        // The status bar is SpringBoard's, never in the window's picture: its height, for the simulator driver to put it in (devices.mjs `withDisplay`).
        if wholeWindow, let bar = vp.window?.windowScene?.statusBarManager?.statusBarFrame.height, bar > 0, !(vp.window?.windowScene?.statusBarManager?.isStatusBarHidden ?? true) { r["statusBar"] = Agent.r2(bar) }
        // The window's own safe area, whatever the page's `viewport-fit`: the rows `compare` leaves out (the home indicator's too).
        if wholeWindow, let window = vp.window { r["safeArea"] = ["top": Agent.r2(window.safeAreaInsets.top), "bottom": Agent.r2(window.safeAreaInsets.bottom)] }
        #endif
        if let presented { r["presented"] = presented }
        if loading > 0 { r["imagesPending"] = loading }
        if !unshown.isEmpty {
            r["unshown"] = unshown.map { n -> [String: Any] in ["id": Int(n.id), "testId": n.props["testId"] ?? NSNull()] }
            r["unshownNote"] = "struck through in red: \(NavigationHost.unshownReason)"
        }
        // What the picture shows in the page that the app presents as UIKit's
        // own, said where the image cannot (euxy port, FRICTION §2).
        // Under native presentation what is still painted is what a person sees painted too (a content popover).
        let painted = presenter.menus.presented.filter { $0.window != nil && !$0.isHidden }
        if !painted.isEmpty, ExactEnv.agentPaints {
            r["painted"] = painted.map { n -> [String: Any] in ["id": Int(n.id), "testId": n.props["testId"] ?? NSNull()] }
            r["paintedNote"] = "the agent, not the app, painted the open popover in this picture, in the page (LLP 1021 D4); the app shows it as UIKit's (a menu at its button), which a screenshot shows under --touch platform"
        }
        let bars = presenter.navigation.paintedForBars()
        if !bars.isEmpty {
            r["chrome"] = bars.map { n -> [String: Any] in ["id": Int(n.id), "testId": n.props["testId"] ?? NSNull()] }
            r["chromeNote"] = "--chrome agent paints the authored header and tablist: the app shows UIKit's navigation bar and tab bar for them, which --chrome platform (exact.mjs's default) shows and `screenshot <png> window` draws"
        }
        // The software keyboard is not in the capture, which UIKit draws in a process of its own, and the app may
        // stand above it (LLP 1102 §3.17): say so, where the image alone reads as a shortened screen. The simulator
        // driver puts the keyboard's pixels in a window capture (`windowTop`, LLP 1116.008 D8; devices.mjs).
        let container = session.presenter.keyboardContainer
        if let top = container.flatMap({ session.presenter.keyboardGuideTop(in: $0) }) {
            var keyboard: [String: Any] = ["visible": true, "top": Agent.r2(top)]
            if let window = keyboardTopForCheck() { keyboard["windowTop"] = Agent.r2(window) }
            r["keyboard"] = keyboard
            r["note"] = "the software keyboard is up: it is not in the capture, and the app above it may be shortened (state shows keyboard.top)"
        }
        return r
    }

}
#endif
