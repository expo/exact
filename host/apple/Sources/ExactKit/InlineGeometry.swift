// @ref LLP 1051.000 D1 — inline frames are the union of the presented fragments.
import Foundation
import CoreText
import CExact
import ExactCore
#if os(macOS)
import AppKit
private typealias PlatformFont = NSFont
#else
import UIKit
private typealias PlatformFont = UIFont
#endif

extension Runtime {
    func setInlineGeometry(_ read: @escaping (UInt32) -> CGRect?, range: @escaping (UInt32, UInt32, UInt32) -> CGRect?) {
        inlineFrame = read
        rangeFrame = range
        let ctx = UInt(bitPattern: Unmanaged.passUnretained(self).toOpaque())
        on {
            exact_set_inline_geometry(rt, Self.readInlineFrame, Self.readRangeFrame, UnsafeMutableRawPointer(bitPattern: ctx))
        }
    }

    private static let readRangeFrame: ExactRangeFrameFn = { ctx, id, start, end in
        guard let ctx else { return ExactInlineFrame() }
        let runtime = Unmanaged<Runtime>.fromOpaque(ctx).takeUnretainedValue()
        return Owner.shared.callMain {
            guard let rect = runtime.rangeFrame?(id, start, end) else { return ExactInlineFrame() }
            return ExactInlineFrame(x: Double(rect.minX), y: Double(rect.minY), width: Double(rect.width), height: Double(rect.height), available: 1)
        }
    }

    private static let readInlineFrame: ExactInlineFrameFn = { ctx, id in
        guard let ctx else { return ExactInlineFrame() }
        let runtime = Unmanaged<Runtime>.fromOpaque(ctx).takeUnretainedValue()
        return Owner.shared.callMain {
            guard let rect = runtime.inlineFrame?(id) else { return ExactInlineFrame() }
            return ExactInlineFrame(x: Double(rect.minX), y: Double(rect.minY), width: Double(rect.width), height: Double(rect.height), available: 1)
        }
    }
}

