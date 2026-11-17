// CSS line breaking on Apple (LLP 1008 §3, CSS Text 3 §5): where a line may
// end, and min-content's unbreakable pieces, from the shared walker's
// opportunities (UAX #14 as Chrome tailors it, LLP 1043 §4 C) rather than
// CoreText's or CFStringTokenizer's, which break after a `/` before a letter.
import Foundation
import CoreText
import CExact

/// One paragraph layout's opportunities and where it has got to in them.
package struct LineBreakPlan {
    package var boundaries: [Int] = []
    package var contentEnds: [Int] = []
    package var hardEnds: [Int] = []
    package var boundaryIndex = 0, hardIndex = 0
    private var ordinary: [Int] = [], ordinaryContent: [Int] = []
    private var ordinaryIndex = 0

    /// The opportunities a line ends at by choice, each with its content
    /// end: all of them less the emergency hyphens, which only a word too
    /// wide for its line breaks at (`suggest`).
    private var chosen: (boundaries: [Int], contentEnds: [Int]) {
        ordinary.isEmpty ? (boundaries, contentEnds) : (ordinary, ordinaryContent)
    }

    package init() {}

    package mutating func prepare(_ text: NSString, length: Int, emergency: [Int] = []) {
        (contentEnds, hardEnds) = TextEngine.breakEnds(text, boundaries: boundaries, length: length)
        if !emergency.isEmpty {
            let ends = Set(emergency.map { $0 + 1 })
            ordinary = boundaries.filter { !ends.contains($0) }
            ordinaryContent = TextEngine.breakEnds(text, boundaries: ordinary, length: length).0
        }
    }
    package mutating func advance(to start: Int) {
        while boundaryIndex < boundaries.count && boundaries[boundaryIndex] <= start { boundaryIndex += 1 }
        while ordinaryIndex < ordinary.count && ordinary[ordinaryIndex] <= start { ordinaryIndex += 1 }
        while hardIndex < hardEnds.count && hardEnds[hardIndex] <= start { hardIndex += 1 }
    }
    /// The line's length from `start` in `room`. No opportunities (an
    /// infinite offer): CoreText's own break. A zero-width offer keeps
    /// CoreText's degenerate breaking, as the region worker does, a `normal`
    /// word kept whole.
    package func suggest(_ typesetter: CTTypesetter, start: Int, room: Double, offerWidth: CGFloat, breakWord: Bool, length: Int, text: NSString) -> Int {
        guard !boundaries.isEmpty else { return CTTypesetterSuggestLineBreak(typesetter, start, room) }
        if offerWidth <= 0 {
            var count = CTTypesetterSuggestLineBreak(typesetter, start, room)
            if !breakWord {
                var i = boundaryIndex
                while i < boundaries.count && boundaries[i] < start + count { i += 1 }
                if i < boundaries.count { count = boundaries[i] - start }
            }
            return count
        }
        // A forced break ends the line wherever the room is.
        let hard = (hardIndex < hardEnds.count ? hardEnds[hardIndex] : length) - start
        let fitted = min(CTTypesetterSuggestClusterBreak(typesetter, start, room), hard)
        // UILabel keeps a whole word on its own line before using a syllable
        // break inside it. Authored soft hyphens remain ordinary opportunities.
        if !ordinary.isEmpty {
            var i = ordinaryIndex
            while i < ordinary.count && ordinary[i] - start <= hard && ordinaryContent[i] <= start + fitted { i += 1 }
            if i > ordinaryIndex { return ordinary[i - 1] - start }
        }
        return TextEngine.cssBreak(start: start, fitted: fitted, hard: hard, boundaries: boundaries, contentEnds: contentEnds,
                                   from: boundaryIndex, breakWord: breakWord, text: text)
    }
    /// The next line's length from `start` in `room`, as `suggest`, where a
    /// soft hyphen's break shows one, which must fit too (`fitSoftHyphen`).
    package mutating func next(_ typesetter: CTTypesetter, source: NSAttributedString, text: NSString, start: Int, room: Double,
                       offerWidth: CGFloat, breakWord: Bool, length: Int) -> Int {
        advance(to: start)
        let plan = self
        func suggest(_ room: Double) -> Int {
            plan.suggest(typesetter, start: start, room: room, offerWidth: offerWidth, breakWord: breakWord, length: length, text: text)
        }
        let count = TextEngine.fitSoftHyphen(start: start, count: suggest(room), room: room, boundaries: boundaries, source: source,
                                             typesetter: typesetter, length: length, retry: suggest)
        return TextEngine.breakBeforeRoom(typesetter, source: source, start: start, count: count, room: room, boundaries: boundaries,
                                          hard: hardIndex < hardEnds.count ? hardEnds[hardIndex] : length)
    }

    /// CSS `text-wrap-style: balance` (LLP 1001 §6) as Blink does it
    /// (`exact_textflow`'s balance): the lengths of the lines before the first
    /// forced break, when those are two to six at `limit` and none overflows.
    /// They break where Blink's scorer puts them; where it gives nothing and
    /// the text has no forced break, at its bisection's width
    /// (`TextEngine.balancedLimit`); nil keeps the greedy lines. `widest` is
    /// those lines' widest broken greedily, which is still the box's measure:
    /// balancing moves breaks, never the box. `fontSize` is the paragraph's
    /// own. Call before the layout advances this plan; `next` is the layout's
    /// own break, and `removed` the trailing spaces it leaves out of a line's
    /// width, given the line's room.
    package func balance(_ limit: Double, length: Int, fontSize: CGFloat, justified: Bool, insets: LineInsets,
                 removed: (CTLine, CGFloat) -> CGFloat, next: (inout LineBreakPlan, Int, Double) -> Int,
                 typesetter: CTTypesetter, source: NSAttributedString) -> (counts: [Int], widest: CGFloat)? {
        let until = hardEnds.first ?? length, ends = Set(chosen.boundaries)
        func inset(_ start: Int) -> Double { Double(insets.at(start).width) }
        // The greedy lines' lengths at `limit`, with their widths' sum and
        // widest, or nil for more than six or one that overflows its room.
        func greedy(_ limit: Double) -> (counts: [Int], total: CGFloat, widest: CGFloat)? {
            var plan = self, start = 0, counts: [Int] = [], total: CGFloat = 0, widest: CGFloat = 0
            while start < until {
                guard counts.count < TextEngine.balanceMaxLines else { return nil }
                let room = limit - inset(start)
                var count = next(&plan, start, room)
                if count <= 0 { count = length - start }
                let range = CFRangeMake(start, count)
                let line = TextEngine.inkedSoftHyphen(CTTypesetterCreateLine(typesetter, range), source: source, range: range)
                let content = CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil)) - removed(line, CGFloat(room))
                // A word wider than its room, or one broken inside (`break-word`, at an emergency hyphen too).
                let end = start + count
                if content > CGFloat(room) + 1.0 / 64 || (end < until && !ends.contains(end)) { return nil }
                total += content + CGFloat(inset(start)); widest = max(widest, content + CGFloat(inset(start)))
                counts.append(count); start = end
            }
            return (counts, total, widest)
        }
        guard limit.isFinite, limit > 0, let lines = greedy(limit), lines.counts.count >= 2 else { return nil }
        if let counts = scored(limit, lines: lines.counts.count, until: until, fontSize: fontSize, justified: justified, insets: insets,
                               typesetter: typesetter, source: source) {
            return (counts, lines.widest)
        }
        guard hardEnds.isEmpty,
              let room = TextEngine.balancedLimit(limit, count: lines.counts.count, total: Double(lines.total), fits: {
                  greedy($0).map { $0.counts.count <= lines.counts.count } ?? false
              }), let narrow = greedy(room) else { return nil }
        return (narrow.counts, lines.widest)
    }

    /// The lengths of the `lines` lines Blink's scorer breaks the text before
    /// `until` into (`exact_text_balanced_breaks`), over this plan's
    /// opportunities, the emergency hyphens left out, and the typesetter's
    /// advances; nil where it gives nothing. A tab keeps it out, as in Blink.
    /// A later run's start room (`LineInsets.edgeStarts`) is kerned on the
    /// character before the run and belongs to the line the run is on: a
    /// line that starts with the run starts before the room, and one that
    /// ends before the run ends before it.
    private func scored(_ limit: Double, lines: Int, until: Int, fontSize: CGFloat, justified: Bool, insets: LineInsets,
                        typesetter: CTTypesetter, source: NSAttributedString) -> [Int]? {
        let text = source.string as NSString
        guard until > 0, text.range(of: "\t", range: NSRange(location: 0, length: until)).location == NSNotFound else { return nil }
        // Each UTF-16 unit's place from the start: its glyphs' advances, in logical order.
        var place = [CGFloat](repeating: 0, count: until + 1)
        for run in CTLineGetGlyphRuns(CTTypesetterCreateLine(typesetter, CFRangeMake(0, until))) as! [CTRun] {
            let count = CTRunGetGlyphCount(run)
            var advances = [CGSize](repeating: .zero, count: count), indices = [CFIndex](repeating: 0, count: count)
            CTRunGetAdvances(run, CFRangeMake(0, 0), &advances)
            CTRunGetStringIndices(run, CFRangeMake(0, 0), &indices)
            for (advance, index) in zip(advances, indices) where index < until { place[index + 1] += advance.width }
        }
        for i in 1...until { place[i] += place[i - 1] }
        var ends: [Float] = [], nexts: [Float] = [], hyphens: [UInt8] = [], offsets: [Int] = []
        var rest: Double?
        for (boundary, content) in zip(chosen.boundaries, chosen.contentEnds) where boundary <= until {
            var end = place[min(content, until)], next = place[boundary]
            let hyphenated = boundary < until && content == boundary && text.character(at: boundary - 1) == 0xAD
            if hyphenated {
                // The hyphen a break here shows, part of its line (`inkedSoftHyphen`).
                let range = CFRangeMake(boundary - 1, 1), plain = CTTypesetterCreateLine(typesetter, range)
                end += CGFloat(CTLineGetTypographicBounds(TextEngine.inkedSoftHyphen(plain, source: source, range: range), nil, nil, nil)
                               - CTLineGetTypographicBounds(plain, nil, nil, nil))
            }
            if boundary < until {
                // The start room of a run that begins here. Kerned on a space it hangs with the space; after
                // any other character a line that ends here leaves it out (`exactBreakRoom`).
                let room = insets.edgeStarts[boundary] ?? 0
                end -= TextEngine.breakRoom(endingAt: boundary, in: source)
                next -= room
                // The rest of the inset is the same at every break: a list item's indent.
                rest = Double(insets.at(boundary).width - room)
            }
            ends.append(Float(end)); nexts.append(Float(next)); hyphens.append(hyphenated ? 1 : 0); offsets.append(boundary)
        }
        guard offsets.last == until, let rest else { return nil }
        var breaks = [UInt32](repeating: 0, count: TextEngine.balanceMaxLines)
        let count = exact_text_balanced_breaks(Float(limit - rest), Float(Double(insets.at(0).width) - rest), Float(fontSize), justified ? 1 : 0,
                                               ends, nexts, hyphens, offsets.count, lines, &breaks, breaks.count)
        guard count > 0 else { return nil }
        var counts: [Int] = [], start = 0
        for at in breaks.prefix(count) { counts.append(offsets[Int(at)] - start); start = offsets[Int(at)] }
        return counts + [until - start]
    }
}

