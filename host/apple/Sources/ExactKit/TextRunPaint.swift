// Per-run `text-shadow` and `-webkit-text-stroke` (LLP 1077 D3, D7): CSS
// paints both per inline box, so each run carries its own computed values
// (a run's style crosses with every inherited row resolved). A paragraph
// whose runs share one shadow can keep it on the paragraph when no inline
// background or opacity group needs separate paint. Other runs carry
// theirs as an attribute the line painter draws with Core
// Graphics around those glyph runs; Core Text ignores an `NSShadow`.
import CoreText
import Foundation
#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// A text node's or run's shadow and stroke rows as they crossed, colours
/// still light/dark pairs: resolved per appearance by `resolve`.


extension Spec {
    /// Promote a shared shadow only when no inline background or opacity
    /// group needs separate paint.
    mutating func gatherShadows() {
        let painted = runs.filter { !$0.text.isEmpty }
        guard let first = painted.first else { shadow = nil; return }
        let tokenShadow = lineClamp > 0 ? (strut ?? first).shadow : first.shadow
        if tokenShadow == first.shadow, painted.allSatisfy({ $0.shadow == first.shadow && $0.opacity.isEmpty && $0.background == nil && $0.outerBackgrounds.isEmpty }) {
            shadow = first.shadow
            for i in runs.indices { runs[i].shadow = nil }
        } else {
            shadow = nil
        }
    }
}

extension Spec {
    /// How far the shadows a raster paints reach past the box to the left
    /// and right, each at most `TextRasterJob.maxShadowReach`: what a band's
    /// clip must admit for them. Runs' own, and the paragraph's when it is
    /// HDR (`hdrShadow`); an SDR paragraph-wide shadow is the layer's,
    /// outside the pixels.
    var shadowReach: (left: CGFloat, right: CGFloat) {
        var left: CGFloat = 0, right: CGFloat = 0
        for shadow in runs.map(\.shadow) + [hdrShadow] {
            guard let s = shadow, TextEngine.isShadow(s) else { continue }
            // As `TextRunShadow.reach`: a Gaussian of σ = blur / 2 is spent by 3σ.
            let spread = s[2] * 1.5 + 1
            left = max(left, spread - s[0]); right = max(right, spread + s[0])
        }
        let cap = TextRasterJob.maxShadowReach
        return (min(left, cap), min(right, cap))
    }
    /// How far the same shadows reach above and below the glyphs that cast
    /// them. Uncapped: a job keeps a shadow anywhere within its room, so in a
    /// tall paragraph a line far from a band can cast into it.
    var shadowRows: (up: CGFloat, down: CGFloat) {
        var up: CGFloat = 0, down: CGFloat = 0
        for shadow in runs.map(\.shadow) + [hdrShadow] {
            guard let s = shadow, TextEngine.isShadow(s) else { continue }
            let spread = s[2] * 1.5 + 1
            up = max(up, spread - s[1]); down = max(down, spread + s[1])
        }
        return (up, down)
    }
}

extension TextEngine {
    /// A `text-shadow`: offset x, y, blur, then a colour as `color` takes it.
    static func isShadow(_ s: [Double]) -> Bool { s.count == 7 || s.count == 12 }

    static func shadowColor(_ s: [Double]) -> CGColor { color(Array(s[3...])).cgColor }
}

/// A run's own shadow as a Core Text attribute: offset x, y and blur in
/// points (CSS's radius), and its colour.
final class TextRunShadow: NSObject {
    let offset: CGSize
    let blur: CGFloat
    let color: CGColor
    /// `s`: offset x, y, blur, then the colour, as `Run.shadow`.
    init(_ s: [Double]) {
        offset = CGSize(width: s[0], height: s[1]); blur = s[2]
        color = TextEngine.shadowColor(s)
    }
    override func isEqual(_ object: Any?) -> Bool {
        guard let other = object as? TextRunShadow else { return false }
        return offset == other.offset && blur == other.blur && color == other.color
    }
    override var hash: Int { offset.width.hashValue ^ offset.height.hashValue ^ blur.hashValue ^ color.hashValue }

    static func of(_ run: CTRun) -> TextRunShadow? {
        (CTRunGetAttributes(run) as NSDictionary)[NSAttributedString.Key.exactShadow] as? TextRunShadow
    }

