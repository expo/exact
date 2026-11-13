// A `text` inside a paragraph is an inline box whose padding, border and
// margin take room in the line beside its text (the kernel's
// `TextRun::edges`, LLP 1001; expo/exact#389), as a `<span>`'s do.
import Foundation
import CoreText

extension NSAttributedString.Key {
    /// The inline boxes' room kerned after this character (`Run.inlineEnd`
    /// of its run, `inlineStart` of the next): no glyph's ink, so no
    /// background but its own boxes' reaches into it.
    package static let exactEdgeRoom = NSAttributedString.Key("ExactInlineEdgeRoom")
    /// The next run's start room kerned after this character, which is no
    /// white space: where a line breaks after it, that room goes to the next
    /// line with the run's text, as Chrome keeps a `<span>`'s padding with
    /// its text, and this line does not count it.
    package static let exactBreakRoom = NSAttributedString.Key("ExactInlineBreakRoom")
}

extension TextEngine {
    /// The room inline boxes take beside their text (`Run.inlineStart`,
    /// `inlineEnd`), as Core Text's kern after a glyph: a run's end after its
    /// last character, its start after the character before it. The first
    /// run's start has none before it and is the first line's indent
    /// (`Spec.firstLineIndent`); a line that starts with a later run starts
    /// with its start (`LineInsets.edgeStarts`), which the line before does
    /// not count (a space hangs; any other character's `exactBreakRoom`).
    package static func kernInlineEdges(_ s: NSMutableAttributedString, runs: [Run]) {
        guard runs.contains(where: { $0.inlineStart != 0 || $0.inlineEnd != 0 }) else { return }
        kernInlineEdges(s, runs: runs.map { (text: $0.text, start: $0.inlineStart, end: $0.inlineEnd) })
    }

    /// Each run's text and its room before and after it, as `Run`'s.
    package static func kernInlineEdges(_ s: NSMutableAttributedString, runs: [(text: String, start: CGFloat, end: CGFloat)]) {
        guard runs.contains(where: { $0.start != 0 || $0.end != 0 }) else { return }
        let text = s.string as NSString
        func add(_ room: CGFloat, before end: Int, breaks: Bool) {
            guard room != 0, end > 0 else { return }
            let range = text.rangeOfComposedCharacterSequence(at: end - 1)
            func more(_ key: NSAttributedString.Key) {
                let had = (s.attribute(key, at: range.location, effectiveRange: nil) as? CGFloat) ?? 0
                s.addAttribute(key, value: had + room, range: range)
            }
            more(.kern); more(.exactEdgeRoom)
            if breaks, let scalar = Unicode.Scalar(text.character(at: end - 1)), !scalar.properties.isWhitespace {
                more(.exactBreakRoom)
            }
        }
        var offset = 0
        for r in runs {
            let length = r.text.utf16.count
            add(r.start, before: offset, breaks: true)
            add(r.end, before: offset + length, breaks: false)
            offset += length
        }
    }

    /// The start room a line ending at `end` leaves to the next line
    /// (`exactBreakRoom`), which its width does not count.
    package static func breakRoom(endingAt end: Int, in source: NSAttributedString) -> CGFloat {
        guard end > 0, end < source.length else { return 0 }
        return source.attribute(.exactBreakRoom, at: end - 1, effectiveRange: nil) as? CGFloat ?? 0
    }

    /// The line from `start` taken on to the first opportunity past `count`
    /// that ends right before a run with a start room after a character that
    /// is no white space (between two CJK characters), where the line fits
    /// without that room, which goes to the next line as Chrome puts it.
    /// Core Text's own break counted the room on this line.
    package static func breakBeforeRoom(_ typesetter: CTTypesetter, source: NSAttributedString, start: Int, count: Int,
                                        room: Double, boundaries: [Int], hard: Int) -> Int {
        guard source.length > 0, let b = boundaries.first(where: { $0 > start + count }), b <= hard else { return count }
        let leaves = breakRoom(endingAt: b, in: source)
        guard leaves != 0 else { return count }
        let line = CTTypesetterCreateLine(typesetter, CFRange(location: start, length: b - start))
        return CTLineGetTypographicBounds(line, nil, nil, nil) - Double(leaves) <= room ? b - start : count
    }
}

extension Spec {
    /// Where the first line starts: `text-indent`, and the start edge of
    /// the inline boxes the first run is in, which nothing precedes to kern
    /// (`TextEngine.kernInlineEdges`).
    package var firstLineIndent: CGFloat { textIndent + (runs.first?.inlineStart ?? 0) }
}

extension LineInsets {
    /// The UTF-16 starts of the runs with an inline box's start room: a
    /// line starting there starts with it, as Chrome keeps a `<span>`'s
    /// padding with its text, while the line before leaves its kern out
    /// (a space hangs; `exactBreakRoom`). Elsewhere the kern is the room.
    package static func edgeStarts(_ runs: [Run]) -> [Int: CGFloat] {
        guard runs.dropFirst().contains(where: { $0.inlineStart != 0 }) else { return [:] }
        return edgeStarts(runs.map { (text: $0.text, start: $0.inlineStart) })
    }

    /// Each run's text and its room before it, as `Run`'s.
    package static func edgeStarts(_ runs: [(text: String, start: CGFloat)]) -> [Int: CGFloat] {
        guard runs.dropFirst().contains(where: { $0.start != 0 }) else { return [:] }
        var out: [Int: CGFloat] = [:], offset = 0
        for r in runs {
            if r.start != 0, offset > 0 { out[offset, default: 0] += r.start }
            offset += r.text.utf16.count
        }
        return out
    }
}