extension TextEngine {
    /// The most lines `balance` balances; more break greedily, as in Chromium.
    package static let balanceMaxLines = 6
    /// `exact_textflow::balanced_width` over a paragraph's own breaker,
    /// Blink's bisection: the limit, within 1 pt of the narrowest, at which
    /// a paragraph of `count` greedy lines at `limit`, their widths summing to
    /// `total`, still `fits` (breaks into no more lines, none overflowing).
    /// The search runs in 1/64 pt (a browser layout unit) from 0.8 of the
    /// average line. nil for one line, more than `balanceMaxLines`, or no
    /// narrower limit that fits.
    package static func balancedLimit(_ limit: Double, count: Int, total: Double, fits: (Double) -> Bool) -> Double? {
        guard limit.isFinite, limit > 0, (2...balanceMaxLines).contains(count) else { return nil }
        let box = Int((limit * 64).rounded(.down))
        var lower = Int((Float(Int((max(0, total) * 64).rounded(.down)) / count) * 0.8).rounded()), upper = box
        while lower + 64 < upper {
            let middle = (upper + lower) / 2
            if fits(Double(middle) / 64) { upper = middle } else { lower = middle }
        }
        return upper < box ? Double(upper) / 64 : nil
    }
    /// Each opportunity's content end, and the ends of the forced breaks
    /// (CR LF one), in one forward pass. A boundary's own forced break is no
    /// content, and the spaces before it hang (CSS Text 3 §4.1.3): U+0020,
    /// tabs and the other space separators; no-break spaces do not.
    package static func breakEnds(_ text: NSString, boundaries: [Int], length: Int) -> ([Int], [Int]) {
        func hangs(_ ch: unichar) -> Bool { hangingSpace(ch) }
        // runStart[e]: where the spaces and tabs ending at e begin.
        var runStart = [Int](repeating: 0, count: length + 1)
        var hard: [Int] = []
        var i = 0
        while i < length {
            let ch = text.character(at: i)
            runStart[i + 1] = hangs(ch) ? runStart[i] : i + 1
            if ch == 13, i + 1 < length, text.character(at: i + 1) == 10 {
                runStart[i + 2] = i + 2; hard.append(i + 2); i += 2; continue
            }
            if forcedBreak(ch) { hard.append(i + 1) }
            i += 1
        }
        let content = boundaries.map { boundary -> Int in
            var end = min(boundary, length)
            if end > 0, forcedBreak(text.character(at: end - 1)) {
                end -= end >= 2 && text.character(at: end - 1) == 10 && text.character(at: end - 2) == 13 ? 2 : 1
            }
            return runStart[end]
        }
        return (content, hard)
    }
    /// A space that hangs at a line's end: U+0020, a tab, or another Unicode
    /// space separator (Zs) that is not a no-break space (U+00A0, U+2007, U+202F).
    package static func hangingSpace(_ ch: unichar) -> Bool {
        ch == 0x20 || ch == 0x09 || ch == 0x1680 || (0x2000...0x200A).contains(ch) && ch != 0x2007 || ch == 0x205F || ch == 0x3000
    }
    package static func forcedBreak(_ ch: unichar) -> Bool {
        ch == 10 || ch == 13 || ch == 0x0B || ch == 0x0C || ch == 0x85 || ch == 0x2028 || ch == 0x2029
    }

