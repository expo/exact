// Where a broken line draws an inline box's room (`Run.inlineStart`,
// `inlineEnd`): on the box's start or end side of its text. A box's sides
// are its own direction's, its paragraph's, whatever the direction of its
// text, and its first and last fragments are in visual order (CSS 2 §8.6,
// §9.10; CSS Writing Modes 3 §2.1). The room is kerned in logical order to
// break lines (`TextEngine.kernInlineEdges`), as Chrome counts it, and each
// line moves its kerns to where it draws them (`TextEngine.placingRooms`).
#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif
import CoreText

extension NSAttributedString.Key {
    /// The run each character with inline-box room around it is in
    /// (`InlineRoomRun`).
    package static let exactRoomRun = NSAttributedString.Key("ExactInlineRoomRun")
}

/// A run with room beside its text: its index among the paragraph's runs,
/// its UTF-16 range, its start and end room, and whether its boxes are right
/// to left. `left` is the part of its room the line made from this source
/// draws left of its glyphs, where no glyph can carry a kern
/// (`TextEngine.leftRoom`).
package final class InlineRoomRun: NSObject, @unchecked Sendable {
    package let index: Int
    package let range: NSRange
    package let start: CGFloat
    package let end: CGFloat
    package let rtl: Bool
    package let left: CGFloat
    package init(index: Int, range: NSRange, start: CGFloat, end: CGFloat, rtl: Bool, left: CGFloat = 0) {
        self.index = index; self.range = range; self.start = start; self.end = end; self.rtl = rtl; self.left = left
    }
    func with(left: CGFloat) -> InlineRoomRun {
        InlineRoomRun(index: index, range: range, start: start, end: end, rtl: rtl, left: left)
    }
    override package func isEqual(_ object: Any?) -> Bool {
        guard let other = object as? InlineRoomRun else { return false }
        return index == other.index && range == other.range && start == other.start && end == other.end
            && rtl == other.rtl && left == other.left
    }
    override package var hash: Int { index }
}

/// One room as a line draws it.
package struct PlacedRoom {
    package let run: InlineRoomRun
    /// Its start room, else its end room.
    package let start: Bool
    package let width: CGFloat
    /// Its left edge, in the line's coordinates.
    package let x: CGFloat
    /// Whether its run's text is on its left.
    package let textLeft: Bool
    /// What draws it: the kern of this UTF-16 index, else the line's
    /// inset (a start that begins its line) or nothing left of its glyphs.
    let kernAt: Int?
    let inset: Bool
}

