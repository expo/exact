// A `text` inside a paragraph is an inline box whose inline-axis padding,
// border and margin take room in the line (the kernel's `TextRun::edges`):
// on Core Text a kern after the glyph before it, as a `<span>`'s padding
// takes room in Chrome (#389: a verse number touched the next word).
import XCTest
import CoreText
import CExact
@testable import ExactKit

final class InlineEdgesTests: XCTestCase {
    private let engine = TextEngine(resolve: { _ in nil })
    private func run(_ text: String, start: CGFloat = 0, end: CGFloat = 0) -> Run {
        var r = Run(text: text, size: 16, weight: 400, family: 0, italic: false, lineHeight: nil, letterSpacing: 0)
        r.inlineStart = start; r.inlineEnd = end
        return r
    }
    private func spec(_ runs: [Run]) -> Spec {
        Spec(runs: runs, align: 0, lineClamp: 0, color: [0, 0, 0, 255], strut: run(""))
    }
    /// Where the glyph of UTF-16 `index` is drawn on the first line.
    private func x(_ p: Paragraph, _ index: Int) -> CGFloat {
        for run in CTLineGetGlyphRuns(p.lines[0]) as! [CTRun] {
            let count = CTRunGetGlyphCount(run)
            var indices = [CFIndex](repeating: 0, count: count), positions = [CGPoint](repeating: .zero, count: count)
            CTRunGetStringIndices(run, CFRange(location: 0, length: count), &indices)
            CTRunGetPositions(run, CFRange(location: 0, length: count), &positions)
            if let i = indices.firstIndex(of: index) { return positions[i].x }
        }
        return .nan
    }

    func testARunsEdgesTakeRoomBesideItsText() throws {
        let plain = engine.paragraph(spec([run("1"), run("The")]), width: .infinity)
        // A verse number's `padding-right`: the gap after "1", before "The".
        let padded = engine.paragraph(spec([run("1", end: 4.8), run("The")]), width: .infinity)
        XCTAssertEqual(padded.width, plain.width + 4.8, accuracy: 0.05)
        XCTAssertEqual(x(padded, 1), x(plain, 1) + 4.8, accuracy: 0.05)
        // A later run's start goes after the glyph before it.
        let started = engine.paragraph(spec([run("1"), run("The", start: 3)]), width: .infinity)
        XCTAssertEqual(x(started, 1), x(plain, 1) + 3, accuracy: 0.05)
        // The first run's start, with nothing before it, is the first line's.
        let first = engine.paragraph(spec([run("1", start: 6), run("The")]), width: .infinity)
        XCTAssertEqual(first.width, plain.width + 6, accuracy: 0.05)
        XCTAssertEqual(spec([run("1", start: 6)]).firstLineIndent, 6)
        // A fitted paragraph's room comes from the kernel at its scale, as
        // its measure request's does: the run's scaling leaves it.
        XCTAssertEqual(run("1", start: 2, end: 4).scaled(0.5).inlineEnd, 4)
    }

    /// Where a line breaks right before a run with a start, the room starts
    /// the next line with its text, as Chrome keeps a `<span>`'s padding with
    /// it; the space before it, kerned, hangs at the end of the line before.
    func testAStartAtABreakStartsTheNextLine() {
        let plain = engine.paragraph(spec([run("aaaa "), run("bbbb")]), width: .infinity)
        let padded = spec([run("aaaa "), run("bbbb", start: 20)])
        let one = engine.paragraph(padded, width: .infinity)
        XCTAssertEqual(one.lines.count, 1)
        XCTAssertEqual(x(one, 5), x(plain, 5) + 20, accuracy: 0.05, "on one line, after the space")
        XCTAssertEqual(one.insets.at(5).width, 20, "a line starting there would start with it")
        let two = engine.paragraph(padded, width: plain.width + 10)
        XCTAssertEqual(two.lines.count, 2)
        XCTAssertEqual(CTLineGetStringRange(two.lines[1]).location, 5)
        XCTAssertEqual(two.insets.at(5).left, 20, "the second line starts at the room")
        // Its width is its wider line's: the second, with the room.
        let word = engine.paragraph(spec([run("bbbb")]), width: .infinity).width
        let first = engine.paragraph(spec([run("aaaa")]), width: .infinity).width
        XCTAssertEqual(two.width, max(first, word + 20), accuracy: 0.5)
        // Min-content: the word carries its room; the space hangs.
        XCTAssertEqual(engine.minContentWidth(padded), word + 20, accuracy: 0.5)
    }

