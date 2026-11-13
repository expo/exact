// Reader atoms share the editor's icon source, metrics and placement (LLP 1045.000 D4).
import CExact
import CoreText
#if os(macOS)
import AppKit
#else
import UIKit
#endif

extension TextEngine {
    /// Split an atom inside its direction isolate. Word joiners keep the
    /// replacement character and label on one line; the icon adds no height.
    func atomIcons(_ runs: [Run], node: NodeView? = nil) -> [Run] {
        runs.flatMap { original -> [Run] in
            guard var icon = original.atomIcon else { return [original] }
            var label = original
            label.atomIcon = nil
            let face = font(label)
            let side = icon.size.map { icon.sizeEm ? $0 * face.pointSize : $0 } ?? face.capHeight
            let gap = icon.gap.map { icon.gapEm ? $0 * face.pointSize : $0 } ?? face.pointSize * 0.25
            let pixels = Int((side * 3).rounded(.up))
            if icon.source.hasPrefix("symbol:") {
                guard AtomIcons.symbol(icon.source, size: side) != nil else { return [label] }
                icon.loaded = true
            } else {
                if let node {
                    let session = node.presenter?.session
                    _ = AtomIcons.image(icon.source, pixels: pixels, resolver: session?.app.resolver,
                                        log: { [weak session] in session?.log($0) }, subscriber: ObjectIdentifier(node)) { [weak node, source = icon.source] in
                        node?.atomIconReady(geometryChanged: AtomIcons.failed(source, pixels: pixels))
                    }
                } else {
                    _ = AtomIcons.image(icon.source, pixels: pixels, resolver: assetResolver, subscriber: ObjectIdentifier(self)) {}
                }
                guard !AtomIcons.failed(icon.source, pixels: pixels) else { return [label] }
                icon.loaded = AtomIcons.cached(icon.source, pixels: pixels) != nil
            }
            guard let isolate = label.text.firstIndex(where: { $0 == "\u{2066}" || $0 == "\u{2067}" }) else { return [label] }
            let start = label.text.index(after: isolate)
            var prefix = label
            prefix.text = String(label.text[..<start]) + "\u{2060}"
            label.text = "\u{2060}" + label.text[start...]
            var picture = original
            icon.size = side; icon.sizeEm = false; icon.gap = gap; icon.gapEm = false; icon.capHeight = face.capHeight
            picture.atomIcon = icon
            picture.text = "\u{FFFC}"
            picture.box = InlineBox(width: side + gap, height: 0, align: 0, shift: 0)
            picture.decoration = "none"; picture.underlineColor = nil; picture.strikeColor = nil
            return [prefix, picture, label]
        }
    }
}

extension NodeView {
    /// A success changes paint; a failure also removes the reserved advance.
    func atomIconReady(geometryChanged: Bool) {
        invalidateText()
        #if os(macOS)
        needsDisplay = true
        #else
        setNeedsDisplay()
        #endif
        guard geometryChanged, let session = presenter?.session, session.booted,
              session.state != .destroyed, !session.atomIconRemeasurePending else { return }
        session.atomIconRemeasurePending = true
        // All completions already queued on main share one geometry update.
        DispatchQueue.main.async { [weak session] in
            guard let session else { return }
            session.atomIconRemeasurePending = false
            guard session.booted, session.state != .destroyed else { return }
            session.apply(session.runtime.on { session.runtime.read(exact_text_scale_changed(session.runtime.rt)) })
        }
    }
}

/// Immutable pixels captured before a line reaches any background painter.
final class TextAtomIcon: NSObject, @unchecked Sendable {
    let image: CGImage?
    let tinted: Bool
    let side: CGFloat, gap: CGFloat, capHeight: CGFloat
    let rtl: Bool
    init(_ icon: AtomIconRun) {
        side = icon.size ?? 0; gap = icon.gap ?? 0; capHeight = icon.capHeight; rtl = icon.rtl
        tinted = icon.source.hasPrefix("symbol:sf/")
        if tinted, let symbol = AtomIcons.symbol(icon.source, size: side) {
            #if os(macOS)
            var rect = CGRect(origin: .zero, size: symbol.size)
            image = symbol.cgImage(forProposedRect: &rect, context: nil, hints: nil)
            #else
            image = symbol.cgImage
            #endif
        } else { image = AtomIcons.cached(icon.source, pixels: Int((side * 3).rounded(.up))) }
    }
    /// The same aspect fit and cap-height centre as AtomAttachment, in y-up text coordinates.
    func frame(at x: CGFloat) -> CGRect {
        guard let image else { return .zero }
        let scale = min(side / CGFloat(image.width), side / CGFloat(image.height))
        let width = CGFloat(image.width) * scale, height = CGFloat(image.height) * scale
        return CGRect(x: x + (rtl ? gap : 0) + (side - width) / 2,
                      y: (capHeight - height) / 2, width: width, height: height)
    }
    static func draw(_ line: CTLine, in context: CGContext, ink: CGColor? = nil) {
        for run in CTLineGetGlyphRuns(line) as! [CTRun] { draw(run, in: context, ink: ink) }
    }
    static func draw(_ run: CTRun, in context: CGContext, ink: CGColor? = nil) {
        let attrs = CTRunGetAttributes(run) as NSDictionary
        guard attrs[NSAttributedString.Key.exactHidden] as? Bool != true,
              let icon = attrs[NSAttributedString.Key.exactAtomIcon] as? TextAtomIcon,
              let image = icon.image, CTRunGetGlyphCount(run) > 0 else { return }
        var position = CGPoint.zero
        CTRunGetPositions(run, CFRange(location: 0, length: 1), &position)
        let rect = icon.frame(at: position.x)
        context.saveGState()
        if let font = attrs[kCTFontAttributeName] {
            let font = font as! CTFont
            context.clip(to: CGRect(x: position.x, y: -CTFontGetDescent(font), width: icon.side + icon.gap,
                                    height: CTFontGetAscent(font) + CTFontGetDescent(font) - 1))
        }
        if icon.tinted {
            context.clip(to: rect, mask: image)
            let color = ink ?? (attrs[NSAttributedString.Key.foregroundColor] as? PlatformColor)?.cgColor
                ?? (attrs[kCTForegroundColorAttributeName] as! CGColor?) ?? CGColor(gray: 0, alpha: 1)
            context.setFillColor(color); context.fill(rect)
        } else { context.draw(image, in: rect) }
        context.restoreGState()
    }
}

extension NSAttributedString.Key {
    static let exactAtomIcon = NSAttributedString.Key("ExactAtomIcon")
}
