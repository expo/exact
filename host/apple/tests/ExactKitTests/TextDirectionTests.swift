import XCTest
import CoreText
#if os(macOS)
import AppKit
#else
import UIKit
#endif
@testable import ExactKit

final class TextDirectionTests: XCTestCase {
    func testSelectionsCoverTheirGlyphsOnMixedDirectionLines() {
        let engine = TextEngine(resolve: { _ in nil })
        let cases: [(String, Int, NSRange)] = [
            ("abc אבג def", 0, NSRange(location: 4, length: 3)),
            ("abc אבג def", 0, NSRange(location: 1, length: 5)),
            ("abc אבג def", 0, NSRange(location: 5, length: 4)),
            ("אבג abc דהו", 1, NSRange(location: 4, length: 3)),
            ("שלום 2026 עולם", 1, NSRange(location: 5, length: 4)),
            ("שלום עולם", 1, NSRange(location: 0, length: 4)),
            ("plain words", 0, NSRange(location: 2, length: 6)),
            ("عربي", 1, NSRange(location: 0, length: 4)),
            ("abc عربي def", 0, NSRange(location: 4, length: 4)),
            ("مُحَمَّد", 1, NSRange(location: 0, length: 8)),
            ("abc אבג def", 0, NSRange(location: 0, length: 11)),
            ("abc\u{200F}אבג def", 0, NSRange(location: 0, length: 3)),
            ("abc\u{200F}אבג def", 0, NSRange(location: 1, length: 2)),
            ("אבג\u{200E}abc דהו", 1, NSRange(location: 0, length: 3)),
            ("אבג\u{200E}abc דהו", 1, NSRange(location: 1, length: 2)),
            // CoreText places the caret before a run's trailing mark off the run.
            ("abc אבג\u{200F} def", 0, NSRange(location: 4, length: 3)),
            ("אבג abc\u{200F}", 1, NSRange(location: 4, length: 3)),
            ("abc\u{200F}", 0, NSRange(location: 0, length: 3)),
            // A mark alone at the end of a run paints nothing.
            ("abc \u{200F}אבג", 0, NSRange(location: 4, length: 2)),
            ("abc אבג\u{200F} def", 0, NSRange(location: 7, length: 2)),
        ]
        for (text, direction, selection) in cases {
            let run = Run(text: text, size: 16, weight: 400, family: 0,
                italic: false, lineHeight: nil, letterSpacing: 0)
            var spec = Spec(runs: [run], align: 0, lineClamp: 0, color: [0, 0, 0, 255], strut: run)
            spec.direction = direction
            let paragraph = engine.paragraph(spec, width: 300)
            XCTAssertEqual(paragraph.lines.count, 1)
            let bounds = CGRect(x: 17, y: 23, width: 300, height: 40)
            let expected = selectedGlyphExtents(paragraph.lines[0], selection)
            let origin = bounds.minX + paragraph.origin(0, align: 0, width: bounds.width)
            assertSelectionExtents(paragraph.selectionRects(selection, align: 0, in: bounds, dirty: .infinite),
                expected, origin: origin, message: "paragraph: \(text), \(selection)")
            let source = RegionTextSource.capture(spec, engine: engine)
            let done = expectation(description: "reader geometry")
            DispatchQueue.global().async {
                var paints: [(RegionRaster, Data, String)] = []
                for compact in [false, true] {
                    let layout = RegionWorkerLayout.shape(source, width: bounds.width, compact: compact)
                    assertSelectionExtents(layout.selectionRects(selection, in: bounds), expected,
                        origin: origin, message: "worker (compact \(compact)): \(text), \(selection)")
                    let p = layout.metadata
                    let captured = RegionParagraph(source: source, sourceSHA256: p.sourceSHA256,
                        lines: [layout.line(at: 0)], baselines: p.baselines, width: p.width, height: p.height,
                        lineBottoms: p.lineBottoms, offeredWidth: bounds.width, attributed: layout.preparation.attributed)
                    assertSelectionExtents(captured.selectionRects(selection, in: bounds, dirty: .infinite),
                        expected, origin: origin, message: "captured: \(text), \(selection)")
                    paints += readerSelectionPaint(layout, selection: selection, bounds: bounds,
                        expected: expected, origin: origin)
                }
                let completed = paints
                DispatchQueue.main.async {
                    for (raster, expectedPixels, message) in completed {
                        XCTAssertTrue(raster.image()?.dataProvider?.data as Data? == expectedPixels, message)
                    }
                    done.fulfill()
                }
            }
            wait(for: [done], timeout: 10)
        }
    }

