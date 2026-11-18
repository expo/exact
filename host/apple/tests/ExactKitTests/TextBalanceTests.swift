// CSS `text-wrap-style: balance` on Apple (LLP 1001 §6): the lines before
// the first forced break, up to six, break where Chrome's do; the box
// measures as greedy lines. The expected lines and widths are Chrome's for
// 16px Arial and 10px Menlo (a Range per line). Menlo's ideographs fall back
// to a face 1em wide, as `Menlo, "Hiragino Sans GB"` draws them in Chrome.
import XCTest
import CoreText
@testable import ExactKit

final class TextBalanceTests: XCTestCase {
    private let engine: TextEngine = {
        let engine = TextEngine(resolve: { _ in nil })
        let arial = CTFontDescriptorCreateWithNameAndSize("ArialMT" as CFString, 16)
        let menlo = CTFontDescriptorCreateWithNameAndSize("Menlo-Regular" as CFString, 10)
        engine.catalog = [9: [RegisteredFace(family: "Arial", generic: nil, weight: 400, italic: false, descriptor: arial)],
                          10: [RegisteredFace(family: "Menlo", generic: nil, weight: 400, italic: false, descriptor: menlo)]]
        return engine
    }()
    private let sentence = "Settle every thread in this section and keep the newest one open"

    private func spec(_ text: String, balance: Bool, align: Int = 0, whiteSpace: Int = 0, clamp: Int = 0, menlo: Bool = false) -> Spec {
        let run = Run(text: text, size: menlo ? 10 : 16, weight: 400, family: menlo ? 10 : 9, italic: false, lineHeight: nil, letterSpacing: 0)
        var s = Spec(runs: [run], align: align, lineClamp: clamp, color: [0, 0, 0, 255], whiteSpace: whiteSpace)
        s.balance = balance
        return s
    }
    private func lines(_ p: Paragraph, _ text: String) -> [String] {
        p.lines.map {
            let r = CTLineGetStringRange($0)
            return (text as NSString).substring(with: NSRange(location: r.location, length: r.length))
        }
    }
    private func inked(_ p: Paragraph) -> [CGFloat] {
        p.lines.map { CGFloat(CTLineGetTypographicBounds($0, nil, nil, nil) - CTLineGetTrailingWhitespaceWidth($0)) }
    }
    /// The lines `spec` balances into at `width`, checked for what holds of
    /// every balanced paragraph: as many lines as greedy, each in the box
    /// unless it is the greedy line, a measure no less than the greedy one,
    /// and the same lines and measure from the region worker.
    private func balanced(_ spec: Spec, _ width: CGFloat, file: StaticString = #filePath, line: UInt = #line) -> [String] {
        let text = spec.runs.map(\.text).joined()
        var unbalanced = spec
        unbalanced.balance = false
        let p = engine.paragraph(spec, width: width), greedy = engine.paragraph(unbalanced, width: width)
        let made = lines(p, text), plain = lines(greedy, text)
        let name = "\(text.debugDescription) at \(width)"
        XCTAssertGreaterThanOrEqual(p.width, greedy.width, "the greedy measure: \(name)", file: file, line: line)
        XCTAssertEqual(made.count, plain.count, "the line count: \(name)", file: file, line: line)
        guard made.count == plain.count, let source = p.shape?.attributed else { return made }
        for (i, s) in made.enumerated() {
            // The line's width in the box: its inset in it, and none of the
            // start room it leaves the run that starts the next line.
            let range = CTLineGetStringRange(p.lines[i])
            let ink = CTLineGetTypographicBounds(p.lines[i], nil, nil, nil) - CTLineGetTrailingWhitespaceWidth(p.lines[i])
                - Double(TextEngine.breakRoom(endingAt: range.location + range.length, in: source))
                + Double(p.insets.at(range.location).width)
            XCTAssert(ink <= Double(width) + 1 / 64 || s == plain[i], "line \(i) overflows: \(name)", file: file, line: line)
        }
        for compact in [false, true] {
            let region = region(spec, width, compact: compact)
            XCTAssertEqual(region.lines, made, "the region worker's lines, compact \(compact): \(name)", file: file, line: line)
            XCTAssertEqual(region.width, p.width, "the region worker's measure, compact \(compact): \(name)", file: file, line: line)
        }
        return made
    }
    /// The lines the region worker makes of `spec` at `width`, and their measure.
    private func region(_ spec: Spec, _ width: CGFloat, compact: Bool) -> (lines: [String], width: CGFloat) {
        let source = RegionTextSource.capture(spec, engine: engine), made = RegionLines(), done = DispatchSemaphore(value: 0)
        DispatchQueue.global().async {
            let shaped = RegionWorkerLayout.shape(source, width: width, compact: compact).metadata
            made.lines = shaped.lines.map { (source.text as NSString).substring(with: $0.range) }
            made.width = shaped.width
            done.signal()
        }
        done.wait()
        return (made.lines, made.width)
    }
    /// Balanced Menlo runs of one size, each with the room its inline box
    /// takes before and after it: a `<span>`'s `padding-left` and
    /// `padding-right`.
    private func spans(_ runs: [(text: String, start: CGFloat, end: CGFloat)], size: CGFloat) -> Spec {
        var s = spec("", balance: true, menlo: true)
        s.runs = runs.map { run in
            var r = s.runs[0]
            r.text = run.text; r.size = size; r.inlineStart = run.start; r.inlineEnd = run.end
            return r
        }
        return s
    }