    /// Core Graphics' shadow is in base space, y up: points in a view's
    /// context, pixels (`scale`) in a bitmap the host made.
    func set(on ctx: CGContext, scale: CGFloat) {
        ctx.setShadow(offset: CGSize(width: offset.width * scale, height: -offset.height * scale), blur: blur * scale, color: color)
    }

    /// The ink a line's run shadows add past its glyphs' (`ink`, y down).
    static func reach(_ line: CTLine, ink: CGRect) -> CGRect {
        var out = CGRect.null
        var seen: [TextRunShadow] = []
        for run in CTLineGetGlyphRuns(line) as! [CTRun] {
            guard let s = of(run), !seen.contains(s) else { continue }
            seen.append(s)
            // A Gaussian of σ = blur / 2 is spent by 3σ.
            let spread = s.blur * 1.5 + 1
            out = out.union(ink.offsetBy(dx: s.offset.width, dy: s.offset.height).insetBy(dx: -spread, dy: -spread))
        }
        return out
    }
}

extension NSAttributedString.Key {
    static let exactHidden = NSAttributedString.Key("ExactRunHidden")
    static let exactShadow = NSAttributedString.Key("ExactRunShadow")
    /// `Run.fixedInk`: the run's colour is one its author fixed.
    static let exactFixedInk = NSAttributedString.Key("ExactRunFixedInk")
}

