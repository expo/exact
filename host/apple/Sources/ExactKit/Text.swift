import ExactCore
import CExact
import CoreText
#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif

final class PresentedText: TextPresentation {
    #if os(macOS)
    var readerParagraphs: [UInt32: RegionReaderParagraph] = [:] {
        didSet { readerLock.lock(); readerViews = Set(readerParagraphs.keys); readerLock.unlock() }
    }
    private let readerLock = NSLock()
    private var readerViews = Set<UInt32>()
    /// Whether a reader paragraph is held for `view`: readable off main.
    func readerHolds(_ view: UInt32) -> Bool {
        readerLock.lock(); defer { readerLock.unlock() }
        return readerViews.contains(view)
    }
    #endif
    var fieldChrome: FieldChromeCache?
    var buttonMeasurements: ButtonMeasureCache?
    var platformControlID: UInt16?
    var platformControlFont: PlatformFont?
    var platformControlName = NSData()
    func reset() {
        #if os(macOS)
        readerParagraphs.removeAll()
        #endif
        platformControlID = nil
    }
    func checkpoint() -> () -> Void {
        let id = platformControlID, font = platformControlFont, name = platformControlName
        return { [self] in platformControlID = id; platformControlFont = font; platformControlName = name }
    }
    func controlFont(_ engine: TextEngine, size: CGFloat, weight: Int, family: Int, italic: Bool) -> PlatformFont? {
        engine.controlFont(size: size, weight: weight, family: family, italic: italic)
    }
    func measure(_ engine: TextEngine, _ request: ExactMeasureRequest) -> ExactMetrics? {
        // The boot's paragraphs, counted (the launch report's `text.measures`).
        if ApplyProfile.on { ApplyProfile.count("text.measures") }
        #if os(macOS)
        return engine.readerMeasureOnUI(request)
        #else
        return nil
        #endif
    }
    func markdown(_ engine: TextEngine, source: Run, atoms: String, faces: String, rtl: Bool) -> [Run]? {
        MarkdownLink.installed?.expand(source.text, base: source, color: nil, boldText: engine.boldTextSteps(source.family), atoms: MarkdownAtoms(kinds: atoms, faces: faces, rtl: rtl))
    }
    func resolveRuns(_ engine: TextEngine, runs: [Run]) -> [Run] { engine.atomIcons(runs) }
    func editorMarkdown(_ engine: TextEngine, source: Run, atoms: String, faces: String, rtl: Bool, width: CGFloat?) -> [Run]? {
        MarkdownLink.installed?.editorRuns(source.text, base: source, atoms: MarkdownAtoms(kinds: atoms, faces: faces, rtl: rtl), width: width) { engine.font($0) }
    }
    func attributes(_ r: Run, color: [Double], scale: CGFloat, attributes a: inout [NSAttributedString.Key: Any]) {
        a[.foregroundColor] = r.color.map(TextEngine.color) ?? TextEngine.color(color)
        if r.hidden { a[.exactHidden] = true }
        if !r.opacity.isEmpty { a[.exactOpacity] = r.opacity }
        if let icon = r.atomIcon { a[.exactAtomIcon] = TextAtomIcon(icon) }
        if r.fixedInk, r.color != nil { a[.exactFixedInk] = true }
        // A centred stroke over the fill: Core Text's negative width,
        // in percent of the run's size (LLP 1077 D7). Each run's own.
        // Its colour four channels, or nine in its own space (a wide colour).
        if let st = r.stroke, st.count == 5 || st.count == 10, st[0] > 0, r.size > 0 {
            a[.strokeWidth] = -st[0] / Double(r.size) * 100
            a[.strokeColor] = TextEngine.color(Array(st[1...]))
        }
        if let sh = r.shadow, TextEngine.isShadow(sh) { a[.exactShadow] = TextRunShadow(sh) }
        // A run's own line is in its own colour and shows over its boxes';
        // a box's alone is in that box's colour (`Run.propagate`).
        if r.atomIcon == nil, r.underlined || r.underlineColor != nil {
            a[.underlineStyle] = NSUnderlineStyle.single.rawValue
            if !r.underlined, let c = r.underlineColor { a[.underlineColor] = TextEngine.color(c) }
        }
        if r.atomIcon == nil, r.decoration.contains("line-through") || r.strikeColor != nil {
            a[.strikethroughStyle] = NSUnderlineStyle.single.rawValue
            if !r.decoration.contains("line-through"), let c = r.strikeColor { a[.strikethroughColor] = TextEngine.color(c) }
        }
        let fill = r.background.map { TextEngine.color($0).cgColor }.flatMap { $0.alpha > 0 ? $0 : nil }
        if !r.hidden, fill != nil || r.border != nil {
            let f = a[.font] as! PlatformFont
            let (ascent, descent) = CSSLineBox.content(f as CTFont, scale: scale)
            let outer = r.outerBackgrounds.map {
                InlineBackground(color: TextEngine.color($0.color).cgColor, ascent: ascent, descent: descent, box: $0.box, radii: $0.radii,
                                 reach: ($0.before, $0.after, $0.above, $0.below), border: $0.border, opacity: $0.opacity)
            }
            a[.exactBackground] = InlineBackground(color: fill ?? CGColor(gray: 0, alpha: 0), ascent: ascent, descent: descent,
                                                   box: r.backgroundBox, radii: r.backgroundRadii,
                                                   reach: (r.backgroundBefore, r.backgroundAfter, r.backgroundAbove, r.backgroundBelow),
                                                   border: r.border, outer: outer, opacity: r.backgroundOpacity)
        }
    }
    /// Every inline box's extent in the paragraph, once its runs are set,
    /// where any box is rounded, reaches past its text, stands in another or
    /// beside an inline box's room: one table that all of its backgrounds
    /// share, so runs of one colour and font stay one CTRun, as they were
    /// measured, and the painter tells boxes apart by character. A box
    /// with no characters (a run with no text, `EmptyRun`) is an empty
    /// extent where it stands, on the character before it (`EmptyBoxes`).
    func finish(_ string: NSMutableAttributedString, empties: [EmptyRun]) {
        let all = NSRange(location: 0, length: string.length)
        var boxes: [InlineBoxes.Box] = [], at: [UInt32: Int] = [:]
        // The room kerned beside a box is no glyph's: its own boxes' reach
        // covers it, read by character.
        var tabled = false
        string.enumerateAttribute(.exactEdgeRoom, in: all) { value, _, stop in
            if value != nil { tabled = true; stop.pointee = true }
        }
        // Its boxes, outermost first, each as deep as the boxes around it;
        // the indices of those it adds.
        @discardableResult func add(_ inner: InlineBackground, _ range: NSRange) -> [Int] {
            var added: [Int] = []
            for (depth, fill) in ([inner] + inner.outer).reversed().enumerated() {
                tabled = tabled || !fill.radii.isEmpty || fill.reach != (0, 0, 0, 0) || fill.border != nil || !inner.outer.isEmpty
                if let i = at[fill.box] {
                    if range.length > 0 { boxes[i].extent = NSUnionRange(boxes[i].extent, range) }
                } else {
                    at[fill.box] = boxes.count
                    added.append(boxes.count)
                    boxes.append(InlineBoxes.Box(extent: range, radii: fill.radii, color: fill.color, depth: depth,
                                                 before: fill.reach.before, after: fill.reach.after,
                                                 above: fill.reach.above, below: fill.reach.below, border: fill.border, opacity: fill.opacity))
                }
            }
            return added
        }
        string.enumerateAttribute(.exactBackground, in: all) { value, range, _ in
            guard let inner = value as? InlineBackground, inner.box != 0 else { return }
            add(inner, range)
        }
        // A run with no text after a character: the boxes no character is in.
        let anchored = empties.compactMap { empty -> (EmptyRun, InlineBackground, [Int])? in
            guard empty.offset > 0, let inner = empty.attributes[.exactBackground] as? InlineBackground, inner.box != 0 else { return nil }
            return (empty, inner, add(inner, NSRange(location: empty.offset, length: 0)))
        }
        guard !boxes.isEmpty else { return }
        let rtl = (string.attribute(.paragraphStyle, at: 0, effectiveRange: nil) as? NSParagraphStyle)?.baseWritingDirection == .rightToLeft
        let table = tabled ? InlineBoxes(boxes, rtl: rtl) : nil
        string.enumerateAttribute(.exactBackground, in: all) { value, range, _ in
            guard let fill = value as? InlineBackground, fill.box != 0 else { return }
            string.addAttribute(.exactBackground, value: fill.sharing(table), range: range)
        }
        guard let table else { return }
        for (offset, group) in Dictionary(grouping: anchored, by: { $0.0.offset }) {
            let range = (string.string as NSString).rangeOfComposedCharacterSequence(at: offset - 1)
            string.addAttribute(.exactEmptyBoxes, value: group.map { EmptyBoxes(fill: $0.1.sharing(table), boxes: $0.2, past: $0.0.past) },
                                range: range)
        }
    }
}

