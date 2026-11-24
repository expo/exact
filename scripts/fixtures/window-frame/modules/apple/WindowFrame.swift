import AppKit

/// Animates the session's window 300 by 200 points larger or smaller with
/// `NSWindow.setFrame(_:display:animate:)`, from `native.call` or
/// `native.later`. `report` answers what the window's resize steps since the
/// last report showed: how many there were, at how many the page's root was
/// as wide and tall as the viewport (laid out at that size), and at how many
/// the document was larger than the viewport (the page could scroll). With
/// `EXACT_WINDOW_FRAME_FILM` set to a directory, each step's content is drawn
/// there as a PNG, which a person reads as a film of the resize.
final class WindowFrame: ExactModule {
    private var steps = 0, laidOut = 0, overflowed = 0, filmed = 0
    private let film = ProcessInfo.processInfo.environment["EXACT_WINDOW_FRAME_FILM"]
    private var observer: NSObjectProtocol?

    override func call(_ request: [String: Any]) throws -> [String: Any] {
        try frame(request)
    }
    override func later(_ request: [String: Any], reply: ExactReply) {
        do { reply.send(try frame(request)) } catch { reply.fail("\(error)") }
    }
    deinit { observer.map(NotificationCenter.default.removeObserver) }

    private func frame(_ request: [String: Any]) throws -> [String: Any] {
        guard let view = context.sessionView as? NSView, let window = view.window,
              let page = Self.scrollView(in: view) else { throw ExactNativeRefusal("no window") }
        if observer == nil {
            observer = NotificationCenter.default.addObserver(forName: NSWindow.didResizeNotification, object: window, queue: nil) { [weak self] _ in
                guard let self else { return }
                steps += 1
                let clip = page.contentView.bounds.size, document = page.documentView?.frame.size ?? .zero
                if let root = page.documentView?.subviews.first?.frame.size, abs(root.width - clip.width) < 0.5, abs(root.height - clip.height) < 0.5 { laidOut += 1 }
                if document.width - clip.width > 0.5 || document.height - clip.height > 0.5 { overflowed += 1 }
                if let film, let content = window.contentView { Self.draw(content, to: "\(film)/step-\(String(format: "%03d", filmed)).png"); filmed += 1 }
            }
        }
        let to = request["to"] as? String
        if to == "grow" || to == "shrink" {
            let sign: CGFloat = to == "shrink" ? -1 : 1
            var target = window.frame
            target.size.width += 300 * sign
            target.size.height += 200 * sign
            target.origin.y -= 200 * sign
            window.setFrame(target, display: true, animate: true)
        }
        defer { if to == "report" { steps = 0; laidOut = 0; overflowed = 0 } }
        return ["steps": steps, "laidOut": laidOut, "overflowed": overflowed, "width": window.frame.width, "height": window.frame.height]
    }

    private static func draw(_ view: NSView, to path: String) {
        guard let layer = view.layer, let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(view.bounds.width), pixelsHigh: Int(view.bounds.height),
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0),
              let context = NSGraphicsContext(bitmapImageRep: rep) else { return }
        let cg = context.cgContext
        cg.setFillColor(NSColor.windowBackgroundColor.cgColor)
        cg.fill(CGRect(origin: .zero, size: view.bounds.size))
        if view.isFlipped { cg.translateBy(x: 0, y: view.bounds.height); cg.scaleBy(x: 1, y: -1) }
        layer.render(in: cg)
        try? rep.representation(using: .png, properties: [:])?.write(to: URL(fileURLWithPath: path))
    }

    private static func scrollView(in view: NSView) -> NSScrollView? {
        if let scroll = view as? NSScrollView { return scroll }
        for sub in view.subviews { if let found = scrollView(in: sub) { return found } }
        return nil
    }
}

let exactModule: ExactModule.Type = WindowFrame.self