    /// Between two CJK characters a line breaks with no space before the run,
    /// and the room still goes to the next line with its text, as in Chrome
    /// 154: `中文<span style="padding-left:20px">中文</span>` 36 px wide keeps
    /// "中文" on the first line and puts the span's "中" 20 px into the second.
    func testAStartAfterACharacterThatIsNoSpaceMovesWithItsText() {
        let pair = engine.paragraph(spec([run("中文")]), width: .infinity).width
        let padded = spec([run("中文"), run("中文", start: 20)])
        let p = engine.paragraph(padded, width: pair + 4)
        XCTAssertGreaterThanOrEqual(p.lines.count, 2)
        XCTAssertEqual(CTLineGetStringRange(p.lines[0]).length, 2, "the first line keeps both of its characters")
        XCTAssertEqual(p.insets.at(2).left, 20, "the second line starts at the room")
        // The first line's width leaves the room it kerned to the next.
        XCTAssertEqual(p.width, pair / 2 + 20, accuracy: 0.5)
        // Min-content: the run's first character carries its room.
        XCTAssertEqual(engine.minContentWidth(padded), pair / 2 + 20, accuracy: 0.5)
    }

    /// A box's background covers its padding (CSS 2 §14.2): from its first
    /// glyph back by its start, to its last glyph's end past the room kerned
    /// after it less its margin.
    func testABackgroundCoversItsPadding() throws {
        let gray: [Double] = [242, 242, 247, 255]
        func boxed(_ text: String, start: CGFloat, end: CGFloat, pad: (CGFloat, CGFloat)) -> Run {
            var r = run(text, start: start, end: end)
            r.background = gray; r.backgroundBox = 9; r.backgroundBefore = pad.0; r.backgroundAfter = pad.1
            return r
        }
        func fill(_ runs: [Run]) throws -> CGRect {
            let p = engine.paragraph(spec(runs), width: .infinity)
            return try XCTUnwrap(TextLinePaint.backgrounds(p.lines[0], at: .zero).first?.rect)
        }
        let bare = try fill([run("a "), boxed("code", start: 0, end: 0, pad: (0, 0)), run(" b")])
        // 4 of padding each side, and 3 of margin after it, which the background leaves out.
        let padded = try fill([run("a "), boxed("code", start: 4, end: 7, pad: (4, 4)), run(" b")])
        XCTAssertEqual(padded.minX, bare.minX, accuracy: 0.05, "the room starts where the bare box did")
        XCTAssertEqual(padded.width, bare.width + 8, accuracy: 0.05)
        // A square box with a margin alone paints its text and not the margin.
        let margined = try fill([run("a "), boxed("code", start: 0, end: 10, pad: (0, 0)), run(" b")])
        XCTAssertEqual(margined.minX, bare.minX, accuracy: 0.05)
        XCTAssertEqual(margined.width, bare.width, accuracy: 0.05)
    }

    /// Above and below its text a box's background covers its padding and
    /// border too, which move no line (CSS 2 §10.6.1): Chrome paints
    /// `<span style="padding:3px 0 12px;background:#ddd">code</span>` 15 px
    /// taller than its font's content area, from 3 px above it. A box around
    /// it paints its own, not the inner box's.
    func testABackgroundCoversItsBlockPadding() throws {
        let gray: [Double] = [242, 242, 247, 255], red: [Double] = [255, 0, 0, 255]
        var bare = run("code"), padded = bare
        bare.background = gray; bare.backgroundBox = 9
        padded.background = gray; padded.backgroundBox = 9; (padded.backgroundAbove, padded.backgroundBelow) = (3, 12)
        func fills(_ runs: [Run]) -> (Paragraph, [TextLinePaint.Fill]) {
            let p = engine.paragraph(spec(runs), width: .infinity)
            return (p, TextLinePaint.backgrounds(p.lines[0], at: .zero))
        }
        let (plain, b) = fills([run("a "), bare, run(" b")]), (tall, t) = fills([run("a "), padded, run(" b")])
        let short = try XCTUnwrap(b.first?.rect), rect = try XCTUnwrap(t.first?.rect)
        XCTAssertEqual(rect.minY, short.minY - 3, accuracy: 0.05)
        XCTAssertEqual(rect.height, short.height + 15, accuracy: 0.05)
        XCTAssertEqual(rect.width, short.width, accuracy: 0.05, "no wider")
        XCTAssertEqual(tall.height, plain.height, "the line is as tall")
        var inner = padded
        inner.outerBackgrounds = [OuterBackground(color: red, box: 8, radii: [], before: 0, after: 0, above: 1, below: 2)]
        let nested = fills([run("a "), inner, run(" b")]).1
        XCTAssertEqual(nested.count, 2, "\(nested)")
        guard nested.count == 2 else { return }
        XCTAssertEqual(nested[0].rect.minY, short.minY - 1, accuracy: 0.05, "the outer box's own")
        XCTAssertEqual(nested[0].rect.height, short.height + 3, accuracy: 0.05)
        XCTAssertEqual(nested[1].rect, rect)
    }