extension TextEngine {
    private var presented: PresentedText { presentation as! PresentedText }
    convenience init(resolve: @escaping (String) -> URL?, read: ((String) -> Data?)? = nil,
                     bundled: @escaping (String) -> URL? = { _ in nil },
                     coldTextTargetBytes: Int = TextResidency.defaultSoftTargetBytes) {
        self.init(assets: resolve, read: read, bundled: bundled, coldTextTargetBytes: coldTextTargetBytes, presentation: PresentedText())
    }
    /// A painting engine with its measurer.
    static func pair(resolve: @escaping (String) -> URL?, read: ((String) -> Data?)? = nil,
                     bundled: @escaping (String) -> URL? = { _ in nil },
                     answerBytes: Int = TextAnswers.defaultLimit) -> TextEngine {
        let painter = TextEngine(resolve: resolve, read: read, bundled: bundled)
        let measurer = TextEngine(resolve: resolve, read: read, bundled: bundled)
        painter.connect(measurer: measurer, answerBytes: answerBytes)
        return painter
    }
    package static func color(_ c: [Double]) -> PlatformColor {
        if c.count == 9, (0...2).contains(Int(c[4])), case let name = [CGColorSpace.extendedSRGB, CGColorSpace.extendedDisplayP3, CGColorSpace.extendedLinearSRGB][Int(c[4])],
           let space = CGColorSpace(name: name), let made = CGColor(colorSpace: space, components: c[5...].map { CGFloat($0) }),
           let color = PlatformColor(cgColor: ColorRange.tagged(made)) as PlatformColor? { return color }
        #if canImport(UIKit)
        return UIColor(red: c[0] / 255, green: c[1] / 255, blue: c[2] / 255, alpha: c[3] / 255)
        #else
        return NSColor(srgbRed: c[0] / 255, green: c[1] / 255, blue: c[2] / 255, alpha: c[3] / 255)
        #endif
    }
    /// A baseline on the raster's device pixels (`scale`). On macOS, on the
    /// device pixels its line box was rounded to (`Paragraph.lineScale`) as
    /// well: whole points where text is laid out at 1 (LLP 1008 §3).
    static func paintedBaseline(_ baseline: CGFloat, scale: CGFloat, lineScale: CGFloat = 1) -> CGFloat {
        #if os(iOS) || os(tvOS)
        let scale = max(1, scale)
        #else
        let scale = max(1, min(scale, lineScale))
        #endif
        return (baseline * scale).rounded() / scale
    }
    /// Where a line whose baseline is `baseline` below `top` paints it, in
    /// the same space as `top`. On the Mac a view is on the device's pixel
    /// grid and its paragraph box where layout put it in the page
    /// (`PixelGridMac.swift`), so the baseline rounds at its place in the
    /// view, which is its place on the page, as Chromium rounds a glyph run's
    /// baseline; on iOS, below `top`.
    static func paintedLine(_ baseline: CGFloat, top: CGFloat, scale: CGFloat, lineScale: CGFloat = 1) -> CGFloat {
        #if os(macOS)
        paintedBaseline(top + baseline, scale: scale, lineScale: lineScale)
        #else
        top + paintedBaseline(baseline, scale: scale, lineScale: lineScale)
        #endif
    }
    /// The device pixels a point of `ctx`. AppKit keeps a view's backing
    /// scale in the context's base transform, out of its CTM (which reads 1
    /// on a 2x display); UIKit's CTM holds it.
    static func deviceScale(_ ctx: CGContext) -> CGFloat {
        #if os(macOS)
        let t = ctx.userSpaceToDeviceSpaceTransform
        #else
        let t = ctx.ctm
        #endif
        return hypot(t.c, t.d)
    }
    /// Paint a paragraph into a y-down context (a flipped NSView's, a
    /// UIView's): one CTLineDraw per line, on device-pixel baselines
    /// (`paintedBaseline`), flush by alignment.
    static func draw(_ p: Paragraph, spec: Spec, in bounds: CGRect, context ctx: CGContext, dirty: CGRect? = nil, pass: TextLinePaint.Pass = .all, lines: Range<Int>? = nil, background: (([CTRun], CGPoint) -> Void)? = nil) {
        // Font smoothing is graphics state (LLP 1019 D12).
        ctx.saveGState()
        defer { ctx.restoreGState() }
        if spec.antialiased { ctx.setShouldSmoothFonts(false) }
        // CSS `text-shadow`: under every glyph of the paragraph at once, so
        // one line's shadow never covers another's text (LLP 1077 D3). Core
        // Graphics' blur is CSS's radius; its offset is base space, y up.
        let shadow = pass.paintsGlyphs ? spec.shadow : nil
        if let s = shadow {
            ctx.saveGState()
            ctx.setShadow(offset: CGSize(width: s[0], height: -s[1]), blur: s[2], color: TextEngine.shadowColor(s))
            ctx.beginTransparencyLayer(auxiliaryInfo: nil)
        }
        defer { if shadow != nil { ctx.endTransparencyLayer(); ctx.restoreGState() } }
        func placed(_ index: Int) -> (CTLine, CGPoint) {
            let line = spec.ellipsis ? p.ellipsized(index, spec: spec, width: bounds.width) : p.lines[index]
            let baseline = p.baselines[index]
            let x = p.origin(index, align: spec.align, width: bounds.width)
            return (line, CGPoint(x: bounds.minX + x, y: Self.paintedLine(baseline, top: bounds.minY, scale: deviceScale(ctx), lineScale: p.lineScale)))
        }
        if spec.runs.contains(where: { !$0.opacity.isEmpty }) {
            let shown = p.lines.indices.filter { lines?.contains($0) != false }.map(placed)
            if TextLinePaint.drawOpacity(shown.map(\.0), at: shown.map(\.1), in: ctx, scale: 1, pass: pass, background: background) { return }
        }
        func paint(_ index: Int) {
            if let lines, !lines.contains(index) { return }
            let (line, origin) = placed(index)
            background?(CTLineGetGlyphRuns(line) as! [CTRun], origin)
            TextLinePaint.draw(line, at: origin, in: ctx, pass: pass)
        }
        if let dirty {
            // A background's block padding paints past its line's ink, by as
            // much as any box's (`Run.backgroundAbove`).
            var above: CGFloat = 0, below: CGFloat = 0
            for run in spec.runs where run.background != nil || run.border != nil {
                above = run.outerBackgrounds.reduce(max(above, run.backgroundAbove)) { max($0, $1.above) }
                below = run.outerBackgrounds.reduce(max(below, run.backgroundBelow)) { max($0, $1.below) }
            }
            p.inkBounds().forEachLine(from: dirty.minY - bounds.minY - below,
                                     through: dirty.maxY - bounds.minY + above, paint)
        } else {
            for index in p.lines.indices { paint(index) }
        }
    }
    var fieldChrome: FieldChromeCache? { get { presented.fieldChrome } set { presented.fieldChrome = newValue } }
    var buttonMeasurements: ButtonMeasureCache? { get { presented.buttonMeasurements } set { presented.buttonMeasurements = newValue } }
    var platformControlID: UInt16? { get { presented.platformControlID } set { presented.platformControlID = newValue } }
    var platformControlFont: PlatformFont? { get { presented.platformControlFont } set { presented.platformControlFont = newValue } }
    var platformControlName: NSData { get { presented.platformControlName } set { presented.platformControlName = newValue } }
    #if os(macOS)
    var readerParagraphs: [UInt32: RegionReaderParagraph] { get { presented.readerParagraphs } set { presented.readerParagraphs = newValue } }
    func readerHolds(_ view: UInt32) -> Bool { presented.readerHolds(view) }
    #endif
}