extension Presenter {
    /// `rangeFrame` (LLP 1114.007): a textarea's UTF-16 range as TextKit
    /// last laid it out, a plain one's or a Markdown one's source alike, in
    /// viewport points; collapsed, the caret's rectangle. Nil past the
    /// field's text, while text is marked, or when the field is hidden. A
    /// Markdown reader answers its atoms' pills (LLP 1045.000 D7).
    func rangeFrame(_ id: UInt32, start: UInt32, end: UInt32) -> CGRect? {
        if let node = views[id], node.textArea == nil, node.props["markup"] == "markdown" {
            var ancestor: PlatformView? = node
            while let view = ancestor {
                if view.isHidden { return nil }
                ancestor = view.superview
            }
            guard start <= end, node.window != nil, !node.cssVisibilityHidden, let rect = node.readerAtomFrame(Int(start), Int(end)) else { return nil }
            return InspectionGeometry.box(rect, of: node, viewport: viewport)
        }
        guard start <= end, let node = views[id], node.window != nil, !node.isHidden, !node.fieldComposing,
              let field = node.textArea, end <= node.fieldText.utf16.count else { return nil }
        var ancestor: PlatformView? = field
        while let view = ancestor {
            if view.isHidden { return nil }
            ancestor = view.superview
        }
        // Each TextKit 2 segment's text box: its glyphs across and its font's
        // content area down (ascent over descent at the segment's baseline),
        // not the line fragment, which a paragraph's line height makes
        // taller (the grammar's, as Chrome and Linux answer; theme2's 22:27).
        guard let manager = field.textLayoutManager, let content = manager.textContentManager,
              let first = content.location(content.documentRange.location, offsetBy: Int(start)),
              let last = content.location(content.documentRange.location, offsetBy: Int(end)),
              let range = NSTextRange(location: first, end: last) else { return nil }
        manager.ensureLayout(for: range)
        #if os(macOS)
        let origin = field.textContainerOrigin
        guard let storage = field.textStorage else { return nil }
        #else
        let origin = CGPoint(x: field.textContainerInset.left, y: field.textContainerInset.top)
        let storage = field.textStorage
        #endif
        let length = storage.length
        #if os(tvOS)
        // tvOS has no `systemFontSize`; its body style is the system's text size there.
        let systemSize = PlatformFont.preferredFont(forTextStyle: .body).pointSize
        #else
        let systemSize = PlatformFont.systemFontSize
        #endif
        let fallback = field.font ?? field.typingAttributes[.font] as? PlatformFont ?? PlatformFont.systemFont(ofSize: systemSize)
        // A face's content area around `baseline` (its `baselineOffset`
        // raising it), as this host's CSS line box rounds it.
        #if os(macOS)
        let scale = field.pixelScale ?? 1
        #else
        let scale: CGFloat = 1
        #endif
        func area(_ face: PlatformFont, offset: CGFloat, baseline: CGFloat) -> (CGFloat, CGFloat) {
            let (ascent, descent) = CSSLineBox.content(face as CTFont, scale: scale)
            return (baseline - offset - ascent, baseline - offset + descent)
        }
        func face(at index: Int) -> (PlatformFont, CGFloat) {
            guard index >= 0, index < length else { return (fallback, 0) }
            let font = storage.attribute(.font, at: index, effectiveRange: nil) as? PlatformFont ?? fallback
            let offset = (storage.attribute(.baselineOffset, at: index, effectiveRange: nil) as? NSNumber).map { CGFloat($0.doubleValue) } ?? 0
            return (font, offset)
        }
        // The text box over `chars` at `baseline` (y down): every font run's
        // content area in it, or a caret's (the character after it, else
        // before, else the field's font).
        func extent(_ chars: NSRange, caret: Int, baseline: CGFloat) -> (CGFloat, CGFloat) {
            guard chars.length > 0 else {
                let (font, offset) = face(at: caret < length ? caret : caret - 1)
                return area(font, offset: offset, baseline: baseline)
            }
            var (top, bottom) = (CGFloat.infinity, -CGFloat.infinity)
            var at = chars.location
            while at < NSMaxRange(chars) {
                var run = NSRange()
                _ = storage.attributes(at: at, longestEffectiveRange: &run, in: chars)
                let (font, offset) = face(at: at)
                let (t, b) = area(font, offset: offset, baseline: baseline)
                (top, bottom) = (min(top, t), max(bottom, b))
                at = max(NSMaxRange(run), at + 1)
            }
            return (top, bottom)
        }
        let offset = { (location: NSTextLocation) in content.offset(from: content.documentRange.location, to: location) }
        var result = CGRect.null
        // An empty range's `.selection` segment is the insertion point's;
        // TextKit gives each its baseline from the segment's top.
        manager.enumerateTextSegments(in: range, type: start == end ? .selection : .standard, options: []) { segment, rect, baseline, _ in
            let from = segment.map { offset($0.location) } ?? Int(start), to = segment.map { offset($0.endLocation) } ?? from
            let (top, bottom) = extent(NSRange(location: from, length: max(0, to - from)), caret: from, baseline: rect.minY + baseline)
            result = result.union(CGRect(x: rect.minX + origin.x, y: top + origin.y, width: rect.width, height: bottom - top))
            return true
        }
        // No segment (an empty field TextKit has laid nothing out for): the
        // caret's place, and its text box at the line's baseline.
        if result.isNull, start == end, let fragment = manager.textLayoutFragment(for: first) {
            let line = fragment.textLineFragments.last
            let baseline = fragment.layoutFragmentFrame.minY + (line.map { $0.typographicBounds.minY + $0.glyphOrigin.y } ?? 0)
            let (top, bottom) = extent(NSRange(location: Int(start), length: 0), caret: Int(start), baseline: baseline)
            let x = fragment.layoutFragmentFrame.minX + (line?.locationForCharacter(at: 0).x ?? 0)
            result = CGRect(x: x + origin.x, y: top + origin.y, width: 0, height: bottom - top)
        }
        guard !result.isNull, !result.isInfinite else { return nil }
        if start == end { result.size.width = 0 }
        return InspectionGeometry.box(result, of: field, viewport: viewport)
    }

    func inlineFrame(_ id: UInt32) -> CGRect? {
        guard let run = inlineText(id), let owner = textHost(id), owner.window != nil else { return nil }
        let rect = owner.inlineRects(run).map { InspectionGeometry.box($0, of: owner, viewport: viewport) }
            .reduce(CGRect.null) { $0.union($1) }
        return rect.isNull ? nil : rect
    }
}

#if EXACT_AGENT
extension Agent {
    func inlineLayout() -> [[String: Any]] {
        presenter.inlineOwners.keys.sorted().compactMap { id in
            guard let r = presenter.inlineFrame(id) else { return nil }
            return ["id": Int(id), "x": Self.r2(r.minX), "y": Self.r2(r.minY), "w": Self.r2(r.width), "h": Self.r2(r.height)]
        }
    }
}
#endif

