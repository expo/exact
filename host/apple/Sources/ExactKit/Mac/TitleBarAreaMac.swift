#if os(macOS)
import AppKit

/// The title-bar area beside the window's controls, what
/// `env(titlebar-area-x|y|width|height)` reads: Window Controls Overlay's
/// "available area of the title bar" (https://wicg.github.io/window-controls-overlay/),
/// which excludes the area under the window's controls. AppKit's own
/// geometry: the title bar's band where the page's view lies under it
/// (`viewport-fit=cover`), less the traffic lights with their outer margin
/// repeated on the content side. `windowTitlebarLayoutDirection` determines
/// which side holds the controls, independently of the page's direction.
enum TitleBarAreaMac {
    /// x, y, width, height from the view's top left; all zero while the
    /// title bar is hidden (full screen) or a route's toolbar fills it (LLP
    /// 1115.002 D2); `nil` where the page does not underlap the title bar, so
    /// every variable is undefined and its fallback applies, as in a browser
    /// tab. `overlay`: the toolbar is `window-controls-overlay`'s empty one,
    /// whose band is the title bar's (LLP 1039 D8).
    static func rect(of view: NSView, cover: Bool, overlay: Bool = false) -> [CGFloat]? {
        guard cover, let window = view.window, window.styleMask.contains(.titled),
              window.styleMask.contains(.fullSizeContentView) else { return nil }
        if window.styleMask.contains(.fullScreen) { return [0, 0, 0, 0] }
        // The standard buttons' superview is the title bar's own view.
        let buttons = [NSWindow.ButtonType.closeButton, .miniaturizeButton, .zoomButton].compactMap { window.standardWindowButton($0) }
        guard let bar = buttons.first?.superview else { return nil }
        let bounds = view.bounds
        let band = view.convert(bar.bounds, from: bar).intersection(bounds)
        // A view below the title bar, embedded lower in the window, does not underlap it.
        guard !band.isNull, band.width > 0, band.height > 0 else { return nil }
        if window.toolbar?.isVisible == true, !overlay { return [0, 0, 0, 0] }
        var left = band.minX
        var right = band.maxX
        let shown = buttons.filter { !$0.isHidden && $0.superview === bar }
        if let first = shown.map(\.frame.minX).min(), let last = shown.map(\.frame.maxX).max() {
            if window.windowTitlebarLayoutDirection == .rightToLeft {
                let end = NSRect(x: first - (bar.bounds.maxX - last), y: bar.bounds.minY, width: 0, height: bar.bounds.height)
                right = min(right, view.convert(end, from: bar).minX)
            } else {
                let end = NSRect(x: last + first - bar.bounds.minX, y: bar.bounds.minY, width: 0, height: bar.bounds.height)
                left = max(left, view.convert(end, from: bar).maxX)
            }
        }
        let top = view.isFlipped ? band.minY - bounds.minY : bounds.maxY - band.maxY
        return [left - bounds.minX, top, max(0, right - left), band.height]
    }

    /// An authored region includes descendants up to the next declaration.
    /// The page-drawn title bar supplies a drag region on unclaimed ground.
    private static func region(at hit: NSView?, event: NSEvent, titleGround: Bool) -> (drag: Bool, node: NodeView?)? {
        var first: NodeView?
        var view = hit
        while let current = view {
            if let node = current as? NodeView {
                first = first ?? node
                if let value = node.style["app_region"]?.string { return (value == "drag", first) }
            }
            view = first?.presenter?.pointerParent(of: current) ?? current.superview
        }
        guard titleGround, let window = hit?.window, inTitleBand(event, window) else { return nil }
        return (true, first)
    }