/// An inline box's `background-color` (clear for none) and border, carried
/// on the attributed source so that the main thread's paint and a worker's
/// raster read the same value. The content area is the run's own font's, as
/// the web's inline box is. `box`, `radii` and `reach` are the run's box,
/// its `border-radius` and how far its padding box stands out from its text
/// (`before` its first character, `after` its last, `above` and `below` its
/// content area, which moves no line), and `outer` the boxes around it with
/// backgrounds or borders, nearest first, until `PresentedText.finish` puts
/// the paragraph's table of boxes in their place.
final class InlineBackground: NSObject, @unchecked Sendable {
    let color: CGColor
    let ascent: CGFloat
    let descent: CGFloat
    let box: UInt32
    let radii: [InlineRadius]
    let reach: (before: CGFloat, after: CGFloat, above: CGFloat, below: CGFloat)
    let border: InlineBorder?
    let outer: [InlineBackground]
    let boxes: InlineBoxes?
    let opacity: [InlineOpacity]
    init(color: CGColor, ascent: CGFloat, descent: CGFloat, box: UInt32 = 0, radii: [InlineRadius] = [],
         reach: (before: CGFloat, after: CGFloat, above: CGFloat, below: CGFloat) = (0, 0, 0, 0), border: InlineBorder? = nil,
         outer: [InlineBackground] = [], boxes: InlineBoxes? = nil, opacity: [InlineOpacity] = []) {
        self.opacity = opacity
        self.color = color; self.ascent = ascent; self.descent = descent
        self.box = box; self.radii = radii.contains { !$0.isZero } ? radii : []; self.reach = reach; self.border = border
        self.outer = outer; self.boxes = boxes
    }
    func sharing(_ boxes: InlineBoxes?) -> InlineBackground {
        InlineBackground(color: color, ascent: ascent, descent: descent, boxes: boxes, opacity: opacity)
    }
    override func isEqual(_ object: Any?) -> Bool {
        guard let other = object as? InlineBackground else { return false }
        return color == other.color && ascent == other.ascent && descent == other.descent && box == other.box
            && opacity == other.opacity && radii == other.radii && reach == other.reach && border == other.border
            && outer == other.outer && boxes == other.boxes
    }
    override var hash: Int { color.hashValue ^ ascent.hashValue ^ descent.hashValue ^ box.hashValue }
}