    /// A box's border paints on each of its fragments, on its border box,
    /// over its background (CSS 2 §8.6, Appendix E): its top and bottom on
    /// every line, its start side on its first fragment and its end side on
    /// its last (`box-decoration-break: slice`), left and right the other way
    /// about under `rtl`. A box with a border and no background paints none.
    func testABorderIsSlicedAcrossItsFragments() throws {
        let red: [Double] = [255, 0, 0, 255]
        for direction in [0, 1] {
            var boxed = run("alpha beta gamma delta")
            boxed.backgroundBox = 9
            boxed.border = InlineBorder(widths: [1, 2, 3, 4], colors: Array(repeating: red, count: 4),
                                        styles: Array(repeating: "dashed", count: 4))
            var s = spec([run("x "), boxed, run(" y")])
            s.direction = direction
            let p = engine.paragraph(s, width: 120)
            XCTAssertGreaterThan(p.lines.count, 1, "the box wraps")
            // One fragment a line: a space that hangs past the line has none.
            let lines = p.lines.map { TextLinePaint.backgrounds($0, at: .zero) }
            XCTAssertEqual(lines.map(\.count), Array(repeating: 1, count: p.lines.count), "a fragment a line: \(lines)")
            let fills = lines.compactMap(\.first)
            for (i, fill) in fills.enumerated() {
                let border = try XCTUnwrap(fill.border)
                let (first, last) = (i == 0, i == fills.count - 1)
                let (left, right) = direction == 0 ? (first, last) : (last, first)
                XCTAssertEqual(border.widths, [1, right ? 2 : 0, 3, left ? 4 : 0], "line \(i), direction \(direction)")
                XCTAssertEqual(border.styles, Array(repeating: "dashed", count: 4))
                XCTAssertEqual(fill.color.alpha, 0, "no background")
            }
        }
    }

    /// A run with no text and padding is a box in its line, as Chrome lays
    /// `Hi<span style="padding:0 5px;border:2px solid;background:#ddd"></span> there`
    /// out: its padding and border take room after "Hi", where its background
    /// and its border paint, 14 wide, starting and ending there.
    func testAnEmptyRunPaintsItsBox() throws {
        var empty = run("", start: 7, end: 7)
        empty.background = [242, 242, 247, 255]; empty.backgroundBox = 9
        (empty.backgroundBefore, empty.backgroundAfter) = (7, 7)
        empty.border = InlineBorder(widths: [2, 2, 2, 2], colors: Array(repeating: [255, 0, 0, 255], count: 4),
                                    styles: Array(repeating: "solid", count: 4))
        let plain = engine.paragraph(spec([run("Hi"), run(" there")]), width: .infinity)
        let boxed = engine.paragraph(spec([run("Hi"), empty, run(" there")]), width: .infinity)
        XCTAssertEqual(boxed.width, plain.width + 14, accuracy: 0.05)
        let fills = TextLinePaint.backgrounds(boxed.lines[0], at: .zero)
        XCTAssertEqual(fills.count, 1, "\(fills)")
        let fill = try XCTUnwrap(fills.first)
        XCTAssertEqual(fill.rect.minX, x(plain, 2), accuracy: 0.05)
        XCTAssertEqual(fill.rect.width, 14, accuracy: 0.05)
        XCTAssertEqual(fill.border?.widths, [2, 2, 2, 2], "both its ends")
    }