    static func press(_ event: NSEvent, on hit: NSView?, titleGround: Bool) -> Bool {
        guard event.type == .leftMouseDown, !event.modifierFlags.contains(.control), let window = hit?.window,
              let region = region(at: hit, event: event, titleGround: titleGround) else { return false }
        if !region.drag { return titleGround && inTitleBand(event, window) }
        if event.clickCount == 2 {
            if let node = region.node, let listener = node.dblclickListener {
                guard !titleGround else { return false }
                // The first click belonged to AppKit's drag. Track its second click
                // here so the window does not route the up to that drag's view.
                if let up = release(in: window, drag: { _ in }),
                   listener.bounds.contains(listener.convert(up.locationInWindow, from: nil)) {
                    node.dispatchDblclick(node.dblclickTarget(up))
                }
                return true
            }
            switch action(UserDefaults.standard) {
            case .zoom: window.performZoom(nil)
            case .minimize: window.performMiniaturize(nil)
            case .none: break
            }
        } else { window.performDrag(with: event) }
        return true
    }

    private static func inTitleBand(_ event: NSEvent, _ window: NSWindow) -> Bool {
        window.styleMask.contains(.titled) && window.styleMask.contains(.fullSizeContentView)
            && !window.styleMask.contains(.fullScreen) && event.locationInWindow.y >= window.contentLayoutRect.maxY
    }

    /// The mouse-down whose release a `dblclick` listener in the band takes (`takesRelease`).
    nonisolated(unsafe) private static weak var taken: NSEvent?
    /// Whether `view`, the first node a second click in the band reaches, takes that click's release
    /// itself (`release`), as a control tracks its own click: the window then never routes the
    /// release, and an author's `dblclick` there is the only action. On macOS 27.0 AppKit's title
    /// bar recognizes a double click from the releases the window routes and performs the person's
    /// title-bar action (zoom, minimize or fill) over the page's handler; a button's click, which
    /// its tracking takes, does nothing there (LLP 1039 D8).
    static func takesRelease(of event: NSEvent, at view: NodeView) -> Bool {
        guard taken !== event, let window = view.window, event.type == .leftMouseDown, event.clickCount == 2,
              inTitleBand(event, window),
              view.dblclickListener != nil || region(at: view, event: event, titleGround: false)?.drag == false else { return false }
        taken = event
        return true
    }
    /// The button's moves and its release, from the queue to `view`, as the window would send them.
    /// With the button up and no release queued, something else took it, and nothing is waited for.
    static func release(to view: NodeView) {
        if let window = view.window, let up = release(in: window, drag: view.mouseDragged(with:)) { view.mouseUp(with: up) }
    }

    private static func release(in window: NSWindow, drag: (NSEvent) -> Void) -> NSEvent? {
        while let next = window.nextEvent(matching: [.leftMouseDragged, .leftMouseUp],
                                           until: NSEvent.pressedMouseButtons & 1 == 0 ? .distantPast : .distantFuture,
                                           inMode: .eventTracking, dequeue: true) {
            if next.type == .leftMouseUp { return next }
            drag(next)
        }
        return nil
    }

    enum Action: Equatable { case zoom, minimize, none }
    /// What a double click on a title bar does, as System Settings ▸ Desktop
    /// & Dock sets it ("Double-click a window's title bar to"):
    /// `AppleActionOnDoubleClick` in the global domain, else the older
    /// `AppleMiniaturizeOnDoubleClick`, else zoom. "Fill" zooms: AppKit has no
    /// public call for it, and zooming a window with no maximum size fills the
    /// screen too.
    static func action(_ defaults: UserDefaults) -> Action {
        action(actionOnDoubleClick: defaults.string(forKey: "AppleActionOnDoubleClick"),
               miniaturizeOnDoubleClick: defaults.bool(forKey: "AppleMiniaturizeOnDoubleClick"))
    }
    /// The two settings' values, read by `action(_:)`.
    static func action(actionOnDoubleClick: String?, miniaturizeOnDoubleClick: Bool) -> Action {
        switch actionOnDoubleClick {
        case "Minimize": return .minimize
        case "None": return .none
        case .some: return .zoom
        case nil: return miniaturizeOnDoubleClick ? .minimize : .zoom
        }
    }
}

extension ExactSession {
    /// The title-bar area (`TitleBarAreaMac`): x, y, width, height, or none.
    public func titlebarArea(_ rect: [CGFloat]?) {
        guard booted, state != .destroyed else { return }
        apply(runtime.titlebarArea(rect))
    }
}
#endif