/// A paragraph's inline boxes with backgrounds or borders
/// (`PresentedText.finish`): each box's range of characters, its
/// `border-radius`, its colour, how many such boxes it stands in, its reach
/// (`InlineBackground.reach`) and its border, and the paragraph's
/// direction, which says which side a box starts on.
final class InlineBoxes: NSObject {
    struct Box: Equatable {
        var extent: NSRange; let radii: [InlineRadius]; let color: CGColor; let depth: Int
        var before: CGFloat = 0, after: CGFloat = 0, above: CGFloat = 0, below: CGFloat = 0
        var border: InlineBorder?
        var opacity: [InlineOpacity] = []
    }
    let boxes: [Box]
    let rtl: Bool
    init(_ boxes: [Box], rtl: Bool) { self.boxes = boxes; self.rtl = rtl }
    override func isEqual(_ object: Any?) -> Bool {
        guard let other = object as? InlineBoxes else { return false }
        return self === other || boxes == other.boxes && rtl == other.rtl
    }
    override var hash: Int { boxes.count }
}

/// The boxes of a run with no text (`EmptyRun`), carried by the character
/// before it: its own background, for its content area and the table, the
/// boxes no character is in, outermost first, and the room kerned after
/// its place (`Spec.roomPast`).
final class EmptyBoxes: NSObject, @unchecked Sendable {
    let fill: InlineBackground, boxes: [Int], past: CGFloat
    init(fill: InlineBackground, boxes: [Int], past: CGFloat) { self.fill = fill; self.boxes = boxes; self.past = past }
}