    /// Boxes inside one another each paint their own background into their
    /// own padding, the inner over the outer (CSS 2 Appendix E), as Chrome
    /// paints `<span style="padding:0 2px;background:A;border-radius:4px">p
    /// <span style="padding:0 4px;background:B">x</span></span>`: the outer
    /// from 2 before "p" to 2 past the inner's padding, the inner 4 each side.
    func testNestedBoxesEachPaintTheirOwnPadding() throws {
        let a: [Double] = [255, 0, 0, 255], b: [Double] = [0, 0, 255, 255]
        var p = run("p", start: 2), x = run("x", start: 4, end: 6)
        p.background = a; p.backgroundBox = 8; p.backgroundRadii = Array(repeating: InlineRadius(px: 4), count: 4)
        (p.backgroundBefore, p.backgroundAfter) = (2, 6)
        x.background = b; x.backgroundBox = 9; (x.backgroundBefore, x.backgroundAfter) = (4, 4)
        x.outerBackgrounds = [OuterBackground(color: a, box: 8, radii: p.backgroundRadii, before: 2, after: 6)]
        let bare = engine.paragraph(spec([run("a "), run("p"), run("x"), run(" b")]), width: .infinity)
        let padded = engine.paragraph(spec([run("a "), p, x, run(" b")]), width: .infinity)
        let fills = TextLinePaint.backgrounds(padded.lines[0], at: .zero)
        XCTAssertEqual(fills.count, 2, "\(fills)")
        guard fills.count == 2 else { return }
        let (outer, inner) = (fills[0], fills[1])
        XCTAssertEqual(outer.color.components?.first ?? 0, 1, "the outer box paints first")
        XCTAssertEqual(outer.rect.minX, self.x(bare, 2), accuracy: 0.05)
        XCTAssertEqual(outer.rect.maxX, self.x(bare, 4) + 12, accuracy: 0.05)
        XCTAssertNotNil(outer.path, "its own corners rounded")
        XCTAssertEqual(inner.rect.minX, self.x(bare, 3) + 2, accuracy: 0.05)
        XCTAssertEqual(inner.rect.maxX, self.x(bare, 4) + 10, accuracy: 0.05)
        XCTAssertNil(inner.path)
    }

    /// An inline box's start and end follow its own direction, its
    /// paragraph's, whatever the direction of its text, and its first and
    /// last fragments are in visual order (CSS 2 §8.6, §9.10; CSS Writing
    /// Modes 3 §2.1): in a right-to-left paragraph its start room is right of
    /// its text and its end room left of it, as Chrome draws `<p dir=rtl>אב
    /// <span style="padding:0 3px 0 7px">גדה</span> וז</p>` and the same span
    /// holding "abc"; in a left-to-right one a Hebrew span's start is left of
    /// its last letter, its leftmost. Each case: its texts, its direction,
    /// and the UTF-16 indices of the run's leftmost glyph and of the glyph
    /// right of the run, with the room each has left of it.
    func testABoxsRoomsAreOnItsOwnSidesWhateverItsTextsDirection() {
        for (texts, direction, leftmost, after, rooms) in [
            (["אב ", "גדה", " וז"], 1, 5, 2, (7, 10)),
            (["אב ", "abc", " גד"], 1, 3, 2, (7, 10)),
            (["ab ", "אבג", " cd"], 0, 5, 6, (3, 10)),
        ] as [([String], Int, Int, Int, (CGFloat, CGFloat))] {
            var bare = spec(texts.map { run($0) }), padded = spec([run(texts[0]), run(texts[1], start: 3, end: 7), run(texts[2])])
            bare.direction = direction; padded.direction = direction
            let plain = engine.paragraph(bare, width: .infinity), boxed = engine.paragraph(padded, width: .infinity)
            XCTAssertEqual(boxed.width, plain.width + 10, accuracy: 0.05, "\(texts)")
            XCTAssertEqual(x(boxed, leftmost), x(plain, leftmost) + rooms.0, accuracy: 0.05, "\(texts): room left of the run")
            XCTAssertEqual(x(boxed, after), x(plain, after) + rooms.1, accuracy: 0.05, "\(texts): room right of it")
        }
    }

    /// A right-to-left run that ends its line has its end room left of its
    /// glyphs, where no glyph is to carry a kern: the line counts it, and a
    /// left-aligned line starts that much in.
    func testAnEndRoomLeftOfALinesGlyphsIsCounted() {
        var bare = spec([run("אב "), run("גד")]), padded = spec([run("אב "), run("גד", start: 3, end: 7)])
        bare.direction = 1; padded.direction = 1
        let plain = engine.paragraph(bare, width: .infinity), boxed = engine.paragraph(padded, width: .infinity)
        XCTAssertEqual(boxed.width, plain.width + 10, accuracy: 0.05)
        XCTAssertEqual(TextEngine.leftRoom(boxed.lines[0]), 7)
        XCTAssertEqual(x(boxed, 4), x(plain, 4), accuracy: 0.05, "its leftmost glyph has no room kerned left of it")
        XCTAssertEqual(boxed.origin(0, align: 0, width: 300), 7, accuracy: 0.05)
        XCTAssertEqual(boxed.origin(0, align: 2, width: 300), plain.origin(0, align: 2, width: 300) - 3, accuracy: 0.05)
    }