    func testTheIssuesSentenceBalancesAsChromeDoes() {
        let greedy = engine.paragraph(spec(sentence, balance: false), width: 180)
        XCTAssertEqual(lines(greedy, sentence), ["Settle every thread in ", "this section and keep the ", "newest one open"])
        let balanced = engine.paragraph(spec(sentence, balance: true), width: 180)
        XCTAssertEqual(lines(balanced, sentence), ["Settle every thread in ", "this section and keep ", "the newest one open"])
        for (width, chrome) in zip(inked(balanced), [151.2, 150.33, 148.56]) {
            XCTAssertEqual(width, chrome, accuracy: 0.5)
        }
        XCTAssertEqual(balanced.width, greedy.width, "balancing moves breaks, never the box")
        XCTAssertEqual(balanced.height, greedy.height)
        // Centred, the lines centre in the box's width, not the balanced one.
        let centred = engine.paragraph(spec(sentence, balance: true, align: 1), width: 180)
        XCTAssertEqual(lines(centred, sentence), lines(balanced, sentence))
        XCTAssertEqual(CGFloat(CTLineGetPenOffsetForFlush(centred.lines[0], 0.5, 180)), 14.39, accuracy: 0.5)
        // Justified, all but the last fill the box, as Chrome's do.
        let justified = engine.paragraph(spec(sentence, balance: true, align: 3), width: 180)
        XCTAssertEqual(lines(justified, sentence), lines(balanced, sentence))
        XCTAssertEqual(inked(justified)[0], 180, accuracy: 0.5)
    }

    /// The report's headings (expo/exact#437) in 10px Menlo, which Chrome 157
    /// scores rather than breaking at the narrowest width that keeps three
    /// lines: that width gives "Seven studies / in live / typography" and
    /// "A road / that ended / at the sea".
    func testTheReportsHeadingsBreakWhereChromesScorerBreaksThem() {
        let seven = "Seven studies in live typography"
        XCTAssertEqual(lines(engine.paragraph(spec(seven, balance: true, menlo: true), width: 96), seven),
                       ["Seven ", "studies in ", "live typography"])
        let road = "A road that ended at the sea"
        XCTAssertEqual(lines(engine.paragraph(spec(road, balance: true, menlo: true), width: 90), road),
                       ["A road that ", "ended at ", "the sea"])
    }

