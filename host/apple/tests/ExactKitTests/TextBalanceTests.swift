// CSS `text-wrap-style: balance` on Apple (LLP 1001 §6): the lines before
// the first forced break, up to six, break where Chrome's do; the box
// measures as greedy lines. The expected lines and widths are Chrome's for
// 16px Arial and 10px Menlo (a Range per line).
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