extension NSAttributedString.Key {
    static let exactOpacity = NSAttributedString.Key("ExactInlineOpacity")
    static let exactBackground = NSAttributedString.Key("ExactInlineBackground")
    static let exactEmptyBoxes = NSAttributedString.Key("ExactInlineEmptyBoxes")
}

/// One line's paint into a y-down context, shared by every Apple painter.
enum TextLinePaint {
    /// The CTM, not the text matrix, flips y: CoreText positions a glyph
    /// in text space, so a flipped text matrix turned the vertical offsets
    /// of cursive attachment and marks (SF Arabic's) upside down, and that
    /// ink fell below the line box it was measured in.
    /// `scale`: base-space units per point, for a run's own shadow (1 in a
    /// view's context, the pixel scale in a bitmap the host made).
    static func draw(_ line: CTLine, at origin: CGPoint, in ctx: CGContext, scale: CGFloat = 1, pass: Pass = .all) {
        if drawOpacity(line, at: origin, in: ctx, scale: scale, pass: pass) { return }
        ctx.saveGState()
        for fill in backgrounds(line, at: origin) where pass.paintsBackgrounds { fill.paint(in: ctx) }
        guard pass.paintsGlyphs else { ctx.restoreGState(); return }
        // The text matrix is not graphics state; put the caller's back.
        let matrix = ctx.textMatrix
        ctx.translateBy(x: origin.x, y: origin.y)
        ctx.scaleBy(x: 1, y: -1)
        ctx.textMatrix = .identity
        ctx.textPosition = .zero
        if !drawShadowed(line, in: ctx, scale: scale, pass: pass) {
            CTLineDraw(line, ctx)
            TextAtomIcon.draw(line, in: ctx)
        }
        ctx.textMatrix = matrix
        ctx.restoreGState()
    }