    func testCSSDirectionSetsTheBaseEvenWhenTheFirstStrongCharacterDisagrees() {
        let engine = TextEngine(resolve: { _ in nil })
        for text in ["אבג abc", "العربية abc"] {
            let latin = (text as NSString).range(of: "abc").location
            var spec = Spec(runs: [Run(text: text, size: 20, weight: 400, family: 0,
                italic: false, lineHeight: nil, letterSpacing: 0)], align: 0, lineClamp: 0, color: [0, 0, 0, 255])
            let ltr = engine.paragraph(spec, width: 300)
            XCTAssertLessThan(CTLineGetOffsetForStringIndex(ltr.lines[0], 0, nil),
                CTLineGetOffsetForStringIndex(ltr.lines[0], latin, nil), text)
            spec.direction = 1
            let rtl = engine.paragraph(spec, width: 300)
            XCTAssertGreaterThan(CTLineGetOffsetForStringIndex(rtl.lines[0], 0, nil),
                CTLineGetOffsetForStringIndex(rtl.lines[0], latin, nil), text)
            spec.direction = 0
            XCTAssertEqual(engine.paragraph(spec, width: 300).width, ltr.width)
        }
    }

    func testSelectionPreservesCaretsInsideALigature() throws {
        let font = CTFontCreateWithName("HoeflerText-Regular" as CFString, 16, nil)
        let line = CTLineCreateWithAttributedString(NSAttributedString(string: "office",
            attributes: [.font: font, .ligature: 2]))
        guard CTLineGetGlyphCount(line) < 6 else { throw missing("fonts", "ligature font unavailable") }
        let part = try XCTUnwrap(TextEngine.selectionSpans(line, range: NSRange(location: 2, length: 1)).first)
        let whole = try XCTUnwrap(TextEngine.selectionSpans(line, range: NSRange(location: 1, length: 3)).first)
        XCTAssertGreaterThan(part.lowerBound, whole.lowerBound)
        XCTAssertLessThan(part.upperBound, whole.upperBound)
        XCTAssertEqual(part.lowerBound, CTLineGetOffsetForStringIndex(line, 2, nil))
        XCTAssertEqual(part.upperBound, CTLineGetOffsetForStringIndex(line, 3, nil))
        // A ligature that ends its run keeps its carets too.
        let off = CTLineCreateWithAttributedString(NSAttributedString(string: "off", attributes: [.font: font, .ligature: 2]))
        XCTAssertEqual(CTLineGetGlyphCount(off), 2)
        let f = try XCTUnwrap(TextEngine.selectionSpans(off, range: NSRange(location: 1, length: 1)).first)
        XCTAssertEqual(f.upperBound, CTLineGetOffsetForStringIndex(off, 2, nil))
        XCTAssertLessThan(f.upperBound, CGFloat(CTLineGetTypographicBounds(off, nil, nil, nil)))
    }

    func testSelectionSegmentsKeepLogicalEndpointsAndRunDirections() throws {
        let engine = TextEngine(resolve: { _ in nil })
        for (text, direction, selection, startsRTL, endsRTL) in [
            ("abc אבג def", 0, NSRange(location: 4, length: 3), true, true),
            ("אבג abc דהו", 1, NSRange(location: 1, length: 5), true, false),
            ("abc אבג def", 0, NSRange(location: 1, length: 5), false, true),
            ("abc אבג def\nאבג abc דהו", 0, NSRange(location: 5, length: 13), true, false),
        ] {
            let run = Run(text: text, size: 16, weight: 400, family: 0,
                italic: false, lineHeight: 26, letterSpacing: 0)
            var spec = Spec(runs: [run], align: 2, lineClamp: 0, color: [0, 0, 0, 255], whiteSpace: 1, strut: run)
            spec.direction = direction
            let paragraph = engine.paragraph(spec, width: 300)
            let bounds = CGRect(x: 17, y: 23, width: 300, height: paragraph.height)
            let segments = paragraph.selectionSegments(selection, align: 2, in: bounds, dirty: .infinite)
            let starts = segments.filter { $0.span.containsStart }, ends = segments.filter { $0.span.containsEnd }
            XCTAssertEqual(starts.count, 1, text)
            XCTAssertEqual(ends.count, 1, text)
            let start = try XCTUnwrap(starts.first), end = try XCTUnwrap(ends.first)
            XCTAssertEqual(start.span.rtl, startsRTL, text)
            XCTAssertEqual(end.span.rtl, endsRTL, text)
            for (segment, index) in [(start, selection.location), (end, NSMaxRange(selection) - 1)] {
                let glyph = try XCTUnwrap(selectedGlyphExtents(paragraph.lines[segment.line], NSRange(location: index, length: 1)).first)
                let origin = bounds.minX + paragraph.origin(segment.line, align: 2, width: bounds.width)
                XCTAssertLessThanOrEqual(segment.rect.minX, origin + glyph.lowerBound + 0.001, text)
                XCTAssertGreaterThanOrEqual(segment.rect.maxX, origin + glyph.upperBound - 0.001, text)
            }
            if direction == 1 {
                XCTAssertGreaterThan(start.rect.minX, end.rect.maxX, "logical start is visually last")
            }
            if paragraph.lines.count > 1 {
                XCTAssertEqual(start.line, 0)
                XCTAssertEqual(end.line, 1)
                let dirty = CGRect(x: bounds.minX, y: end.rect.midY, width: bounds.width, height: 1)
                let visible = paragraph.selectionSegments(selection, align: 2, in: bounds, dirty: dirty)
                XCTAssertFalse(visible.contains { $0.span.containsStart })
                XCTAssertEqual(visible.filter { $0.span.containsEnd }.count, 1)
            }
        }
        // A selection that starts at an invisible mark starts at the glyph after it.
        let run = Run(text: "abc \u{200F}אבג", size: 16, weight: 400, family: 0, italic: false, lineHeight: nil, letterSpacing: 0)
        let paragraph = engine.paragraph(Spec(runs: [run], align: 0, lineClamp: 0, color: [0, 0, 0, 255], strut: run), width: 300)
        let segments = paragraph.selectionSegments(NSRange(location: 4, length: 2), align: 0, in: CGRect(x: 0, y: 0, width: 300, height: 40), dirty: .infinite)
        let segment = try XCTUnwrap(segments.first)
        XCTAssertEqual(segments.count, 1)
        XCTAssertTrue(segment.span.containsStart && segment.span.containsEnd && segment.span.rtl)
        let glyph = try XCTUnwrap(selectedGlyphExtents(paragraph.lines[0], NSRange(location: 5, length: 1)).first)
        XCTAssertEqual(segment.rect.maxX, paragraph.origin(0, align: 0, width: 300) + glyph.upperBound, accuracy: 0.001)
    }