extension TextEngine {
    /// Each room on `line`: a start or an end is beside the visually
    /// contiguous characters of its run that hold its first or last
    /// character, on its start or end side, so under `rtl` the start on
    /// their right and the end on their left. It is drawn by the kern of the
    /// glyph on its left: its run's own rightmost, or the glyph before its
    /// leftmost, or, with none there but hanging white space, by nothing
    /// (`leftRoom`). A start that begins its line is in its inset.
    package static func placedRooms(_ line: CTLine) -> [PlacedRoom] {
        struct Glyph { let index: Int; let lo: CGFloat; let hi: CGFloat; let run: InlineRoomRun?; var hangs = false }
        var glyphs: [Glyph] = [], rtlBase = false, runs: [Int: InlineRoomRun] = [:]
        for run in CTLineGetGlyphRuns(line) as! [CTRun] {
            let count = CTRunGetGlyphCount(run)
            guard count > 0 else { continue }
            let attributes = CTRunGetAttributes(run) as NSDictionary
            if let style = attributes[NSAttributedString.Key.paragraphStyle] as? NSParagraphStyle {
                rtlBase = style.baseWritingDirection == .rightToLeft
            }
            let room = attributes[NSAttributedString.Key.exactRoomRun] as? InlineRoomRun
            if let room { runs[room.index] = room }
            // A kerned character is a run of its own; its room is right of the glyph that reaches furthest.
            let kerned = attributes[NSAttributedString.Key.exactEdgeRoom] as? CGFloat ?? 0
            var indices = [CFIndex](repeating: 0, count: count)
            var positions = [CGPoint](repeating: .zero, count: count)
            var advances = [CGSize](repeating: .zero, count: count)
            CTRunGetStringIndices(run, CFRange(), &indices)
            CTRunGetPositions(run, CFRange(), &positions)
            CTRunGetAdvances(run, CFRange(), &advances)
            let reach = (0..<count).max { positions[$0].x + advances[$0].width < positions[$1].x + advances[$1].width }
            for i in 0..<count {
                let a = positions[i].x, b = a + advances[i].width
                glyphs.append(Glyph(index: indices[i], lo: min(a, b), hi: max(a, b) - (i == reach ? kerned : 0), run: room))
            }
        }
        guard !runs.isEmpty else { return [] }
        glyphs.sort { ($0.lo, $0.hi) < ($1.lo, $1.hi) }
        // A line's trailing white space hangs past its end edge: left of its
        // pen under an `rtl` base (CoreText's), past its width under `ltr`.
        let trailing = CGFloat(CTLineGetTrailingWhitespaceWidth(line))
        if trailing > 0 {
            let width = CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil)) - trailing
            for i in glyphs.indices where glyphs[i].hi > glyphs[i].lo {
                glyphs[i].hangs = rtlBase ? glyphs[i].lo < -0.001 : glyphs[i].lo > width - 0.001
            }
        }
        let range = CTLineGetStringRange(line), lineEnd = range.location + range.length
        var out: [PlacedRoom] = []
        func place(_ run: InlineRoomRun, start: Bool) {
            let anchor = start ? run.range.location : NSMaxRange(run.range) - 1
            let width = start ? run.start : run.end
            let mine = { (g: Glyph) in g.run?.index == run.index && !g.hangs }
            guard let k = glyphs.indices.filter({ mine(glyphs[$0]) && glyphs[$0].index <= anchor })
                .max(by: { glyphs[$0].index < glyphs[$1].index }) else { return }
            var lo = k, hi = k
            while lo > 0, mine(glyphs[lo - 1]) { lo -= 1 }
            while hi + 1 < glyphs.count, mine(glyphs[hi + 1]) { hi += 1 }
            let group = glyphs[lo...hi]
            let inset = start && anchor == range.location
            if start == run.rtl {
                let right = group.max { $0.hi < $1.hi }!
                out.append(PlacedRoom(run: run, start: start, width: width, x: right.hi, textLeft: true,
                                      kernAt: inset ? nil : right.index, inset: inset))
            } else {
                let left = group.map(\.lo).min()!
                let before = lo > 0 && !glyphs[lo - 1].hangs ? glyphs[lo - 1].index : nil
                out.append(PlacedRoom(run: run, start: start, width: width, x: left - width, textLeft: false,
                                      kernAt: inset ? nil : before, inset: inset))
            }
        }
        for run in runs.values.sorted(by: { $0.index < $1.index }) {
            if run.start != 0, run.range.location >= range.location, run.range.location < lineEnd { place(run, start: true) }
            if run.end != 0, NSMaxRange(run.range) > range.location, NSMaxRange(run.range) <= lineEnd { place(run, start: false) }
        }
        return out
    }

    /// The room a line draws left of its glyphs, which its width counts
    /// (`placedRooms`).
    package static func leftRoom(_ line: CTLine) -> CGFloat {
        var runs: [Int: CGFloat] = [:]
        for run in CTLineGetGlyphRuns(line) as! [CTRun] {
            if let room = (CTRunGetAttributes(run) as NSDictionary)[NSAttributedString.Key.exactRoomRun] as? InlineRoomRun, room.left != 0 {
                runs[room.index] = room.left
            }
        }
        return runs.values.reduce(0, +)
    }

    /// `line`, broken from `source` at `range`, with its rooms kerned where
    /// it draws them (`placedRooms`) rather than in logical order. The
    /// source's prefix keeps every UTF-16 index where it was; a soft hyphen
    /// that ends the line is ink, as `inkedSoftHyphen` made it.
    package static func placingRooms(_ line: CTLine, source: NSAttributedString, range: CFRange) -> CTLine {
        let end = range.location + range.length
        guard range.length > 0, end <= source.length else { return line }
        var any = false
        source.enumerateAttribute(.exactRoomRun, in: NSRange(location: range.location, length: range.length)) { value, _, stop in
            if value != nil { any = true; stop.pointee = true }
        }
        guard any else { return line }
        // Each room the line draws elsewhere than its logical kern: the
        // character it was kerned on, and the one that draws it (nil: left
        // of the line's glyphs).
        var moves: [(from: Int, to: Int?, room: PlacedRoom)] = []
        for placed in placedRooms(line) where !placed.inset {
            let from = placed.start ? placed.run.range.location - 1 : NSMaxRange(placed.run.range) - 1
            if placed.kernAt != from { moves.append((from, placed.kernAt, placed)) }
        }
        guard !moves.isEmpty else { return line }
        let prefix = NSMutableAttributedString(attributedString: source.attributedSubstring(from: NSRange(location: 0, length: end)))
        let text = prefix.string as NSString
        func add(_ amount: CGFloat, at index: Int) {
            let chars = text.rangeOfComposedCharacterSequence(at: index)
            for key in [NSAttributedString.Key.kern, .exactEdgeRoom] {
                let had = (prefix.attribute(key, at: chars.location, effectiveRange: nil) as? CGFloat) ?? 0
                // No kern is the font's own kerning, which a kern of 0 turns off.
                if had + amount == 0 { prefix.removeAttribute(key, range: chars) } else { prefix.addAttribute(key, value: had + amount, range: chars) }
            }
        }
        var left: [Int: (InlineRoomRun, CGFloat)] = [:]
        for move in moves {
            add(-move.room.width, at: move.from)
            if let to = move.to { add(move.room.width, at: to) } else {
                left[move.room.run.index, default: (move.room.run, 0)].1 += move.room.width
            }
        }
        for (run, width) in left.values {
            prefix.addAttribute(.exactRoomRun, value: run.with(left: width), range: NSIntersectionRange(run.range, NSRange(location: 0, length: end)))
        }
        if text.character(at: end - 1) == 0xAD { prefix.replaceCharacters(in: NSRange(location: end - 1, length: 1), with: "-") }
        return CTTypesetterCreateLine(CTTypesetterCreateWithAttributedString(prefix), range)
    }
}