    /// Each fragment of a wrapped box holds its glyphs, its padding past them
    /// on its own start and end sides: in a right-to-left paragraph a Hebrew,
    /// Arabic or Latin box's start border is on its first line's right and
    /// its end border on its last line's left, its last letter inside it.
    func testAWrappedBoxsFragmentsHoldItsGlyphsInARightToLeftParagraph() throws {
        let red: [Double] = [255, 0, 0, 255]
        for texts in [["מילה ", "טקסט ארוך שעובר בין שורות", " סוף"], ["كلمة ", "نص طويل يمتد عبر السطور", " نهاية"],
                      ["Wrap ", "a run that wraps across lines", " end"]] {
            var boxed = run(texts[1], start: 9, end: 9)
            boxed.backgroundBox = 9; (boxed.backgroundBefore, boxed.backgroundAfter) = (6, 6)
            boxed.border = InlineBorder(widths: [3, 3, 3, 3], colors: Array(repeating: red, count: 4), styles: Array(repeating: "solid", count: 4))
            var s = spec([run(texts[0]), boxed, run(texts[2])])
            s.direction = 1
            let p = engine.paragraph(s, width: 180)
            let start = texts[0].utf16.count, end = start + texts[1].utf16.count
            let mine = p.lines.indices.filter {
                let r = CTLineGetStringRange(p.lines[$0])
                return r.location < end && r.location + r.length > start
            }
            XCTAssertGreaterThan(mine.count, 1, "\(texts) wraps")
            for i in mine {
                let line = p.lines[i]
                let fills = TextLinePaint.backgrounds(line, at: .zero)
                XCTAssertEqual(fills.count, 1, "\(texts) line \(i): \(fills)")
                let fill = try XCTUnwrap(fills.first)
                // The run's glyphs on this line, less the room kerned on them;
                // a hanging space is left of the pen.
                var lo = CGFloat.infinity, hi = -CGFloat.infinity
                for glyphRun in CTLineGetGlyphRuns(line) as! [CTRun] {
                    let count = CTRunGetGlyphCount(glyphRun)
                    var indices = [CFIndex](repeating: 0, count: count), positions = [CGPoint](repeating: .zero, count: count)
                    var advances = [CGSize](repeating: .zero, count: count)
                    CTRunGetStringIndices(glyphRun, CFRange(), &indices); CTRunGetPositions(glyphRun, CFRange(), &positions)
                    CTRunGetAdvances(glyphRun, CFRange(), &advances)
                    let room = (CTRunGetAttributes(glyphRun) as NSDictionary)[NSAttributedString.Key.exactEdgeRoom] as? CGFloat ?? 0
                    // The room is right of the glyph that reaches furthest.
                    let reach = (0..<count).max { positions[$0].x + advances[$0].width < positions[$1].x + advances[$1].width }
                    for g in 0..<count where indices[g] >= start && indices[g] < end && positions[g].x >= 0 {
                        lo = min(lo, positions[g].x); hi = max(hi, positions[g].x + advances[g].width - (g == reach ? room : 0))
                    }
                }
                let (first, last) = (i == mine.first, i == mine.last)
                let border = try XCTUnwrap(fill.border)
                XCTAssertEqual(fill.rect.maxX, hi + (first ? 6 : 0), accuracy: 0.05, "\(texts) line \(i): right")
                XCTAssertEqual(fill.rect.minX, lo - (last ? 6 : 0), accuracy: 0.05, "\(texts) line \(i): left")
                XCTAssertEqual(border.widths, [3, first ? 3 : 0, 3, last ? 3 : 0], "\(texts) line \(i)")
            }
        }
    }

    /// The kernel's request carries the edges, and the paragraph it measures
    /// is the one painted from the same runs.
    func testTheRequestsEdgesAreThePaintedSpecs() {
        let texts = ["1", "The"].map { Array($0.utf8) }
        let spec = texts[0].withUnsafeBufferPointer { a in
            texts[1].withUnsafeBufferPointer { b in
                var runs = [a, b].map { bytes -> ExactTextRun in
                    var r = ExactTextRun()
                    r.text = bytes.baseAddress; r.len = bytes.count
                    r.font_size = 16; r.font_weight = 400; r.font_stretch = 100
                    return r
                }
                runs[0].inline_end = 4.8
                var strut = ExactTextRun()
                strut.font_size = 16; strut.font_weight = 400; strut.font_stretch = 100
                return runs.withUnsafeBufferPointer { buffer in
                    var request = ExactMeasureRequest()
                    request.runs = buffer.baseAddress; request.count = buffer.count; request.strut = strut
                    request.width = -1; request.height = -1
                    return engine.requestSpec(request)
                }
            }
        }
        XCTAssertEqual(spec.runs.map(\.inlineEnd), [CGFloat(Float(4.8)), 0])
    }
}