    /// An inline box's start room goes to the scorer with its run: the
    /// report's first heading, its last two words a padded run, breaks as
    /// it does unpadded, and stays greedy when the room leaves "live
    /// typography" no line of its own. A line that ends before the run
    /// after a hyphen, where no space hangs with the room, leaves it out:
    /// "ccc ddddddd-" fits only without it.
    func testAPaddedRunsStartRoomIsScoredWithItsRun() {
        func broken(_ first: String, _ second: String, room: CGFloat) -> [String] {
            var s = spec(first, balance: true, menlo: true)
            var run = s.runs[0]
            run.text = second; run.inlineStart = room
            s.runs.append(run)
            return lines(engine.paragraph(s, width: 96), first + second)
        }
        XCTAssertEqual(broken("Seven studies in ", "live typography", room: 1), ["Seven ", "studies in ", "live typography"])
        XCTAssertEqual(broken("Seven studies in ", "live typography", room: 6), ["Seven studies ", "in live ", "typography"])
        XCTAssertEqual(broken("aaaaa bbbbb ccc ddddddd-", "eeee ffff", room: 30), ["aaaaa bbbbb ", "ccc ddddddd-", "eeee ffff"])
    }

    /// Chromium 159's lines where an inline box has room beside its text. A
    /// run's start room counts on the line the run starts and not on a line
    /// that ends before it, and Blink's scorer counts no room after a line's
    /// last character in that line.
    func testABoxesRoomCountsWhereChromiumsDoes() {
        let cases: [(runs: [(text: String, start: CGFloat, end: CGFloat)], size: CGFloat, width: CGFloat, chromium: [String])] = [
            // Greedy 4 / 2 / 2, whose first line ends before the room.
            ([("中文中文", 0, 0), ("中文中文", 20, 0)], 16, 64, ["中文中", "文中", "文中文"]),
            ([("中文中文中", 0, 0), ("文中文中文", 30, 0)], 16, 96, ["中文中文", "中文", "中文中文"]),
            // A run starts a line: after an ideograph,
            ([("中文", 0, 0), ("中文中文中", 20, 0)], 16, 50, ["中文", "中", "文中", "文中"]),
            ([("中文", 0, 0), ("中文中文中", 8, 0)], 16, 40, ["中文", "中", "文中", "文中"]),
            ([("中文中文", 0, 0), ("中文", 20, 0), ("中文中文中文", 0, 8)], 16, 90, ["中文中文", "中文中", "文中文中文"]),
            // after a space, which hangs,
            ([("aa bb cc ", 0, 0), ("dd ee ff gg", 20, 0)], 10, 72, ["aa bb cc ", "dd ", "ee ff gg"]),
            ([("Seven studies ", 0, 0), ("in live typography", 20, 0)], 10, 96, ["Seven studies ", "in ", "live typography"]),
            ([("A road that ", 0, 0), ("ended at the sea", 15, 0)], 10, 66, ["A road ", "that ", "ended ", "at the sea"]),
            ([("aa bb ", 0, 0), ("cc dd", 10, 0), (" ee ff gg", 0, 0)], 10, 60, ["aa bb ", "cc dd ", "ee ff gg"]),
            // and after a hyphen.
            ([("aa bb-", 0, 0), ("cc dd ee ff", 20, 0)], 10, 60, ["aa bb-", "cc ", "dd ee ff"]),
            // Rooms after a line's last character: a box's end, at the
            // paragraph's end and before a space, and the start of a box
            // that starts with a space.
            ([("中文中文中文中文中文", 0, 20)], 16, 146, ["中文中文中", "文中文中文"]),
            ([("aa bb ", 0, 0), ("cc", 8, 8), (" dd ee ff gg", 0, 0)], 10, 100, ["aa bb cc dd ", "ee ff gg"]),
            ([("live road short newest", 0, 0), (" that these", 20, 0)], 10, 116, ["live road ", "short newest ", "that these"]),
            // That start room is on neither line where the line breaks at
            // the space: "aa bb" and "cc dd", 30.1px each, lose to "aa" and
            // "bb cc dd" in a 75px box, and win in a 70px one.
            ([("aa bb", 0, 0), (" cc dd", 20, 0)], 10, 75, ["aa ", "bb cc dd"]),
            ([("aa bb", 0, 0), (" cc dd", 20, 0)], 10, 70, ["aa bb ", "cc dd"])
        ]
        for c in cases {
            XCTAssertEqual(balanced(spans(c.runs, size: c.size), c.width), c.chromium, "\(c.runs.map(\.text)) at \(c.width)")
        }
        // Greedy, the first line ends at the hanging space that ends its
        // box, where the box's end room is kerned: the breaker leaves the
        // room out with the space, and the line drawn has it beside its last
        // glyph, past the box's edge. Chromium balances the paragraph.
        let hung = spans([("us newest an ", 0, 20), ("open us words that", 12, 0)], size: 10)
        XCTAssertEqual(balanced(hung, 81), ["us newest ", "an open ", "us words that"])
    }

