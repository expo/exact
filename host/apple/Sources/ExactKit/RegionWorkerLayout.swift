import Foundation
import CoreText
import CryptoKit
// Only live layouts own this preparation. It never crosses the worker queue or
// attaches to Sendable source/metadata. Attributed and opaque typesetter storage
// now live as long as their last layout; pixel/index budgets do not cover them.
final class RegionPreparedSource {
    let source: RegionTextSource
    let sourceSHA256: String
    let attributed: NSAttributedString
    let typesetter: CTTypesetter
    let sourceMap: SourceMap
    let emergencyBreaks: [Int]
    // Unicode opportunities depend on this exact source, never its width.
    lazy var lineBreakBoundaries: [Int] = TextEngine.lineBreaks(attributed.string as NSString, length: attributed.length)
    lazy var breakPlan: LineBreakPlan = {
        var plan = LineBreakPlan()
        plan.boundaries = lineBreakBoundaries
        plan.prepare(attributed.string as NSString, length: attributed.length, emergency: emergencyBreaks)
        return plan
    }()

    init(_ source: RegionTextSource) {
        precondition(!Thread.isMainThread, "region preparation must be worker-owned")
        self.source = source
        // Diagnostic identity belongs to this live preparation, not UI capture
        // or a width/history cache. Metadata retains only the immutable string.
        sourceSHA256 = SHA256.hash(data: Data(source.text.utf8)).map { String(format: "%02x", $0) }.joined()
        var spec = Spec(runs: source.runs.map {
            Run(text: $0.text, size: 0, weight: 400, family: 0, italic: false, lineHeight: nil, letterSpacing: 0)
        }, align: source.align, lineClamp: source.lineClamp, color: [],
            overflowWrap: source.overflowWrap, hyphens: source.hyphens, language: source.language)
        // An ordinary reader can capture an already expanded Spec. Its shaped
        // offsets are the captured source's offsets, including emergency points.
        if !source.hyphenated { spec.hyphenateAuto() }
        sourceMap = spec.source
        emergencyBreaks = source.hyphenated ? source.emergencyBreaks : spec.emergencyBreaks
        let value = NSMutableAttributedString(attributedString: source.attributed())
        // Insert backwards into the original styled source; dictionary points
        // lie inside their run, so each inserted hyphen keeps that run's font.
        for point in sourceMap.inserted.reversed() {
            let original = sourceMap.source(point)
            value.insert(NSAttributedString(string: "\u{ad}", attributes: value.attributes(at: original, effectiveRange: nil)), at: original)
        }
        attributed = value
        typesetter = CTTypesetterCreateWithAttributedString(attributed)
    }
}
// This record is deliberately NOT Sendable. CTLines and index survive all
// viewport queries on one queue, then die there before metadata publication.
final class RegionWorkerLayout {
    let source: RegionTextSource
    let lines: [CTLine]
    let baselines: [CGFloat]
    let metadata: RegionParagraph
    let preparation: RegionPreparedSource
    private var viewportLines: [Int: CTLine] = [:]
    var lineCount: Int { metadata.lines.count }
    func beginViewport() { viewportLines.removeAll(keepingCapacity: true) }
    func line(at index: Int) -> CTLine {
        if !lines.isEmpty { return lines[index] }
        if let cached = viewportLines[index] { return cached }
        let r = metadata.lines[index].range
        let range = CFRange(location: r.location, length: r.length)
        let value = TextEngine.finishedLine(CTTypesetterCreateLine(preparation.typesetter, range), source: preparation.attributed,
                                            range: range, justify: source.align == 3 ? Double(metadata.offeredWidth - source.insets.at(r.location).width) : nil)
        if viewportLines.count < 512 { viewportLines[index] = value }
        return value
    }
    /// Immutable event geometry; CoreText stays on this worker.
    func selectionRects(_ selection: NSRange, in box: CGRect) -> [CGRect] {
        precondition(!Thread.isMainThread)
        guard selection.location >= 0, selection.length > 0,
              selection.location <= source.utf16Count,
              selection.length <= source.utf16Count - selection.location else { return [] }
        let selection = metadata.shapedRange(selection)
        var rects: [CGRect] = []
        var low = 0, high = lineCount
        while low < high {
            let mid = low + (high - low) / 2
            if NSMaxRange(metadata.lines[mid].range) <= selection.location { low = mid + 1 } else { high = mid }
        }
        for i in low..<lineCount {
            let range = metadata.lines[i].range
            if range.location >= NSMaxRange(selection) { break }
            let lo = max(selection.location, range.location), hi = min(NSMaxRange(selection), NSMaxRange(range))
            guard hi > lo else { continue }
            let line = line(at: i)
            let x = box.minX + metadata.lines[i].flushOffset
            var above: CGFloat = 0, below: CGFloat = 0
            _ = CTLineGetTypographicBounds(line, &above, &below, nil)
            for span in TextEngine.selectionSpans(line, range: NSRange(location: lo, length: hi - lo)) {
                rects.append(CGRect(x: x + span.lowerBound, y: box.minY + source.painted(baselines[i]) - above,
                                    width: max(1, span.upperBound - span.lowerBound), height: above + below))
            }
        }
        return rects
    }

