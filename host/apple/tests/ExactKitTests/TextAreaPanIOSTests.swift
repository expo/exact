#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

/// A textarea pans only text that overflows its box (`TextArea.fits`), as a
/// browser's scrolls: UIKit's content size rounds up to a whole point, so a
/// field sized to its one line would otherwise scroll by the fraction, and a
/// drag on it would be the field's and no ancestor's.
final class TextAreaPanIOSTests: XCTestCase {
    func testAFieldSizedToItsTextDoesNotPanAndOneItsTextOverflowsMay() {
        let field = TextArea(usingTextLayoutManager: true)
        field.font = .systemFont(ofSize: 17)
        field.textContainerInset = .zero
        field.textContainer.lineFragmentPadding = 0
        field.text = "One line"
        field.frame = CGRect(x: 0, y: 0, width: 240, height: field.font!.lineHeight)
        field.layoutIfNeeded()
        XCTAssertGreaterThan(field.contentSize.height, field.bounds.height, "UIKit's content size rounds up past the box")
        XCTAssertTrue(field.fits)
        XCTAssertFalse(field.gestureRecognizerShouldBegin(field.panGestureRecognizer))
        field.text = "One line\nTwo\nThree"
        field.layoutIfNeeded()
        XCTAssertFalse(field.fits, "three lines in a box of one")
        // Text that overflows pans as UIKit's own text view does
        let plain = UITextView(usingTextLayoutManager: true)
        plain.font = field.font
        plain.textContainerInset = .zero
        plain.textContainer.lineFragmentPadding = 0
        plain.text = field.text
        plain.frame = field.frame
        plain.layoutIfNeeded()
        XCTAssertEqual(field.gestureRecognizerShouldBegin(field.panGestureRecognizer), plain.gestureRecognizerShouldBegin(plain.panGestureRecognizer))
    }
}
#endif