    /// A glyph run's left and right, its advances included, and whether it
    /// runs right to left.
    static func glyphSpan(_ run: CTRun) -> (CGFloat, CGFloat, Bool)? {
        let count = CTRunGetGlyphCount(run)
        guard count > 0 else { return nil }
        var positions = [CGPoint](repeating: .zero, count: count), advances = [CGSize](repeating: .zero, count: count)
        CTRunGetPositions(run, CFRange(), &positions)
        CTRunGetAdvances(run, CFRange(), &advances)
        let xs = zip(positions, advances).flatMap { [$0.x, $0.x + $1.width] }
        return (xs.min()!, xs.max()!, CTRunGetStatus(run).contains(.rightToLeft))
    }

    /// One box's fragment: its border box, its outline where a corner is
    /// rounded, its background colour, and its border with no width where
    /// the box is sliced, which paints over the background.
    struct Fill {
        let rect: CGRect; let path: CGPath?; let color: CGColor; var opacity: [InlineOpacity] = []
        var border: (widths: [CGFloat], colors: [CGColor], styles: [String], radii: [CGSize])?
        func paint(in ctx: CGContext) {
            if color.alpha > 0 {
                ctx.setFillColor(color)
                if let path { ctx.addPath(path); ctx.fillPath() } else { ctx.fill(rect) }
            }
            if let border {
                BorderPaint.paint(ctx, box: rect, widths: border.widths, colors: border.colors, radii: border.radii, styles: border.styles)
            }
        }
    }