extension NodeView {
    /// The glyph that `index` is in, in `line`'s coordinates: its left and
    /// right, whether it runs right to left, and the inline boxes' room
    /// kerned after it (`exactEdgeRoom`), which its advance includes.
    static func glyph(_ line: CTLine, at index: Int) -> (lo: CGFloat, hi: CGFloat, rtl: Bool, room: CGFloat)? {
        for run in CTLineGetGlyphRuns(line) as! [CTRun] {
            let range = CTRunGetStringRange(run), count = CTRunGetGlyphCount(run)
            guard count > 0, index >= range.location, index < range.location + range.length else { continue }
            var indices = [CFIndex](repeating: 0, count: count)
            var positions = [CGPoint](repeating: .zero, count: count)
            var advances = [CGSize](repeating: .zero, count: count)
            CTRunGetStringIndices(run, CFRange(), &indices)
            CTRunGetPositions(run, CFRange(), &positions)
            CTRunGetAdvances(run, CFRange(), &advances)
            // The glyph whose characters start nearest at or before it.
            guard let g = indices.indices.filter({ indices[$0] <= index }).max(by: { indices[$0] < indices[$1] }) else { return nil }
            let a = positions[g].x, b = a + advances[g].width
            let room = (CTRunGetAttributes(run) as NSDictionary)[NSAttributedString.Key.exactEdgeRoom] as? CGFloat ?? 0
            return (min(a, b), max(a, b), CTRunGetStatus(run).contains(.rightToLeft), room)
        }
        return nil
    }