    /// A balanced line stays in the box. Chromium breaks where its scorer
    /// says, whatever fits, and the scorer leaves a box's end room out of
    /// the line that ends at it: its lines here are "live wraps" and "at
    /// lines open", the second 98.3px with its room in a 93px box.
    func testScoredLinesOfWhichOneWouldOverflowByItsBoxesEndRoomAreNotTaken() {
        let boxed = spans([("live wraps at lines open", 0, 20)], size: 10)
        XCTAssertEqual(balanced(boxed, 93), ["live wraps at ", "lines open"])
        XCTAssertEqual(balanced(boxed, 99), ["live wraps ", "at lines open"])
    }

    /// Balancing moves breaks, never the box, so the greedy lines measure
    /// as the layout draws them. Greedy, the second line here ends at the
    /// hanging space that ends its box, and the line drawn has the box's
    /// 20px end room beside its last glyph: 123.63px, where no balanced line
    /// is wider than 108.05px (16px Arial).
    func testABalancedParagraphMeasuresAsItsGreedyLinesAreDrawn() {
        var boxed = spans([("these ", 0, 0), ("newest longer", 12, 8), (" keep ", 12, 20), ("title", 20, 4)], size: 16)
        for i in boxed.runs.indices { boxed.runs[i].family = 9 }
        XCTAssertEqual(balanced(boxed, 137), ["these newest ", "longer ", "keep title"])
        let measure = engine.paragraph(boxed, width: 137).width
        boxed.balance = false
        let greedy = engine.paragraph(boxed, width: 137)
        XCTAssertEqual(lines(greedy, "these newest longer keep title"), ["these newest ", "longer keep ", "title"])
        XCTAssertEqual(greedy.width, 123.63, accuracy: 0.5)
        XCTAssertEqual(measure, greedy.width)
        // The region worker draws and measures the same lines.
        for compact in [false, true] { XCTAssertEqual(region(boxed, 137, compact: compact).width, greedy.width, "compact \(compact)") }
    }

    /// Core Text returns one cluster even where it overflows the room, and
    /// Chromium balances no paragraph with an overflowing line.
    func testAnOverflowingSingleClusterKeepsGreedyLines() {
        for (size, indent) in [(CGFloat(100), CGFloat(0)), (70, 10)] {
            var s = spec("M ", balance: true, whiteSpace: 1, menlo: true)
            var tail = s.runs[0]
            tail.text = "aa bb cc dd"
            s.runs[0].size = size
            s.runs.append(tail)
            s.textIndent = indent
            XCTAssertEqual(balanced(s, 50), ["M ", "aa bb cc ", "dd"])
            XCTAssertGreaterThan(inked(engine.paragraph(s, width: 50))[0] + indent, 50)
        }
        // A 60px "M" is 36px wide: it fits the box alone and balances, and
        // overflows it after its own 20px start room.
        var boxed = spans([("aa ", 0, 0), ("M", 0, 0), (" bb cc dd ee", 0, 0)], size: 10)
        boxed.runs[1].size = 60
        XCTAssertEqual(balanced(boxed, 50), ["aa ", "M ", "bb cc ", "dd ee"])
        boxed.runs[1].inlineStart = 20
        XCTAssertEqual(balanced(boxed, 50), ["aa ", "M ", "bb cc dd ", "ee"])
        // An ideograph that overflows, before a run with a start room.
        var wide = spans([("中", 0, 0), ("aa bb cc dd ee", 20, 0)], size: 10)
        wide.runs[0].size = 60
        XCTAssertEqual(balanced(wide, 50), ["中", "aa ", "bb cc dd ", "ee"])
    }

