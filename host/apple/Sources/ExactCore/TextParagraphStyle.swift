// A paragraph's attributed source's paragraph style: CSS tab stops and the
// base direction, which `TextEngine.attributed` sets after its runs.
#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif
import CoreText

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
    /// orders each run. The first strong character never selects the base.
    package static func setBaseDirection(_ s: NSMutableAttributedString, direction: Int) {
        let range = NSRange(location: 0, length: s.length)
        s.enumerateAttribute(.paragraphStyle, in: range) { value, range, _ in
            let paragraph = (value as? NSParagraphStyle)?.mutableCopy() as? NSMutableParagraphStyle ?? NSMutableParagraphStyle()
            paragraph.baseWritingDirection = direction == 1 ? .rightToLeft : .leftToRight
            s.addAttribute(.paragraphStyle, value: paragraph, range: range)
        }
    }
}