    func testMixedDirectionSelectionsKeepLineOriginsAndDirtyBounds() {
        let run = Run(text: "abc אבג def\nאבג abc דהו", size: 16, weight: 400, family: 0,
            italic: false, lineHeight: 26, letterSpacing: 0)
        let spec = Spec(runs: [run], align: 2, lineClamp: 0, color: [0, 0, 0, 255], whiteSpace: 1, strut: run)
        let paragraph = TextEngine(resolve: { _ in nil }).paragraph(spec, width: 300)
        XCTAssertEqual(paragraph.lines.count, 2)
        let bounds = CGRect(x: 17, y: 23, width: 300, height: paragraph.height)
        let selection = NSRange(location: 5, length: 13)
        let rects = paragraph.selectedLines(selection, align: 2, in: bounds, dirty: .infinite)
        for i in paragraph.lines.indices {
            let line = paragraph.lines[i]
            let expected = selectedGlyphExtents(line, selection)
            let origin = bounds.minX + paragraph.origin(i, align: 2, width: bounds.width)
            assertSelectionExtents(rects.filter { $0.line == i }.map(\.rect), expected,
                origin: origin, message: "line \(i)")
        }
        let first = rects.first!.rect
        let dirty = CGRect(x: bounds.minX, y: first.midY, width: bounds.width, height: 1)
        XCTAssertEqual(paragraph.selectedLines(selection, align: 2, in: bounds, dirty: dirty).map(\.line),
            rects.filter { $0.line == 0 }.map(\.line))
        XCTAssertTrue(paragraph.selectionRects(NSRange(location: 0, length: 0), align: 2,
            in: bounds, dirty: .infinite).isEmpty)
    }

    func testBaseDirectionPreservesOtherParagraphAttributesAndDoesNotOverrideRuns() {
        let paragraph = NSMutableParagraphStyle()
        paragraph.alignment = .center
        paragraph.firstLineHeadIndent = 12
        let string = NSMutableAttributedString(string: "אבג abc", attributes: [.paragraphStyle: paragraph])
        TextEngine.setBaseDirection(string, direction: 0)
        let style = string.attribute(.paragraphStyle, at: 0, effectiveRange: nil) as? NSParagraphStyle
        XCTAssertEqual(style?.baseWritingDirection, .leftToRight)
        XCTAssertEqual(style?.alignment, .center)
        XCTAssertEqual(style?.firstLineHeadIndent, 12)
        XCTAssertNil(string.attribute(.writingDirection, at: 0, effectiveRange: nil))
    }
}