    /// CSS's choice of where a line ends, `fitted` the most clusters that fit
    /// from `start` (no further than the next forced break, `hard`): the last
    /// opportunity whose content fits; else the first opportunity, letting
    /// the word overflow (`normal`), or inside it at `fitted` (`break-word`).
    package static func cssBreak(start: Int, fitted: Int, hard: Int, boundaries: [Int], contentEnds: [Int],
                         from: Int, breakWord: Bool, text: NSString) -> Int {
        var i = from
        while i < boundaries.count && boundaries[i] <= start { i += 1 }
        let first = i
        while i < boundaries.count && boundaries[i] - start <= hard && contentEnds[i] <= start + fitted { i += 1 }
        if i > first { return boundaries[i - 1] - start }
        if !breakWord, first < boundaries.count { return min(boundaries[first] - start, hard) }
        if fitted > 0 { return fitted }
        return start < text.length ? text.rangeOfComposedCharacterSequence(at: start).length : 0
    }

    /// A break at a soft hyphen shows one, which must fit too, as in Chrome
    /// (CoreText counts the invisible SHY as nothing). When it does not, an
    /// earlier opportunity, which shows no hyphen, takes the original room;
    /// only the first opportunity falls back to `retry` with the hyphen's
    /// advance taken from the room (a break inside it, or its overflow).
    package static func fitSoftHyphen(start: Int, count: Int, room: Double, boundaries: [Int], source: NSAttributedString,
                              typesetter: CTTypesetter, length: Int, retry: (Double) -> Int) -> Int {
        var count = count
        let text = source.string as NSString
        while count > 0, start + count < length, room.isFinite, text.character(at: start + count - 1) == 0xAD {
            let plain = CTTypesetterCreateLine(typesetter, CFRangeMake(start, count))
            let ink = CTLineGetTypographicBounds(inkedSoftHyphen(plain, source: source, range: CFRangeMake(start, count)), nil, nil, nil)
            if ink <= room { break }
            var lo = 0, hi = boundaries.count
            while lo < hi { let mid = (lo + hi) / 2; if boundaries[mid] < start + count { lo = mid + 1 } else { hi = mid } }
            if lo > 0, boundaries[lo - 1] > start { count = boundaries[lo - 1] - start; continue }
            return retry(room - (ink - CTLineGetTypographicBounds(plain, nil, nil, nil)))
        }
        return count
    }