extension TextLinePaint {
    /// A line's ink in one colour, into a y-down context: how selected text
    /// is drawn over its selection's fill (LLP 1115.000 D7.9). Each glyph at
    /// its own coverage whatever its ink's alpha, so a label colour, which is
    /// part transparent, is solid when selected, as AppKit's is. An underline,
    /// a line-through and a stroke are CoreText's own, recoloured where it
    /// draws them. Emoji and other colour glyphs stay as they are. A hidden
    /// run paints nothing, nor does one whose ink is fully transparent,
    /// unless `transparentInk` (`background-clip: text`, whose glyphs show).
    /// With `keepsFixed` (the light appearance), a run whose colour its
    /// author fixed (`Run.fixedInk`) is drawn as it was, its own colour at
    /// its own alpha and its decorations with it, as an `NSTextView` leaves it.
    static func draw(_ line: CTLine, at origin: CGPoint, in ctx: CGContext, ink: CGColor, keepsFixed: Bool = false, transparentInk: Bool = false) {
        enum Paint { case nothing, own, ink, decorated, keptLined }
        let runs = CTLineGetGlyphRuns(line) as! [CTRun]
        func opacity(_ run: CTRun) -> Double {
            ((CTRunGetAttributes(run) as NSDictionary)[NSAttributedString.Key.exactOpacity] as? [InlineOpacity] ?? []).reduce(1) { $0 * $1.alpha }
        }
        var solid = [Bool](repeating: true, count: runs.count)
        let paints: [Paint] = runs.enumerated().map { i, run in
            let a = CTRunGetAttributes(run) as NSDictionary
            if a[NSAttributedString.Key.exactHidden] as? Bool == true || opacity(run) == 0 { return .nothing }
            if let font = a[kCTFontAttributeName], CTFontGetSymbolicTraits(font as! CTFont).contains(.traitColorGlyphs) { return .own }
            let alpha = (a[NSAttributedString.Key.foregroundColor] as? PlatformColor)?.cgColor.alpha
                ?? (a[kCTForegroundColorAttributeName] as! CGColor?)?.alpha ?? 1
            if alpha == 0, !transparentInk { return .nothing }
            solid[i] = alpha == 1
            let lined = { (key: NSAttributedString.Key) in (a[key] as? NSNumber)?.doubleValue ?? 0 != 0 }
            let decorated = lined(.underlineStyle) || lined(.strikethroughStyle) || lined(.strokeWidth)
            if keepsFixed, alpha > 0, a[NSAttributedString.Key.exactFixedInk] as? Bool == true { return decorated ? .keptLined : .own }
            return alpha > 0 && decorated ? .decorated : .ink
        }
        ctx.saveGState()
        defer { ctx.restoreGState() }
        // As `draw(_:at:in:)`: the CTM flips y, and the caller's text matrix is put back.
        let matrix = ctx.textMatrix
        defer { ctx.textMatrix = matrix }
        ctx.translateBy(x: origin.x, y: origin.y)
        ctx.scaleBy(x: 1, y: -1)
        ctx.textMatrix = .identity
        let decorated = zip(runs, paints).filter { $0.1 == .decorated }.map { extent($0.0) }
        if !decorated.isEmpty {
            // CoreText draws a decoration only with its whole line
            // (`CTRunDraw` draws none): the line, clipped to the runs that
            // have one, with every painted pixel's colour replaced.
            ctx.saveGState()
            ctx.clip(to: decorated)
            ctx.beginTransparencyLayer(auxiliaryInfo: nil)
            ctx.textPosition = .zero
            CTLineDraw(line, ctx)
            ctx.setBlendMode(.sourceIn)
            ctx.setFillColor(ink)
            ctx.fill(decorated)
            ctx.endTransparencyLayer()
            ctx.restoreGState()
        }
        // A kept run's decoration is CoreText's too, in the run's own colour:
        // the line again, clipped to those runs and left as it draws.
        for (run, paint) in zip(runs, paints) where paint == .keptLined {
            ctx.saveGState()
            ctx.setAlpha(opacity(run))
            ctx.clip(to: extent(run))
            ctx.textPosition = .zero
            CTLineDraw(line, ctx)
            ctx.restoreGState()
        }
        ctx.setFillColor(ink)
        for (i, run) in runs.enumerated() {
            if let icon = (CTRunGetAttributes(run) as NSDictionary)[NSAttributedString.Key.exactAtomIcon] as? TextAtomIcon {
                if !icon.tinted || paints[i] != .nothing {
                    let own = !icon.tinted || paints[i] == .own || paints[i] == .keptLined
                    TextAtomIcon.draw(run, in: ctx, ink: own ? nil : ink)
                }
                continue
            }
            switch paints[i] {
            case .nothing, .keptLined: continue
            case .own:
                ctx.saveGState()
                ctx.setAlpha(opacity(run))
                ctx.textPosition = .zero
                CTRunDraw(run, ctx, CFRange())
                ctx.restoreGState()
            case .decorated where solid[i]: continue
            case .ink, .decorated:
                let count = CTRunGetGlyphCount(run)
                guard count > 0, let font = (CTRunGetAttributes(run) as NSDictionary)[kCTFontAttributeName] else { continue }
                var glyphs = [CGGlyph](repeating: 0, count: count), positions = [CGPoint](repeating: .zero, count: count)
                CTRunGetGlyphs(run, CFRange(), &glyphs)
                CTRunGetPositions(run, CFRange(), &positions)
                ctx.textMatrix = CTRunGetTextMatrix(run)
                CTFontDrawGlyphs(font as! CTFont, glyphs, positions, count, ctx)
            }
        }
    }

    /// Where a glyph run lies along its line, from the line's origin, y up:
    /// its glyphs' advance across, and room above and below for any ink.
    private static func extent(_ run: CTRun) -> CGRect {
        let count = CTRunGetGlyphCount(run)
        var positions = [CGPoint](repeating: .zero, count: count), advances = [CGSize](repeating: .zero, count: count)
        CTRunGetPositions(run, CFRange(), &positions)
        CTRunGetAdvances(run, CFRange(), &advances)
        var lo = CGFloat.infinity, hi = -CGFloat.infinity
        for i in 0..<count {
            lo = min(lo, positions[i].x, positions[i].x + advances[i].width)
            hi = max(hi, positions[i].x, positions[i].x + advances[i].width)
        }
        var ascent: CGFloat = 0, descent: CGFloat = 0
        _ = CTRunGetTypographicBounds(run, CFRange(), &ascent, &descent, nil)
        guard lo <= hi else { return .zero }
        return CGRect(x: lo, y: -descent * 2 - 4, width: hi - lo, height: (ascent + descent) * 2 + 8)
    }