// Fixture ranges include whole glyph clusters. Read their painted advances
// independently of the caret positions used to construct selection rectangles.
private func selectedGlyphExtents(_ line: CTLine, _ selection: NSRange) -> [ClosedRange<CGFloat>] {
    var pieces: [ClosedRange<CGFloat>] = []
    for run in CTLineGetGlyphRuns(line) as! [CTRun] {
        let count = CTRunGetGlyphCount(run)
        var indices = [CFIndex](repeating: 0, count: count)
        var positions = [CGPoint](repeating: .zero, count: count)
        var advances = [CGSize](repeating: .zero, count: count)
        CTRunGetStringIndices(run, CFRange(location: 0, length: 0), &indices)
        CTRunGetPositions(run, CFRange(location: 0, length: 0), &positions)
        CTRunGetAdvances(run, CFRange(location: 0, length: 0), &advances)
        for i in 0..<count where NSLocationInRange(indices[i], selection) {
            let a = positions[i].x, b = a + advances[i].width
            pieces.append(min(a, b)...max(a, b))
        }
    }
    var merged: [ClosedRange<CGFloat>] = []
    for piece in pieces.sorted(by: { $0.lowerBound < $1.lowerBound }) {
        if let last = merged.last, piece.lowerBound <= last.upperBound + 0.001 {
            merged[merged.count - 1] = last.lowerBound...max(last.upperBound, piece.upperBound)
        } else { merged.append(piece) }
    }
    return merged
}

private func assertSelectionExtents(_ rects: [CGRect], _ expected: [ClosedRange<CGFloat>],
                                    origin: CGFloat, message: String, file: StaticString = #filePath, line: UInt = #line) {
    XCTAssertEqual(rects.count, expected.count, message, file: file, line: line)
    for (rect, span) in zip(rects.sorted(by: { $0.minX < $1.minX }), expected) {
        XCTAssertEqual(rect.minX - origin, span.lowerBound, accuracy: 0.001, message, file: file, line: line)
        XCTAssertEqual(rect.maxX - origin, span.upperBound, accuracy: 0.001, message, file: file, line: line)
    }
}

private func readerSelectionPaint(_ layout: RegionWorkerLayout, selection: NSRange, bounds: CGRect,
                                 expected: [ClosedRange<CGFloat>], origin: CGFloat) -> [(RegionRaster, Data, String)] {
    var result: [(RegionRaster, Data, String)] = []
    do {
        let space = CGColorSpace(name: CGColorSpace.sRGB)!
        let profile = NativeProfile.capture(original: space, data: space.copyICCData(), account: NativeProfileAccount()).owner!
        let line = layout.line(at: 0)
        let y = TextEngine.paintedLine(layout.baselines[0], top: bounds.minY, scale: 1)
        var ascent: CGFloat = 0, descent: CGFloat = 0
        _ = CTLineGetTypographicBounds(line, &ascent, &descent, nil)
        for recolor in [false, true] {
            let fill: [CGFloat] = [0.2, 0.4, 0.8, 1]
            let selectedInk: [CGFloat] = [1, 1, 1, 1]
            let request = RegionRasterRequest(serial: 1, publication: 1, generation: 1,
                rows: [RegionPaintRow(artifact: 7, box: bounds, selection: selection)],
                scroll: .zero, size: CGSize(width: 340, height: 80), scale: 1,
                profile: profile, format: CGImageAlphaInfo.premultipliedLast.rawValue,
                background: [1, 1, 1, 1], selectionColor: recolor ? fill + selectedInk : fill)
            let index = try RegionPaintIndex(request: request, lookup: { $0 == 7 ? layout : nil }, account: InkAccount())
            let raster = try index.render(request, account: RegionPixelAccount(), hits: RegionHitAccount())
            let ctx = CGContext(data: nil, width: 340, height: 80, bitsPerComponent: 8, bytesPerRow: 340 * 4,
                space: profile.makeSpace()!, bitmapInfo: request.format)!
            let device = CGColorSpaceCreateDeviceRGB()
            ctx.setFillColor(CGColor(colorSpace: device, components: request.background)!)
            ctx.fill(CGRect(x: 0, y: 0, width: 340, height: 80))
            ctx.translateBy(x: 0, y: 80); ctx.scaleBy(x: 1, y: -1)
            if recolor { TextLinePaint.draw(line, at: CGPoint(x: origin, y: y), in: ctx) }
            for span in expected {
                let rect = CGRect(x: origin + span.lowerBound, y: y - ascent,
                    width: span.upperBound - span.lowerBound, height: ascent + descent)
                ctx.saveGState()
                if recolor { ctx.clip(to: rect) }
                ctx.setFillColor(CGColor(colorSpace: device, components: fill)!)
                ctx.fill(rect)
                if recolor {
                    TextLinePaint.draw(line, at: CGPoint(x: origin, y: y), in: ctx,
                        ink: CGColor(colorSpace: device, components: selectedInk)!, keepsFixed: false)
                }
                ctx.restoreGState()
            }
            if !recolor { TextLinePaint.draw(line, at: CGPoint(x: origin, y: y), in: ctx) }
            ctx.flush()
            result.append((raster, Data(bytes: ctx.data!, count: 340 * 80 * 4),
                "reader paint: \(layout.source.text), \(selection), recolor \(recolor)"))
        }
    } catch { XCTFail("reader selection raster: \(error)") }
    return result
}
