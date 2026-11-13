#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

final class TextAreaPlaceholderIOSTests: XCTestCase {
    private func field(_ placeholder: String, height: CGFloat, lineHeight: CGFloat? = nil,
                       direction: NSWritingDirection = .leftToRight) -> TextArea {
        let field = TextArea(usingTextLayoutManager: true)
        field.frame = CGRect(x: 0, y: 0, width: 240, height: height)
        field.font = .systemFont(ofSize: 17)
        field.textContainerInset = UIEdgeInsets(top: 13, left: 11, bottom: 19, right: 23)
        field.textContainer.lineFragmentPadding = 0
        field.placeholder = placeholder
        field.placeholderColor = .black
        field.textAlignment = direction == .rightToLeft ? .right : .left
        field.applyLineHeight(lineHeight, direction: direction)
        field.layoutIfNeeded()
        return field
    }

    private func renderer(_ bounds: CGRect) -> UIGraphicsImageRenderer {
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        format.opaque = true
        return UIGraphicsImageRenderer(bounds: bounds, format: format)
    }

    private func drawPlaceholder(_ field: TextArea, height: CGFloat) {
        let box = field.bounds.inset(by: field.textContainerInset)
        (field.placeholder as NSString).draw(
            with: CGRect(x: box.minX, y: box.minY, width: max(0, box.width), height: height),
            options: .usesLineFragmentOrigin,
            attributes: [.font: field.font!, .foregroundColor: UIColor.black,
                         .paragraphStyle: field.typingAttributes[.paragraphStyle]!], context: nil)
    }

    private func image(_ field: TextArea, reference: Bool = false) -> UIImage {
        // An unbounded rect can omit glyph runs. Draw the whole short fixture
        // in a finite rect before cropping, including partially visible lines.
        let bounds = reference ? CGRect(x: 0, y: 0, width: field.bounds.width, height: 1024) : field.bounds
        let drawn = renderer(bounds).image { context in
            UIColor.white.setFill()
            context.fill(bounds)
            context.cgContext.clip(to: bounds)
            if reference { drawPlaceholder(field, height: bounds.height) } else { field.draw(field.bounds) }
        }
        return reference ? UIImage(cgImage: drawn.cgImage!.cropping(to: field.bounds)!) : drawn
    }

    func testShortAndClippedPlaceholdersKeepTheirVisibleText() throws {
        for direction: NSWritingDirection in [.leftToRight, .rightToLeft] {
            for lineHeight: CGFloat? in [nil, 12, 48] {
                let pitch = lineHeight ?? 21
                for height in [13 + ceil(pitch * 0.7), 13 + ceil(pitch * 1.7), 140] {
                    for placeholder in ["Ag مرحبا", String(repeating: "Ag مرحبا wraps to the next line. ", count: 8)] {
                        let field = field(placeholder, height: height, lineHeight: lineHeight, direction: direction)
                        let drawn = image(field), reference = image(field, reference: true)
                        let actual = try XCTUnwrap(drawn.cgImage)
                        let expected = try XCTUnwrap(reference.cgImage)
                        let pixels = CapturePixels.bytes(actual)
                        if pixels != CapturePixels.bytes(expected) {
                            XCTContext.runActivity(named: "Placeholder pixels") { activity in
                                activity.add(XCTAttachment(image: drawn))
                                activity.add(XCTAttachment(image: reference))
                            }
                        }
                        XCTAssertTrue(pixels == CapturePixels.bytes(expected), "\(direction), line \(String(describing: lineHeight)), height \(height)")
                        XCTAssertTrue(stride(from: 0, to: pixels.count, by: 4).contains { pixels[$0] < 128 }, "the placeholder drew")
                    }
                }
            }
        }
    }

    func testALongPlaceholderDoesNotLayOutAllItsLines() {
        let field = field(String(repeating: "Placeholder text keeps wrapping. ", count: 65_536), height: 47)
        let renderer = renderer(field.bounds)
        func elapsed(reference: Bool) -> Double {
            let start = CACurrentMediaTime()
            autoreleasepool {
                _ = renderer.image { context in
                    context.cgContext.clip(to: field.bounds)
                    if reference { drawPlaceholder(field, height: .greatestFiniteMagnitude) } else { field.draw(field.bounds) }
                }
            }
            return CACurrentMediaTime() - start
        }
        _ = elapsed(reference: true)
        _ = elapsed(reference: false)
        var actual: [Double] = [], fullLayout: [Double] = []
        for round in 0..<5 {
            if round.isMultiple(of: 2) {
                actual.append(elapsed(reference: false))
                fullLayout.append(elapsed(reference: true))
            } else {
                fullLayout.append(elapsed(reference: true))
                actual.append(elapsed(reference: false))
            }
        }
        let drawn = actual.sorted()[2], full = fullLayout.sorted()[2]
        print("textarea placeholder: \(field.placeholder.utf16.count) UTF-16 units, draw \(drawn * 1000) ms, unbounded \(full * 1000) ms (medians of 5)")
        XCTAssertLessThan(drawn, full / 2, "a two-line view must not pay for the entire placeholder's layout")
    }
}
#endif