    /// Breakings whose lines are as wide in another order score the same
    /// but for rounding, and Blink's positions are whole layout units
    /// (Chromium 159).
    func testEquallyWideBreakingsFallWhereChromiumsDo() {
        func words(_ count: Int) -> Spec { spec([String](repeating: "aa", count: count).joined(separator: " "), balance: true, menlo: true) }
        let pair = "aa aa "
        XCTAssertEqual(balanced(words(9), 32), [pair, pair, pair, "aa ", "aa aa"])
        XCTAssertEqual(balanced(words(11), 32), [pair, pair, pair, pair, "aa ", "aa aa"])
        // 16px Arial: "other what these me measure" and "that road his fox
        // extraordinary" are both 211.67px wide.
        let tie = "other what these me measure in that road his fox extraordinary"
        XCTAssertEqual(balanced(spec(tie, balance: true), 341), ["other what these me measure ", "in that road his fox extraordinary"])
    }

    /// Where Blink's scorer gives nothing (Chrome 157, 16px Arial): three
    /// words bisect the width; a word wider than the box, or broken inside
    /// under `break-word`, keeps every line greedy; and so does a short first
    /// paragraph before a forced break, which the bisection cannot take.
    func testShortParagraphsBisectAndWhatBlinkLeavesAloneStaysGreedy() {
        let short = "large under they"
        XCTAssertEqual(lines(engine.paragraph(spec(short, balance: false), width: 107.6), short), ["large under ", "they"])
        XCTAssertEqual(lines(engine.paragraph(spec(short, balance: true), width: 107.6), short), ["large ", "under they"])
        let long = "Supercalifragilisticexpialidocious is a very long word"
        XCTAssertEqual(lines(engine.paragraph(spec(long, balance: true), width: 120), long),
                       ["Supercalifragilisticexpialidocious ", "is a very long ", "word"])
        var breakWord = spec(long, balance: true)
        breakWord.overflowWrap = 1
        XCTAssertEqual(lines(engine.paragraph(breakWord, width: 120), long),
                       ["Supercalifragilist", "icexpialidocious ", "is a very long ", "word"])
        let forced = "large under they\nnext"
        XCTAssertEqual(lines(engine.paragraph(spec(forced, balance: true, whiteSpace: 3), width: 107.6), forced),
                       ["large under ", "they\n", "next"])
    }

    #if os(iOS)
    /// UILabel's emergency hyphens (LLP 1001 §6, iOS emergency word breaks)
    /// are no place to balance at: a word that fits its line stays whole,
    /// whether the scorer has the paragraph (four words) or the bisection.
    func testAWordThatFitsItsLineIsNotHyphenatedToBalance() {
        let word = "antidisestablishmentarianism"
        func prepared(_ text: String, balance: Bool) -> Spec {
            var s = spec(text, balance: balance)
            s.overflowWrap = 1; s.language = "en"
            s.hyphenateAuto()
            return s
        }
        let room = engine.paragraph(prepared(word, balance: false), width: .infinity).width + 1
        for text in ["aa bb cc " + word, "Work " + word, "Work " + word + " x"] {
            let balanced = prepared(text, balance: true), shaped = balanced.runs[0].text
            XCTAssertFalse(balanced.emergencyBreaks.isEmpty)
            let broken = lines(engine.paragraph(balanced, width: room), shaped)
            XCTAssertEqual(broken, lines(engine.paragraph(prepared(text, balance: false), width: room), shaped), text)
            XCTAssertFalse(broken.contains { $0.hasSuffix("\u{ad}") }, "\(text): \(broken)")
        }
    }
    #endif