    /// Min-content's pieces of `value`: the text between UAX #14 line-break
    /// opportunities, their hanging spaces dropped (`normal` and `break-word`
    /// alike: CSS counts no break-word break toward min-content).
    package func unbreakablePieces(_ value: String) -> [String] {
        Self.pieces(value as NSString, boundaries: lineBoundaries(value as NSString, length: (value as NSString).length))
    }
    /// The same from a worker.
    package static func unbreakablePieces(_ value: String) -> [String] {
        let text = value as NSString
        return pieces(text, boundaries: lineBreaks(text, length: text.length))
    }
    /// A native face's min-content run, using its own control to measure
    /// each candidate so the title/subtitle fonts and chrome stay the OS's.
    package static func widestButtonRun(_ value: String?, whiteSpace: String?, measure: (String) -> CGFloat) -> String? {
        guard let value else { return nil }
        if whiteSpace == "nowrap" || whiteSpace == "pre" { return value }
        var widest = "", width: CGFloat = -1
        for run in unbreakablePieces(value) {
            let next = measure(run)
            if next > width { widest = run; width = next }
        }
        return widest
    }
    /// Where a line may end, as UTF16 offsets, the last being `length`: the
    /// shared walker's opportunities (`exact_text_line_breaks`), Chrome's, with
    /// CFStringTokenizer's dictionary words inside Thai, Lao, Khmer and Myanmar
    /// runs (TextFlowSource.complexWords). Any thread.
    package static func lineBreaks(_ text: NSString, length: Int) -> [Int] {
        guard length > 0 else { return [0] }
        let string = text as String
        let words = TextFlowSource.complexWords(string)
        var ends = [UInt32](repeating: 0, count: length + 1)
        let count = Array(string.utf8).withUnsafeBufferPointer { bytes in
            words.withUnsafeBufferPointer { words in
                ends.withUnsafeMutableBufferPointer { out in
                    exact_text_line_breaks(bytes.baseAddress, bytes.count, words.baseAddress, words.count, out.baseAddress, out.count)
                }
            }
        }
        var boundaries = ends.prefix(min(count, ends.count)).map { Int($0) }.filter { $0 > 0 && $0 <= length }
        if boundaries.last != length { boundaries.append(length) }
        return boundaries
    }
    package static func pieces(_ text: NSString, boundaries: [Int]) -> [String] {
        pieceRanges(text, boundaries: boundaries).map { piece in
            let s = text.substring(with: piece.range)
            return piece.hyphenated ? String(s.dropLast()) + "-" : s
        }
    }
    /// The pieces as UTF-16 ranges of `text`. A piece is `hyphenated` when it
    /// breaks right after a soft hyphen, which then shows as a hyphen
    /// (CSS Text 3 §5.3) and counts toward the piece's width.
    package static func pieceRanges(_ text: NSString, boundaries: [Int]) -> [(range: NSRange, hyphenated: Bool)] {
        guard text.length > 0 else { return [] }
        func trims(_ ch: unichar) -> Bool { hangingSpace(ch) || forcedBreak(ch) }
        var pieces: [(range: NSRange, hyphenated: Bool)] = []
        var start = 0
        for end in boundaries where end > start {
            var trimmed = end
            while trimmed > start, trims(text.character(at: trimmed - 1)) { trimmed -= 1 }
            var lead = start
            while lead < trimmed, trims(text.character(at: lead)) { lead += 1 }
            if trimmed > lead {
                pieces.append((NSRange(location: lead, length: trimmed - lead), trimmed == end && text.character(at: end - 1) == 0xAD))
            }
            start = end
        }
        return pieces
    }
    /// Min-content's pieces of a paragraph: `pieceRanges` over the runs'
    /// joined `text`, from `boundaries` taken over the whole of it, so a run's
    /// end inside a word is no break (CSS Text 3, "Line Breaking Details").
    /// A hung run (a list marker) is no piece and ends the pieces before it.
    /// Each piece carries the indent of the run it starts in.
    package static func paragraphPieces(_ text: NSString, runs: [Run], boundaries: [Int]) -> [(range: NSRange, hyphenated: Bool, indent: CGFloat)] {
        var pieces: [(range: NSRange, hyphenated: Bool, indent: CGFloat)] = []
        var starts: [(at: Int, indent: CGFloat)] = []
        var spanStart = 0, offset = 0, next = 0, run = 0
        func flush(_ end: Int) {
            guard end > spanStart else { return }
            while next < boundaries.count, boundaries[next] <= spanStart { next += 1 }
            var inside: [Int] = []
            while next < boundaries.count, boundaries[next] < end { inside.append(boundaries[next] - spanStart); next += 1 }
            inside.append(end - spanStart)
            let span = text.substring(with: NSRange(location: spanStart, length: end - spanStart)) as NSString
            for piece in pieceRanges(span, boundaries: inside) {
                let at = spanStart + piece.range.location
                while run + 1 < starts.count, starts[run + 1].at <= at { run += 1 }
                pieces.append((NSRange(location: at, length: piece.range.length), piece.hyphenated, starts[run].indent))
            }
        }
        for r in runs {
            let end = offset + (r.text as NSString).length
            if r.hang {
                flush(offset)
                spanStart = end
            } else {
                starts.append((offset, r.indent))
            }
            offset = end
        }
        flush(offset)
        return pieces
    }
}
