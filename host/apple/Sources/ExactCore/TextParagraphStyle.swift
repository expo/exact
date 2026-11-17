// A paragraph's attributed source's paragraph style: CSS tab stops and the
// base direction, which `TextEngine.attributed` sets after its runs.
#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif
import CoreText
import CExact

extension TextEngine {
    /// Eight spaces of `font`, letter spacing included: a tab stop's interval.
    package static func tabInterval(_ font: CTFont, letterSpacing: CGFloat) -> CGFloat {
        let space = CTLineGetTypographicBounds(CTLineCreateWithAttributedString(
            NSAttributedString(string: " ", attributes: [.font: font])), nil, nil, nil)
        return 8 * (CGFloat(space) + letterSpacing)
    }

    /// CSS `tab-size: 8` (its initial value): a preserved tab advances to the
    /// next multiple of eight spaces of the paragraph's own font, letter
    /// spacing included, from the line's start. CoreText's default is twelve
    /// stops 28 pt apart; Chrome's is this (LLP 1053 G5).
    func setTabStops(_ s: NSMutableAttributedString, run: Run) {
        TextEngine.setTabStops(s, interval: TextEngine.tabInterval(font(run) as CTFont, letterSpacing: run.letterSpacing))
    }

    package static func setTabStops(_ s: NSMutableAttributedString, interval: CGFloat) {
        guard interval > 0, s.length > 0 else { return }
        let range = NSRange(location: 0, length: s.length)
        let paragraph = ((s.attribute(.paragraphStyle, at: 0, effectiveRange: nil) as? NSParagraphStyle)?.mutableCopy() as? NSMutableParagraphStyle) ?? NSMutableParagraphStyle()
        paragraph.tabStops = []
        paragraph.defaultTabInterval = interval
        s.addAttribute(.paragraphStyle, value: paragraph, range: range)
    }

    /// CSS direction sets the paragraph base; normal Unicode bidi still
    /// orders each run. The first strong character selects the base only in
    /// natural text (LLP 1060.000 D3), each paragraph's its own.
    package static func setBaseDirection(_ s: NSMutableAttributedString, direction: Int, natural: Bool = false) {
        setBaseDirection(s, bases: natural ? naturalBases(s.string, fallback: direction) : [(start: 0, rtl: direction == 1)])
    }

    /// Each paragraph's base from its UTF-16 start to the next one's.
    package static func setBaseDirection(_ s: NSMutableAttributedString, bases: [(start: Int, rtl: Bool)]) {
        for (i, base) in bases.enumerated() {
            let end = i + 1 < bases.count ? bases[i + 1].start : s.length
            guard end > base.start else { continue }
            s.enumerateAttribute(.paragraphStyle, in: NSRange(location: base.start, length: end - base.start)) { value, range, _ in
                let paragraph = (value as? NSParagraphStyle)?.mutableCopy() as? NSMutableParagraphStyle ?? NSMutableParagraphStyle()
                paragraph.baseWritingDirection = base.rtl ? .rightToLeft : .leftToRight
                s.addAttribute(.paragraphStyle, value: paragraph, range: range)
            }
        }
    }

    /// Natural text's paragraphs (LLP 1060.000 D3): each one's UTF-16 start
    /// and whether its base is right to left, as the kernel resolves them
    /// (`exact_text_paragraph_bases`), split where the kernel splits: at
    /// UAX #9 paragraph separators, a carriage return and line feed as one.
    /// `fallback` (the layout direction) is the base of one without a strong
    /// character.
    package static func naturalBases(_ text: String, fallback: Int) -> [(start: Int, rtl: Bool)] {
        var utf8 = Array(text.utf8)
        let count = utf8.withUnsafeMutableBufferPointer { exact_text_paragraph_bases($0.baseAddress, $0.count, UInt8(fallback == 1 ? 1 : 0), nil, 0) }
        guard count > 0 else { return [(0, fallback == 1)] }
        var directions = [UInt8](repeating: 0, count: count)
        _ = utf8.withUnsafeMutableBufferPointer { exact_text_paragraph_bases($0.baseAddress, $0.count, UInt8(fallback == 1 ? 1 : 0), &directions, count) }
        var starts = [0], at = 0, afterCR = false
        for unit in text.utf16 {
            at += 1
            let separator = unit == 0x0A || unit == 0x0D || (0x1C...0x1E).contains(unit) || unit == 0x85 || unit == 0x2029
            if separator && !(afterCR && unit == 0x0A) { starts.append(at) }
            else if separator, let last = starts.indices.last { starts[last] = at }
            afterCR = unit == 0x0D
        }
        return zip(starts, directions).map { ($0, $1 == 1) }
    }
}