    /// CSS: an inline box's background covers each of its line fragments,
    /// its glyphs' advance across and its font's content area down, and its
    /// padding past its first and last characters. Glyph runs that fallback
    /// or bidi split are joined again where they touch, within one box; two
    /// boxes side by side stay two, and a box inside another paints over it
    /// (CSS 2 Appendix E: each inline box's background in tree order). The
    /// room kerned on a glyph for the boxes beside it (`exactEdgeRoom`) is
    /// no glyph's: a box covers it where the line draws it (`placedRooms`).
    /// A box's `border-radius` rounds only its own start and end (`slice`),
    /// in visual order as CSS 2's §8.6 places them: under `ltr` the start on
    /// the leftmost piece of the line that holds its first character and
    /// the end on the rightmost of the line that holds its last; under
    /// `rtl` the other way about. Percentages resolve against each piece.
    /// Its border's start and end sides are sliced the same way, and its
    /// top and bottom are on every piece.
    static func backgrounds(_ line: CTLine, at origin: CGPoint) -> [Fill] {
        struct Piece { var lo: CGFloat, hi: CGFloat; let box: Int?; let color: CGColor; let fill: InlineBackground }
        var spans: [Piece] = []
        // The boxes whose first and whose last character is on this line.
        var starts = Set<Int>(), ends = Set<Int>()
        let runs = CTLineGetGlyphRuns(line) as! [CTRun]
        // CSS removes the collapsible spaces that end a line (CSS Text 3
        // §4.1.3): a wrapped run's background stops at its last glyph, as the
        // browser paints it. They
        // hang past the line's end edge: its width under an `ltr` base (LLP
        // 1053 §0, the Linux parity), left of its pen under `rtl` (CoreText's).
        let rtlBase = runs.first.flatMap { (CTRunGetAttributes($0) as NSDictionary)[NSAttributedString.Key.paragraphStyle] as? NSParagraphStyle }?
            .baseWritingDirection == .rightToLeft
        let trailing = CGFloat(CTLineGetTrailingWhitespaceWidth(line))
        let start = rtlBase && trailing > 0 ? 0 : -CGFloat.infinity
        let end = !rtlBase && trailing > 0 ? CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil)) - trailing : CGFloat.infinity
        // Each run's background, for the room beside it (`placedRooms`).
        var fills: [Int: InlineBackground] = [:]
        for run in runs {
            let attributes = CTRunGetAttributes(run) as NSDictionary
            // The boxes of runs with no text after this one's last character:
            // each its reach either side of its place, starting and ending there.
            for empty in attributes[NSAttributedString.Key.exactEmptyBoxes] as? [EmptyBoxes] ?? [] {
                guard let (lo, hi, backwards) = Self.glyphSpan(run), let table = empty.fill.boxes else { continue }
                let place = backwards ? lo + empty.past : hi - empty.past
                for b in empty.boxes {
                    let box = table.boxes[b]
                    let (before, after) = backwards ? (box.after, box.before) : (box.before, box.after)
                    spans.append(Piece(lo: place - before, hi: place + after, box: b, color: box.color, fill: empty.fill))
                    starts.insert(b); ends.insert(b)
                }
            }
            guard let fill = attributes[NSAttributedString.Key.exactBackground] as? InlineBackground else { continue }
            if let room = attributes[NSAttributedString.Key.exactRoomRun] as? InlineRoomRun { fills[room.index] = fill }
            let count = CTRunGetGlyphCount(run)
            guard count > 0 else { continue }
            var positions = [CGPoint](repeating: .zero, count: count)
            var advances = [CGSize](repeating: .zero, count: count)
            CTRunGetPositions(run, CFRange(), &positions)
            CTRunGetAdvances(run, CFRange(), &advances)
            guard let boxes = fill.boxes else {
                var lo = CGFloat.infinity, hi = -CGFloat.infinity
                for i in 0..<count {
                    lo = min(lo, positions[i].x, positions[i].x + advances[i].width)
                    hi = max(hi, positions[i].x, positions[i].x + advances[i].width)
                }
                (lo, hi) = (max(lo, start), min(hi, end))
                if hi > lo { spans.append(Piece(lo: lo, hi: hi, box: nil, color: fill.color, fill: fill)) }
                continue
            }
            // Each glyph's characters: from its index to the next one up.
            var indices = [CFIndex](repeating: 0, count: count)
            CTRunGetStringIndices(run, CFRange(), &indices)
            let range = CTRunGetStringRange(run)
            let stops = Set(indices).sorted() + [range.location + range.length]
            let next = Dictionary(uniqueKeysWithValues: zip(stops.dropLast(), stops.dropFirst()))
            // The ellipsis stands for what was cut, not for the box's end.
            let token = attributes[NSAttributedString.Key.exactTruncation] != nil
            // The room kerned on a glyph is no glyph's ink (`exactEdgeRoom`):
            // it is right of the glyph that reaches furthest.
            let room = attributes[NSAttributedString.Key.exactEdgeRoom] as? CGFloat ?? 0
            let reach = (0..<count).max { positions[$0].x + advances[$0].width < positions[$1].x + advances[$1].width }
            let backwards = CTRunGetStatus(run).contains(.rightToLeft)
            for i in 0..<count {
                let from = indices[i], to = next[from]!, glyph = NSRange(location: from, length: to - from)
                let lo = max(start, min(positions[i].x, positions[i].x + advances[i].width))
                let hi = min(end, max(positions[i].x, positions[i].x + advances[i].width) - (i == reach ? room : 0))
                for (b, box) in boxes.boxes.enumerated() {
                    let mine = NSIntersectionRange(box.extent, glyph)
                    guard mine.length > 0 else { continue }
                    // Where a box starts or ends, whatever this glyph's advance (a
                    // trimmed space, a zero-width or combining character).
                    if !token, from <= box.extent.location { starts.insert(b) }
                    if !token, NSMaxRange(box.extent) <= to { ends.insert(b) }
                    guard hi > lo else { continue }
                    // A glyph of several characters in several boxes (a ligature
                    // across two) is shared between them by characters, in the
                    // run's direction, as a caret divides a ligature.
                    let share = (hi - lo) / CGFloat(to - from)
                    let near = CGFloat(mine.location - from) * share, far = CGFloat(NSMaxRange(mine) - from) * share
                    let (a, z) = backwards ? (hi - far, hi - near) : (lo + near, lo + far)
                    if z > a { spans.append(Piece(lo: a, hi: z, box: b, color: box.color, fill: fill)) }
                }
            }
        }
        // The room beside a run, where the line draws it: a box covers the
        // room of each run inside it, and at its own start and end only its
        // padding and border (its reach), beside its text.
        for room in TextEngine.placedRooms(line) {
            guard let fill = fills[room.run.index], let table = fill.boxes else { continue }
            for (b, box) in table.boxes.enumerated() where NSIntersectionRange(box.extent, room.run.range).length > 0 {
                let w = min(room.width, room.start && box.extent.location == room.run.range.location ? box.before
                            : !room.start && NSMaxRange(box.extent) == NSMaxRange(room.run.range) ? box.after : room.width)
                guard w > 0 else { continue }
                let lo = room.textLeft ? room.x : room.x + room.width - w
                spans.append(Piece(lo: lo, hi: lo + w, box: b, color: box.color, fill: fill))
            }
        }
        // Outer boxes first, so that a box inside another paints over it.
        let depth = { (p: Piece) in p.box.flatMap { p.fill.boxes?.boxes[$0].depth } ?? 0 }
        spans.sort { (depth($0), $0.lo) < (depth($1), $1.lo) }
        var merged: [Piece] = []
        for span in spans {
            if let last = merged.last, last.box == span.box, span.box != nil || last.fill.isEqual(span.fill), span.lo <= last.hi + 0.01 {
                merged[merged.count - 1].hi = max(last.hi, span.hi)
            } else { merged.append(span) }
        }
        // top-left, top-right, bottom-right, bottom-left, for each piece
        var kept = [[Bool]](repeating: [false, false, false, false], count: merged.count)
        for b in Set(merged.compactMap(\.box)) {
            let mine = merged.indices.filter { merged[$0].box == b }
            guard let rtl = merged[mine[0]].fill.boxes?.rtl, let left = mine.first, let right = mine.last else { continue }
            if rtl ? ends.contains(b) : starts.contains(b) { kept[left][0] = true; kept[left][3] = true }
            if rtl ? starts.contains(b) : ends.contains(b) { kept[right][1] = true; kept[right][2] = true }
        }
        return merged.indices.map { i in
            let piece = merged[i], fill = piece.fill, box = piece.box.flatMap { fill.boxes?.boxes[$0] }
            let above = fill.ascent + (box?.above ?? 0), below = fill.descent + (box?.below ?? 0)
            let rect = CGRect(x: origin.x + piece.lo, y: origin.y - above, width: piece.hi - piece.lo, height: above + below)
            let corners = zip(box?.radii ?? [], kept[i]).map { radius, kept in
                kept ? CGSize(width: radius.length(basis: rect.width), height: radius.length(basis: rect.height)) : .zero
            }
            let rounded = corners.contains { $0 != .zero }
            var out = Fill(rect: rect, path: rounded ? BorderPaint.roundedRect(rect, BorderPaint.reduced(corners, in: rect)) : nil,
                           color: piece.color, opacity: box?.opacity ?? fill.opacity)
            if let border = box?.border {
                let w = border.widths
                out.border = ([w[0], kept[i][1] ? w[1] : 0, w[2], kept[i][0] ? w[3] : 0], border.colors.map { TextEngine.color($0).cgColor },
                              border.styles, rounded ? corners : Array(repeating: .zero, count: 4))
            }
            return out
        }
    }
}