    /// Which of a line's paint a pass makes. A raster paints an HDR
    /// `text-shadow` apart from the ink, for a layer of its own (LLP 1100 D8).
    enum Pass {
        /// Everything: a view's own drawing.
        case all
        /// A raster's ink, excluding ungrouped HDR shadows drawn in a separate cast.
        case ink
        /// Only the glyphs of the runs that cast this shadow, with no shadow.
        case caster(TextRunShadow)
        /// Only the inline boxes' backgrounds, under glyphs another view
        /// draws (the Numerals capability's rolling text, `NumeralLink`).
        case backgrounds

        /// An inline background casts no run's shadow.
        var paintsBackgrounds: Bool { if case .caster = self { false } else { true } }
        var paintsGlyphs: Bool { if case .backgrounds = self { false } else { true } }
    }

    /// A line whose glyph runs carry their own shadows, in a context already
    /// at the line's origin, y up: each stretch of runs sharing one shadow
    /// drawn in a transparency layer that casts it, under those glyphs, as
    /// Chrome paints each inline box's shadow with its text. With `selected`
    /// (an opacity group's runs, by index), only those runs, and none of
    /// their neighbours' ink.
    static func drawShadowed(_ line: CTLine, in ctx: CGContext, scale: CGFloat, pass: Pass = .all, selected: Set<Int>? = nil) -> Bool {
        let runs = CTLineGetGlyphRuns(line) as! [CTRun]
        var shadows = runs.map(TextRunShadow.of)
        let advances = runs.map(extent).reduce(CGRect.null) { $0.union($1) }
        func hidden(_ index: Int) -> Bool {
            selected?.contains(index) == false
                || (CTRunGetAttributes(runs[index]) as NSDictionary)[NSAttributedString.Key.exactHidden] as? Bool == true
        }
        func grouped(_ index: Int) -> Bool { (CTRunGetAttributes(runs[index]) as NSDictionary)[NSAttributedString.Key.exactOpacity] != nil }
        func draw(_ shown: [CTRun]) {
            guard !shown.isEmpty else { return }
            if selected != nil { return drawApart(shown, of: line, in: ctx) }
            var clips = shown.map(extent)
            // Decorations occupy advances; italic ink and stroke can pass
            // the line's edges. Admit that ink in the same clip operation,
            // so a fractional edge is not composited twice.
            for run in shown {
                let attributes = CTRunGetAttributes(run) as NSDictionary
                let font = attributes[kCTFontAttributeName] as! CTFont
                let stroke = abs((attributes[NSAttributedString.Key.strokeWidth] as? NSNumber)?.doubleValue ?? 0) * CTFontGetSize(font) / 200
                let ink = CTRunGetImageBounds(run, nil, CFRange()).insetBy(dx: -stroke - 1, dy: -stroke - 1)
                if ink.minX < advances.minX {
                    clips.append(CGRect(x: ink.minX, y: ink.minY, width: min(ink.maxX, advances.minX) - ink.minX, height: ink.height))
                }
                if ink.maxX > advances.maxX {
                    clips.append(CGRect(x: max(ink.minX, advances.maxX), y: ink.minY, width: ink.maxX - max(ink.minX, advances.maxX), height: ink.height))
                }
                if ink.minY < advances.minY {
                    clips.append(CGRect(x: ink.minX, y: ink.minY, width: ink.width, height: min(ink.maxY, advances.minY) - ink.minY))
                }
                if ink.maxY > advances.maxY {
                    clips.append(CGRect(x: ink.minX, y: max(ink.minY, advances.maxY), width: ink.width, height: ink.maxY - max(ink.minY, advances.maxY)))
                }
            }
            ctx.saveGState()
            ctx.clip(to: clips)
            ctx.textPosition = .zero
            CTLineDraw(line, ctx)
            ctx.restoreGState()
            for run in shown { TextAtomIcon.draw(run, in: ctx) }
        }
        switch pass {
        case .all: break
        case .ink:
            // A grouped run's HDR shadow composites inside its group.
            shadows = runs.indices.map { i in shadows[i].flatMap { ColorRange.isHDR($0.color) && !grouped(i) ? nil : $0 } }
        case .caster(let cast):
            draw(runs.indices.filter { shadows[$0] == cast && !hidden($0) && !grouped($0) }.map { runs[$0] })
            return true
        case .backgrounds: return true
        }
        guard selected != nil || shadows.contains(where: { $0 != nil }) || runs.indices.contains(where: hidden) else { return false }
        var i = 0
        while i < runs.count {
            var j = i + 1
            while j < runs.count, shadows[j] == shadows[i] { j += 1 }
            if let s = shadows[i] {
                ctx.saveGState()
                s.set(on: ctx, scale: scale)
                ctx.beginTransparencyLayer(auxiliaryInfo: nil)
            }
            // Decorations belong to CTLineDraw; clip inside the transparency
            // layer so the resulting shadow can extend beyond the run stretch.
            draw((i..<j).filter { !hidden($0) }.map { runs[$0] })
            if shadows[i] != nil { ctx.endTransparencyLayer(); ctx.restoreGState() }
            i = j
        }
        return true
    }