    /// A soft hyphen's break costs the scorer a penalty, so Chrome's lines
    /// end at none here where the greedy third line ends "un-".
    func testSoftHyphensBalanceAsChromesDo() {
        let text = "extra\u{ad}ordinary circum\u{ad}stances pre\u{ad}vail in un\u{ad}usual times"
        XCTAssertEqual(lines(engine.paragraph(spec(text, balance: false), width: 120), text),
                       ["extra\u{ad}ordinary ", "circum\u{ad}stances ", "pre\u{ad}vail in un\u{ad}", "usual times"])
        XCTAssertEqual(lines(engine.paragraph(spec(text, balance: true), width: 120), text),
                       ["extra\u{ad}ordinary ", "circum\u{ad}stances ", "pre\u{ad}vail in ", "un\u{ad}usual times"])
    }

    func testRecolouringBalancedTextKeepsItsMeasuredWidth() {
        let original = engine.paragraph(spec(sentence, balance: true), width: 180)
        var red = spec(sentence, balance: true)
        red.color = [255, 0, 0, 255]
        let recoloured = engine.paragraph(red, width: 180)
        XCTAssertEqual(lines(recoloured, sentence), lines(original, sentence))
        XCTAssertEqual(recoloured.width, original.width)
        XCTAssertEqual(recoloured.height, original.height)
        let fromRanges = engine.layout(original.shape!, width: 180,
                                       ranges: original.lines.map { CTLineGetStringRange($0) })
        XCTAssertEqual(fromRanges.width, original.width)
        withExtendedLifetime(original) {}
    }

    /// Chrome balances only the lines before the first forced break; the
    /// rest break greedily at the box's width.
    func testOnlyTheLinesBeforeTheFirstForcedBreakBalance() {
        let text = "Settle every thread in this section\nand keep the newest one open now please ok"
        let p = engine.paragraph(spec(text, balance: true, whiteSpace: 3), width: 180)
        XCTAssertEqual(lines(p, text), ["Settle every thread ", "in this section\n", "and keep the newest one ", "open now please ok"])
    }

    /// Seven lines, or a clamp, stay greedy, as in Chrome.
    func testMoreThanSixLinesAndAClampStayGreedy() {
        let seven = "a b c d e f g h i j k l m n o p q r s t u v w x y z aa bb cc dd ee ff gg hh ii jj kk ll mm nn"
        let greedy = lines(engine.paragraph(spec(seven, balance: false), width: 100), seven)
        XCTAssertEqual(greedy.count, 7)
        XCTAssertEqual(lines(engine.paragraph(spec(seven, balance: true), width: 100), seven), greedy)
        let six = "a b c d e f g h i j k l m n o p q r s t u v w x y z aa bb cc dd ee ff gg hh ii jj kk"
        XCTAssertEqual(lines(engine.paragraph(spec(six, balance: true), width: 100), six),
                       ["a b c d e f g ", "h i j k l m n ", "o p q r s t u ", "v w x y z aa ", "bb cc dd ee ff ", "gg hh ii jj kk"], "Chrome's six lines")
        let clamped = engine.paragraph(spec(sentence, balance: true, clamp: 3), width: 180)
        XCTAssertEqual(lines(clamped, sentence)[1], "this section and keep the ")
    }
}

/// What a region worker made, read after its queue signals.
private final class RegionLines: @unchecked Sendable {
    var lines: [String] = []
    var width: CGFloat = 0
}