    private init(source: RegionTextSource, lines: [CTLine], baselines: [CGFloat], metadata: RegionParagraph,
                 preparation: RegionPreparedSource) {
        self.source = source; self.lines = lines; self.baselines = baselines; self.metadata = metadata
        self.preparation = preparation
    }
    static func shape(_ source: RegionTextSource, width: CGFloat, retainHits: Bool = true,
                      preparation: RegionPreparedSource? = nil, compact: Bool = false) -> RegionWorkerLayout {
        shape(source, width: width, retainHits: retainHits, preparation: preparation, compact: compact, beforeMetadata: {})
    }
    static func shape(_ source: RegionTextSource, width: CGFloat, retainHits: Bool = true,
                      preparation: RegionPreparedSource? = nil, compact: Bool = false,
                      beforeMetadata: () throws -> Void) rethrows -> RegionWorkerLayout {
        precondition(!Thread.isMainThread, "region shape must be worker-owned")
        let spec = source
        let preparation = preparation ?? RegionPreparedSource(source)
        precondition(preparation.source === source, "preparation belongs to this exact captured source")
        let attributed = preparation.attributed
        let typesetter = preparation.typesetter
        let length = attributed.length
        let minimum = source.strut?.box ?? TextLineBox()
        var explicit = false
        var lineBottoms: [CGFloat] = []
        var lines: [CTLine] = []
        var summaries: [RegionLine] = []
        let fontInk = source.runs.reduce(CGRect.null) { $0.union(CTFontGetBoundingBox($1.font.value)) }
        let flush: CGFloat = source.align == 1 ? 0.5 : source.align == 2 ? 1 : 0
        var baselines: [CGFloat] = []
        var maxWidth: CGFloat = 0
        var y: CGFloat = 0, lastBaseline: CGFloat = 0
        var start = 0
        var lineCount = 0
        let limit = width.isFinite ? Double(width) : Double.greatestFiniteMagnitude
        var plan = width.isFinite ? preparation.breakPlan : LineBreakPlan()
        let text = attributed.string as NSString
        func next(_ plan: inout LineBreakPlan, _ start: Int, _ room: Double) -> Int {
            plan.next(typesetter, source: attributed, text: text, start: start, room: room, offerWidth: width,
                      breakWord: source.overflowWrap != 0, length: length)
        }
        func removed(_ line: CTLine, _ room: CGFloat) -> CGFloat {
            TextEngine.uncountedTrailingWidth(line, whiteSpace: source.whiteSpace, source: attributed, room: room)
        }
        let balanced = source.balance && spec.lineClamp == 0 && width.isFinite && width > 0
            ? plan.balance(limit, length: length, inset: { Double(source.insets.at($0).width) }, removed: removed,
                           next: next, typesetter: typesetter, source: attributed) : nil
        while start < length {
            if lineCount % 128 == 0 { try beforeMetadata() }
            if spec.lineClamp > 0 && lineCount == spec.lineClamp { break }
            var count: Int
            let inset = source.insets.at(start)
            if source.whiteSpace == 2 {
                count = length - start
            } else if source.whiteSpace == 4 {
                count = CTTypesetterSuggestLineBreak(typesetter, start, Double.greatestFiniteMagnitude)
            } else {
                let room = (balanced.map { start < $0.until ? $0.room : limit } ?? limit) - Double(inset.width)
                count = next(&plan, start, room)
            }
            if count <= 0 { count = length - start }
            var line = TextEngine.inkedSoftHyphen(CTTypesetterCreateLine(typesetter, CFRangeMake(start, count)),
                                                  source: attributed, range: CFRangeMake(start, count))
            var clamps = false
            let room = limit - Double(inset.width)
            var natural: CGFloat? // as TextEngine.layout: a too-long word keeps its width
            if spec.lineClamp > 0 && lineCount + 1 == spec.lineClamp
                && TextEngine.clampsLast(line, start: start, count: count, length: length, width: room) {
                if start + count == length { natural = CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil)) }
                line = TextEngine.clampedLine(attributed, range: NSRange(location: start, length: count), width: room) ?? line
                clamps = true
            }
            var ascent: CGFloat = 0, descent: CGFloat = 0, leading: CGFloat = 0
            // The natural width: justification fills the box, never sizes it.
            let drawn = CGFloat(CTLineGetTypographicBounds(line, &ascent, &descent, &leading))
            let w = max(0, (natural ?? (drawn - removed(line, CGFloat(limit) - inset.width))) + inset.width)
            if spec.align == 3, !clamps, width.isFinite {
                line = TextEngine.justified(line, source: attributed, range: CFRangeMake(start, count), width: limit - Double(inset.width))
            }
            // CSS inline boxes share a baseline. Include the paragraph strut
            // and only the runs on this line, preserving each font's half-leading.
            var box = minimum
            var lineInk = fontInk
            for glyphRun in CTLineGetGlyphRuns(line) as! [CTRun] {
                let range = CTRunGetStringRange(glyphRun)
                let attributes = CTRunGetAttributes(glyphRun) as NSDictionary
                let shapedFont = attributes[kCTFontAttributeName] as! CTFont
                if compact { lineInk = lineInk.union(CTFontGetBoundingBox(shapedFont)) }
                var matched = false, includesNormal = false
                // CoreText can coalesce adjacent spans with the same glyph
                // attributes even when their authored line heights differ.
                let lo = preparation.sourceMap.source(range.location)
                let hi = preparation.sourceMap.source(range.location + range.length)
                for authored in spec.runs(overlapping: NSRange(location: lo, length: hi - lo)) {
                    matched = true
                    if authored.lineHeight != nil {
                        // Explicit boxes use authored metrics; fallback ink
                        // can overflow without enlarging the inline box.
                        box.include(authored.font.box)
                    } else {
                        includesNormal = true
                    }
                }
                if !matched, source.strutExplicit {
                    box.include(minimum)
                    continue
                }
                if matched && !includesNormal { continue }
                // Normal line height includes the actual emoji/fallback face's
                // line box, as the browser's does.
                box.include(TextLineBox(shapedFont, height: nil, scale: source.lineScale))
            }
            explicit = explicit || box.explicit
            if retainHits || lineCount == 0 { baselines.append(y + box.baseline) }
            lastBaseline = y + box.baseline
            y += box.height
            if retainHits { lineBottoms.append(y) }
            maxWidth = max(maxWidth, w)
            if retainHits {
                if compact {
                    summaries.append(RegionLine(line, flush: flush, width: width, rtl: source.direction == 1, whiteSpace: source.whiteSpace,
                                                text: attributed, inset: inset, captureHits: false,
                                                conservativeInk: lineInk.isNull ? .zero : lineInk))
                } else { lines.append(line) }
            }
            lineCount += 1
            start += count
        }
        // An authored line height keeps its fractions; `normal` rounds up once
        // (CSSLineBox); a width rounds up to the 1/64 layout unit.
        // Check before metadata and at its coarse line boundaries. No partial
        // metrics or layout binding escape if the authoritative owner changed.
        try beforeMetadata()
        maxWidth = max(maxWidth, balanced?.widest ?? 0)
        var height = explicit ? y : CSSLineBox.normalHeight(y, scale: source.lineScale)
        // CSS `text-box`, cut as `TextEngine.trimTextBox` cuts a paragraph with lines.
        if source.textBox.trim != 0, lineCount > 0, let root = source.strut?.value, let first = baselines.first {
            let cut = TextEngine.textBoxTrim(source.textBox, root: root, first: first, last: lastBaseline, height: height)
            TextEngine.trimLines(cut, baselines: &baselines, lineBottoms: &lineBottoms, height: &height)
        }
        let metadata: RegionParagraph
        if compact {
            metadata = RegionParagraph(source: source, sourceSHA256: preparation.sourceSHA256,
                lines: summaries, inserted: preparation.sourceMap.inserted, baselines: baselines, lineBottoms: lineBottoms,
                width: CSSLineBox.layoutWidth(maxWidth), height: height, offeredWidth: width)
        } else {
            metadata = try RegionParagraph(source: source, sourceSHA256: preparation.sourceSHA256,
                lines: lines, baselines: baselines, width: CSSLineBox.layoutWidth(maxWidth), height: height,
                lineBottoms: lineBottoms, offeredWidth: width, retainHits: retainHits, captureHits: false,
                attributed: attributed, inserted: preparation.sourceMap.inserted, metadataCheckpoint: beforeMetadata)
        }
        return RegionWorkerLayout(source: source, lines: retainHits ? lines : [], baselines: retainHits ? baselines : [],
                                  metadata: metadata, preparation: preparation)
    }

    func exactIndex(at point: CGPoint, in bounds: CGRect) -> Int {
        precondition(!Thread.isMainThread)
        if point.y < bounds.minY { return 0 }
        if point.y > bounds.maxY { return source.utf16Count }
        guard let i = metadata.lineIndex(at: point.y - bounds.minY) else { return 0 }
        let value = CTLineGetStringIndexForPosition(line(at: i),CGPoint(
            x: point.x - bounds.minX - metadata.lines[i].flushOffset,y: 0))
        return metadata.sourceIndex(value)
    }

    static func minimumWidth(_ source: RegionTextSource) -> CGFloat {
        precondition(!Thread.isMainThread)
        if source.whiteSpace == 2 || source.whiteSpace == 4 {
            return shape(source, width: .infinity, retainHits: false).metadata.width
        }
        var widest: CGFloat = 0
        if source.overflowWrap == 2 {
            let attributed = source.attributed(), text = attributed.string as NSString
            var start = 0
            while start < text.length {
                let range = text.rangeOfComposedCharacterSequence(at: start)
                let line = CTLineCreateWithAttributedString(attributed.attributedSubstring(from: range))
                widest = max(widest, CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil)))
                start = NSMaxRange(range)
            }
            return CSSLineBox.layoutWidth(widest)
        }
        let preparation = RegionPreparedSource(source)
        let attributed = preparation.attributed
        let text = attributed.string as NSString
        let emergency = Set(preparation.emergencyBreaks.map { $0 + 1 })
        let boundaries = preparation.lineBreakBoundaries.filter { !emergency.contains($0) }
        struct Piece: Hashable {
            let text: NSAttributedString
            let hyphenated: Bool
        }
        var words: [Piece: CGFloat] = [:]
        var bytes = 0
        var indent = source.textIndent
        for piece in TextEngine.pieceRanges(text, boundaries: boundaries) {
            let key = Piece(text: attributed.attributedSubstring(from: piece.range), hyphenated: piece.hyphenated)
            let value: CGFloat
            if let cached = words[key] { value = cached }
            else {
                let range = CFRange(location: piece.range.location, length: piece.range.length)
                var line = CTTypesetterCreateLine(preparation.typesetter, range)
                if piece.hyphenated { line = TextEngine.inkedSoftHyphen(line, source: attributed, range: range) }
                value = CSSLineBox.layoutWidth(CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil)))
                let cost = key.text.string.utf8.count + 64
                if cost <= 1024 * 1024 - bytes { words[key] = value; bytes += cost }
            }
            widest = max(widest, value + indent)
            indent = 0
        }
        return widest
    }

}