    /// Runs without the rest of their line. `CTRunDraw` draws no decoration,
    /// and Core Text draws one only with its whole line, so the line is drawn
    /// again with invisible glyphs, clipped to each decorated run: below its
    /// baseline before its glyphs (the underline) and above it after them
    /// (the line-through), the order `CTLineDraw` paints them in. Core Text
    /// paints a stroked or colour-glyph run even in the invisible mode, so a
    /// decorated one is drawn whole by the line clipped to it; such a
    /// neighbour's ink that crosses into a decorated run's advance is drawn
    /// with that run.
    private static func drawApart(_ shown: [CTRun], of line: CTLine, in ctx: CGContext) {
        func decorated(_ run: CTRun) -> Bool {
            let attributes = CTRunGetAttributes(run) as NSDictionary
            return CTRunGetGlyphCount(run) > 0 && [NSAttributedString.Key.underlineStyle, .strikethroughStyle].contains { ((attributes[$0] as? NSNumber)?.intValue ?? 0) != 0 }
        }
        func paintsItself(_ run: CTRun) -> Bool {
            let attributes = CTRunGetAttributes(run) as NSDictionary
            if ((attributes[NSAttributedString.Key.strokeWidth] as? NSNumber)?.doubleValue ?? 0) != 0 { return true }
            return attributes[kCTFontAttributeName].map { CTFontGetSymbolicTraits($0 as! CTFont).contains(.traitColorGlyphs) } ?? false
        }
        func drawLine(clippedTo clips: [CGRect]) {
            guard !clips.isEmpty else { return }
            ctx.saveGState()
            ctx.clip(to: clips)
            ctx.setTextDrawingMode(.invisible)
            ctx.textPosition = .zero
            CTLineDraw(line, ctx)
            ctx.restoreGState()
        }
        let lined = shown.filter(decorated)
        let whole = lined.filter(paintsItself), split = lined.filter { !paintsItself($0) }
        func half(_ run: CTRun, above: Bool) -> CGRect {
            let rect = extent(run)
            var origin = CGPoint.zero
            CTRunGetPositions(run, CFRange(location: 0, length: 1), &origin)
            return above ? CGRect(x: rect.minX, y: origin.y, width: rect.width, height: rect.maxY - origin.y)
                : CGRect(x: rect.minX, y: rect.minY, width: rect.width, height: origin.y - rect.minY)
        }
        drawLine(clippedTo: split.map { half($0, above: false) })
        for run in shown where !whole.contains(where: { $0 === run }) {
            ctx.textPosition = .zero
            CTRunDraw(run, ctx, CFRange())
        }
        drawLine(clippedTo: split.map { half($0, above: true) })
        drawLine(clippedTo: whole.map(extent))
        for run in shown { TextAtomIcon.draw(run, in: ctx) }
    }
}

extension RunPaintRows {
    var namesTint: Bool { shadowColor?.namesTint == true || strokeColor?.namesTint == true }
    func resolve(dark: Bool, contrast: Bool? = nil, elevated: Bool = false, tint: PlatformColor? = nil, color: [Double]) -> (shadow: [Double]?, stroke: [Double]?) {
        let shade = shadow.map { $0 + (shadowColor?.textChannels(dark: dark, contrast: contrast, elevated: elevated, tint: tint) ?? color) }
        let stroke = strokeWidth > 0 ? [strokeWidth] + (strokeColor?.textChannels(dark: dark, contrast: contrast, elevated: elevated, tint: tint) ?? color) : nil
        return (shade, stroke)
    }
}