    /// Where the inline box over UTF-16 `extent` is across `line`, in the
    /// line's coordinates: its glyphs, less the room kerned on them
    /// (`exactEdgeRoom`), and the room beside its runs where the line draws
    /// it (`placedRooms`): all of each inner run's and, at its own start and
    /// end, `before` and `after` of it, beside its text. With `hangs`, the
    /// line's trailing white space hangs and has none. Pieces that touch are
    /// one.
    static func boxSpans(_ line: CTLine, _ extent: NSRange, before: CGFloat, after: CGFloat, hangs: Bool) -> [ClosedRange<CGFloat>] {
        var pieces: [(CGFloat, CGFloat)] = []
        // Past the line's width under an `ltr` base, left of its pen under `rtl` (CoreText's).
        let trailing = hangs ? CGFloat(CTLineGetTrailingWhitespaceWidth(line)) : 0
        let width = CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil)) - trailing
        for run in CTLineGetGlyphRuns(line) as! [CTRun] {
            let count = CTRunGetGlyphCount(run)
            guard count > 0 else { continue }
            var indices = [CFIndex](repeating: 0, count: count)
            var positions = [CGPoint](repeating: .zero, count: count)
            var advances = [CGSize](repeating: .zero, count: count)
            CTRunGetStringIndices(run, CFRange(), &indices)
            CTRunGetPositions(run, CFRange(), &positions)
            CTRunGetAdvances(run, CFRange(), &advances)
            let attributes = CTRunGetAttributes(run) as NSDictionary
            let room = attributes[NSAttributedString.Key.exactEdgeRoom] as? CGFloat ?? 0
            let rtlBase = (attributes[NSAttributedString.Key.paragraphStyle] as? NSParagraphStyle)?.baseWritingDirection == .rightToLeft
            let reach = (0..<count).max { positions[$0].x + advances[$0].width < positions[$1].x + advances[$1].width }
            for i in 0..<count where NSLocationInRange(indices[i], extent) {
                let a = positions[i].x, b = a + advances[i].width
                let (lo, hi) = (min(a, b), max(a, b) - (i == reach ? room : 0))
                if trailing > 0, hi > lo, rtlBase ? lo < -0.001 : lo > width - 0.001 { continue }
                pieces.append((lo, hi))
            }
        }
        for room in TextEngine.placedRooms(line) where NSIntersectionRange(room.run.range, extent) == room.run.range {
            let w = min(room.width, room.start && room.run.range.location == extent.location ? before
                        : !room.start && NSMaxRange(room.run.range) == NSMaxRange(extent) ? after : room.width)
            guard w > 0 else { continue }
            let lo = room.textLeft ? room.x : room.x + room.width - w
            pieces.append((lo, lo + w))
        }
        var merged: [ClosedRange<CGFloat>] = []
        for (lo, hi) in pieces.sorted(by: { $0.0 < $1.0 }) where hi >= lo {
            if let last = merged.last, lo <= last.upperBound + 0.01 {
                merged[merged.count - 1] = last.lowerBound...max(last.upperBound, hi)
            } else { merged.append(lo...hi) }
        }
        return merged
    }

    func inlineBounds(_ run: InlineText) -> CGRect? {
        let rect = inlineRects(run).reduce(CGRect.null) { $0.union($1) }
        return rect.isNull ? nil : rect
    }

    func inlineRects(_ run: InlineText) -> [CGRect] {
        guard let text else { return [] }
        let spec = paragraphSpec()
        // Source ranges precede whitespace collapse and inserted hyphens.
        let start = spec.source.collapsed(run.range.location), end = spec.source.collapsed(NSMaxRange(run.range))
        let fit = props["fitScale"].flatMap(Float.init) ?? 1
        let r = run.lightRun
        // A run with no text: the room kerned after its place (`Spec.roomPast`).
        let past = run.range.length == 0 ? inlineText.filter(\.paints).firstIndex { $0.id == run.id }
            .flatMap { spec.runs.indices.contains($0) ? spec.roomPast($0) : nil } : nil
        return shapedRects(start, end, empty: run.range.length == 0, font: text.font(run.run(dark: drawsDark).scaled(fit)) as CTFont,
                           reach: (r.backgroundBefore, r.backgroundAfter, r.backgroundAbove, r.backgroundBelow), past: past ?? 0)
    }

    /// The fragments of the shaped text `start..<end` as the paragraph lays it
    /// out, each its line's slice of `font`'s content area. With `reach`, an
    /// inline box's reach past its text (`InlineBackground.reach`), each its
    /// slice of that box's border box instead (CSS 2 §10.6.1, §8.6), as
    /// WebKit's `getClientRects` answers (`boxSpans`), every slice `above`
    /// and `below` its content area. An empty range's box stands either side
    /// of its place, `past` before the end of the room kerned on the
    /// character before it.
    func shapedRects(_ start: Int, _ end: Int, empty: Bool, font: CTFont, line onlyLine: Int? = nil,
                     reach: (before: CGFloat, after: CGFloat, above: CGFloat, below: CGFloat)? = nil, past: CGFloat = 0) -> [CGRect] {
        guard let paragraph = paragraphLayout() else { return [] }
        let spec = paragraphSpec(), box = paragraphBox()
        let (ascent, descent) = CSSLineBox.content(font, scale: paragraph.lineScale)
        let (above, below) = (ascent + (reach?.above ?? 0), descent + (reach?.below ?? 0))
        #if os(iOS) || os(tvOS)
        let scale = traitCollection.displayScale
        #else
        let scale = pixelScale ?? 1
        #endif
        var placedEmpty = false
        return paragraph.lines.indices.flatMap { index -> [CGRect] in
            if let onlyLine, index != onlyLine { return [] }
            let line = paragraph.lines[index], range = CTLineGetStringRange(line)
            let lo = max(start, range.location), hi = min(end, range.location + range.length)
            if empty {
                guard !placedEmpty, start >= range.location, start <= range.location + range.length else { return [] }
                placedEmpty = true
            } else if hi <= lo { return [] }
            let x = box.minX + paragraph.origin(index, align: spec.align, width: box.width)
            let y = TextEngine.paintedLine(paragraph.baselines[index], top: box.minY, scale: scale, lineScale: paragraph.lineScale) - above
            let spans: [ClosedRange<CGFloat>]
            if empty {
                // After the glyph before it, whose advance has its room, as the painter places it.
                let anchor = lo > range.location ? Self.glyph(line, at: lo - 1) : nil
                let rtl = anchor?.rtl == true
                let place = anchor.map { rtl ? $0.lo + past : $0.hi - past } ?? CTLineGetOffsetForStringIndex(line, lo, nil)
                let (before, after) = rtl ? (reach?.after ?? 0, reach?.before ?? 0) : (reach?.before ?? 0, reach?.after ?? 0)
                spans = [place - before...place + after]
            } else if let reach {
                spans = Self.boxSpans(line, NSRange(location: start, length: end - start), before: reach.before, after: reach.after,
                                      hangs: spec.whiteSpace != 1 && spec.whiteSpace != 4)
                if spans.isEmpty { return [] }
            } else { spans = TextEngine.selectionSpans(line, range: NSRange(location: lo, length: hi - lo)) }
            return spans.compactMap { span in
                var rect = CGRect(x: x + span.lowerBound, y: y, width: span.upperBound - span.lowerBound, height: above + below)
                // An empty inline box keeps its insertion point and font height.
                // A nonempty run covering only hanging wrap space has no fragment.
                // Hanging white space has no fragment: past the line's width
                // under an `ltr` base, left of its pen under `rtl` (CoreText's).
                if !empty && reach == nil && spec.whiteSpace != 1 && spec.whiteSpace != 4 {
                    let trailing = CGFloat(CTLineGetTrailingWhitespaceWidth(line))
                    let width = CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil)) - trailing
                    let start = max(rect.minX, x), end = min(rect.maxX, x + width)
                    guard end > start else { return nil }
                    rect.origin.x = start; rect.size.width = end - start
                }
                if let fragments = columnRecord?.fragments, !fragments.isEmpty {
                    guard let fragment = fragments.first(where: { $0.lines.contains(index) }) else { return nil }
                    rect = rect.offsetBy(dx: fragment.dx, dy: fragment.dy)
                }
                return rect
            }
        }
    }
}
