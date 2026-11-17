import XCTest
import CoreText
import ImageIO
import IOSurface
import CExact
import ExactCore
@testable import ExactKit
@testable import ExactMarkdown
#if os(macOS)
import AppKit
#else
import UIKit
#endif

final class AtomReaderIconTests: XCTestCase {
    private var session: ExactSession!
    private var holding = false
    private let source = "Before [Ab](badge:done) after"
    private let kinds = #"[{"name":"badge","link":"badge:","iconSize":"12px","gap":"3px"}]"#
    private let svg = "data:image/svg+xml;base64," + Data(#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 4 4"><rect width="4" height="4" fill="red"/></svg>"#.utf8).base64EncodedString()

    private let asymmetricSVG = "data:image/svg+xml;base64," + Data(#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 4 4"><path d="M0 0h4v2H0z" fill="red"/><path d="M0 2h4v2H0z" fill="blue"/></svg>"#.utf8).base64EncodedString()

    override func setUp() {
        super.setUp()
        ExactMarkdown.install(); AtomIcons.reset()
        session = ExactApp.shared.makeSession(label: "reader-icons")
    }
    override func tearDown() {
        if holding { AtomIcons.queue.resume(); holding = false }
        session.destroy(); session = nil
        AtomIcons.reset(); super.tearDown()
    }
    private func reader(_ icon: String, rtl: Bool = false, pinned: String = "", background: String = "#ffffff", color: String = "#000000", fontSize: Int = 16, fit: String = "1", atomKinds: String? = nil, decoration: String = "") throws -> NodeView {
        #if os(macOS)
        session.text.readerParagraphs[1] = nil
        #endif
        let faces = String(data: try JSONSerialization.data(withJSONObject: [["source": "[Ab](badge:done)", "label": "Ab", "icon": icon,
            "color": color, "backgroundColor": background, "direction": pinned, "ariaLabel": "Completed books"]]), encoding: .utf8)!
        session.presenter.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "text", "props": ["text": source, "markup": "markdown", "atoms": atomKinds ?? kinds, "atomFaces": faces, "fitScale": fit],
             "style": ["font_size": fontSize, "text_decoration_line": decoration, "text_color": [0, 0, 0, 255], "direction": rtl ? "rtl" : "ltr", "user_select": "text"]],
            ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 400, "h": 60],
        ]))
        return try XCTUnwrap(session.presenter.views[1])
    }
    private func width(_ node: NodeView) throws -> CGFloat {
        CGFloat(CTLineGetTypographicBounds(try XCTUnwrap(node.paragraphLayout()?.lines.first), nil, nil, nil))
    }
    private func picture(_ line: CTLine, selected: Bool = false, keepsFixed: Bool = false) throws -> [UInt8] {
        let context = try XCTUnwrap(CGContext(data: nil, width: 400, height: 40, bitsPerComponent: 8, bytesPerRow: 1600,
                                              space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        context.translateBy(x: 0, y: 40); context.scaleBy(x: 1, y: -1)
        if selected { TextLinePaint.draw(line, at: CGPoint(x: 0, y: 24), in: context, ink: CGColor(red: 0, green: 0, blue: 1, alpha: 1), keepsFixed: keepsFixed) }
        else { TextLinePaint.draw(line, at: CGPoint(x: 0, y: 24), in: context) }
        return Array(UnsafeBufferPointer(start: context.data!.assumingMemoryBound(to: UInt8.self), count: 64000))
    }
    private func red(_ pixels: [UInt8]) -> Int {
        stride(from: 0, to: pixels.count, by: 4).filter { pixels[$0] > 200 && pixels[$0 + 1] < 40 && pixels[$0 + 2] < 40 && pixels[$0 + 3] > 200 }.count
    }
    private func iconRun(_ line: CTLine) throws -> (CTRun, TextAtomIcon) {
        for run in CTLineGetGlyphRuns(line) as! [CTRun] {
            if let icon = (CTRunGetAttributes(run) as NSDictionary)[NSAttributedString.Key.exactAtomIcon] as? TextAtomIcon { return (run, icon) }
        }
        throw NSError(domain: "missing reader icon", code: 1)
    }

    private func sample(_ pixels: [UInt8], _ line: CTLine, u: CGFloat, v: CGFloat) throws -> [UInt8] {
        let (run, icon) = try iconRun(line)
        var point = CGPoint.zero
        CTRunGetPositions(run, CFRange(location: 0, length: 1), &point)
        let box = icon.frame(at: point.x)
        let x = Int(box.minX + box.width * u), y = Int(24 - box.maxY + box.height * v)
        let at = y * 1600 + x * 4
        return Array(pixels[at..<at + 4])
    }

    func testBreakWordAfterAnIconMatchesMeasurement() throws {
        let node = try reader("symbol:sf/circle.fill")
        node.props["text"] = "[Ab](badge:done) extraordinary"
        node.style["overflow_wrap"] = .string("break-word")
        #if !os(iOS)
        node.style["hyphens"] = .string("auto")
        #endif
        node.invalidateText()
        var text = node.props["text"]!, faces = node.props["atomFaces"]!, declarations = kinds
        text.withUTF8 { t in faces.withUTF8 { f in declarations.withUTF8 { k in
            var run = ExactTextRun()
            run.text = t.baseAddress; run.len = t.count; run.font_size = 16; run.font_weight = 400; run.font_stretch = 100
            let strut = run
            withUnsafePointer(to: &run) { pointer in
                var request = ExactMeasureRequest()
                request.runs = pointer; request.count = 1; request.strut = strut
                request.height = -1; request.markup = 1; request.overflow_wrap = 1
                #if !os(iOS)
                request.hyphens = 2
                #endif
                request.atoms = k.baseAddress; request.atoms_len = k.count
                request.faces = f.baseAddress; request.faces_len = f.count
                let measured = session.text.requestSpec(request), painted = node.paragraphSpec()
                XCTAssertFalse(measured.source.inserted.isEmpty)
                XCTAssertEqual(painted.source.inserted, measured.source.inserted)
                XCTAssertEqual(painted.emergencyBreaks, measured.emergencyBreaks)
                for width in stride(from: 90, through: 180, by: 5) {
                    let a = session.text.paragraph(measured, width: CGFloat(width))
                    let b = session.text.paragraph(painted, width: CGFloat(width))
                    XCTAssertEqual(b.lines.map { CTLineGetStringRange($0).length },
                                   a.lines.map { CTLineGetStringRange($0).length }, "width \(width)")
                    request.width = Float(width)
                    XCTAssertEqual(b.height, CGFloat(session.text.measure(request).height), accuracy: 0.1)
                }
            }
        } } }
    }

    func testManyLoadingBadgesDoNotRemeasureOnSuccessAndCoalesceFailure() throws {
        XCTAssertNil(session.boot(plan: try fixturePlan("HEADLESS_MARKDOWN"), size: CGSize(width: 400, height: 600)).error)
        let engine = session.text.measuring
        let before = Owner.shared.sync { engine.measureCount }
        session.apply(session.runtime.on { session.runtime.read(exact_text_scale_changed(session.runtime.rt)) })
        let oneRemeasure = Owner.shared.sync { engine.measureCount } - before
        XCTAssertGreaterThan(oneRemeasure, 0)
        for source in [svg, "data:image/svg+xml,%3Csvg"] {
            AtomIcons.queue.suspend(); holding = true
            let node = try reader(source)
            node.props["text"] = Array(repeating: "[Ab](badge:done)", count: 1000).joined(separator: " ")
            node.invalidateText()
            _ = node.paragraphSpec()
            // Rebuilding while the decode is pending must keep one subscription.
            node.invalidateText()
            _ = node.paragraphSpec()
            let other = NodeView(id: 10001, kind: "text", presenter: session.presenter)
            other.props = node.props; other.style = node.style
            _ = other.paragraphSpec()
            XCTAssertEqual(AtomIcons.pendingCount(source, pixels: 36), 2)
            let start = Owner.shared.sync { engine.measureCount }
            AtomIcons.queue.resume(); holding = false
            waitFor("all badge callbacks delivered") { node.cachedTextSpec == nil && other.cachedTextSpec == nil }
            waitForMainQueue()
            let measures = Owner.shared.sync { engine.measureCount } - start
            let failed = AtomIcons.failed(source, pixels: 36)
            XCTAssertEqual(measures, failed ? oneRemeasure : 0)
            print("reader icon badges: 1000 per node, 2 nodes, failed: \(failed), measurements: \(measures), one remeasure: \(oneRemeasure)")
        }
    }

    func testFittedIconsMatchMeasurementAtTheFittedFontSize() throws {
        for metrics in [#""iconSize":"1em","gap":"0.2em""#, #""iconSize":"12px","gap":"3px""#, ""] {
            let kinds = "[{\"name\":\"badge\",\"link\":\"badge:\"" + (metrics.isEmpty ? "" : "," + metrics) + "}]"
            let normal = try reader("symbol:sf/circle.fill", atomKinds: kinds)
            var text = source, faces = normal.props["atomFaces"]!, declarations = kinds
            let measured = text.withUTF8 { t in faces.withUTF8 { f in declarations.withUTF8 { k in
                var run = ExactTextRun()
                run.text = t.baseAddress; run.len = t.count; run.font_size = 16; run.font_weight = 400; run.font_stretch = 100
                let strut = run
                return withUnsafePointer(to: &run) { pointer in
                    var request = ExactMeasureRequest()
                    request.runs = pointer; request.count = 1; request.strut = strut
                    request.width = -1; request.height = -1; request.markup = 1
                    request.atoms = k.baseAddress; request.atoms_len = k.count
                    request.faces = f.baseAddress; request.faces_len = f.count
                    return session.text.measure(request)
                }
            } } }
            let expected = CGFloat(measured.width)
            let normalIcon = try iconRun(XCTUnwrap(normal.paragraphLayout()).lines[0]).1
            let fitted = try reader("symbol:sf/circle.fill", fontSize: 32, fit: "0.5", atomKinds: kinds)
            XCTAssertEqual(try width(fitted), expected, accuracy: 0.1, metrics)
            let icon = try iconRun(XCTUnwrap(fitted.paragraphLayout()).lines[0]).1
            XCTAssertEqual(icon.side, normalIcon.side, accuracy: 0.01, metrics)
            XCTAssertEqual(icon.gap, normalIcon.gap, accuracy: 0.01, metrics)
            XCTAssertEqual(icon.capHeight, normalIcon.capHeight, accuracy: 0.01, metrics)
        }
    }

    func testHeadlessIconsResolveAfterFitAndRebuildUntilLoaded() throws {
        AtomIcons.queue.suspend(); holding = true
        let headless = try HeadlessSession(plan: fixturePlan("HEADLESS_MARKDOWN"), size: CGSize(width: 400, height: 600),
            facts: SessionFacts(displayScale: 1, preferences: [], rootFontSize: 16), assets: session.app.resolver, presentation: PresentedText())
        defer { headless.close() }
        let trigger = try XCTUnwrap(headless.snapshot().nodes.values.first { $0.props["testId"] == "show-icons" })
        let snapshot = try headless.press(trigger.id)
        for name in ["fitted-symbol", "fitted-image", "failed-image"] {
            let saved = try XCTUnwrap(snapshot.nodes.values.first { $0.props["testId"] == name })
            XCTAssertEqual(saved.props["fitScale"].flatMap(Double.init), 0.5)
            let faces = try XCTUnwrap(saved.props["atomFaces"]?.data(using: .utf8))
            let face = try XCTUnwrap((JSONSerialization.jsonObject(with: faces) as? [[String: Any]])?.first)
            let source = try XCTUnwrap(face["icon"] as? String)
            let initial = try XCTUnwrap(saved.lines.first).width
            if !source.hasPrefix("symbol:") {
                XCTAssertFalse(saved.hasComputedLines, "a pending icon must not freeze the first build")
                XCTAssertGreaterThan(AtomIcons.pendingCount(source, pixels: Int((PlatformFont.systemFont(ofSize: 16).capHeight * 3).rounded(.up))), 0,
                                     "headless layout starts the decode without a node")
            } else {
                let live = try reader(source, fontSize: 32, fit: "0.5", atomKinds: saved.props["atoms"])
                XCTAssertEqual(initial, try width(live), accuracy: 0.1, "headless and live use the fitted icon advance")
            }
        }
        AtomIcons.queue.resume(); holding = false
        for name in ["fitted-image", "failed-image"] {
            let saved = try XCTUnwrap(snapshot.nodes.values.first { $0.props["testId"] == name })
            let faces = try XCTUnwrap(saved.props["atomFaces"]?.data(using: .utf8))
            let face = try XCTUnwrap((JSONSerialization.jsonObject(with: faces) as? [[String: Any]])?.first)
            let source = try XCTUnwrap(face["icon"] as? String)
            let pixels = Int((PlatformFont.systemFont(ofSize: 16).capHeight * 3).rounded(.up))
            guard AtomIcons.pendingCount(source, pixels: pixels) > 0 || AtomIcons.cached(source, pixels: pixels) != nil || AtomIcons.failed(source, pixels: pixels) else { continue }
            waitFor("headless image settled") { AtomIcons.cached(source, pixels: pixels) != nil || AtomIcons.failed(source, pixels: pixels) }
            let live = try reader(source, fontSize: 32, fit: "0.5", atomKinds: saved.props["atoms"])
            XCTAssertEqual(try XCTUnwrap(saved.lines.first).width, try width(live), accuracy: 0.1, name)
            XCTAssertTrue(saved.hasComputedLines)
            XCTAssertEqual(AtomIcons.failed(source, pixels: pixels), name == "failed-image")
        }
    }

    func testCollapsedAtomFramesUseTheWholePillAtBothEnds() throws {
        let atom = (source as NSString).range(of: "[Ab](badge:done)")
        for rtl in [false, true] {
            for pin in ["", "ltr", "rtl"] {
                let node = try reader("symbol:sf/circle.fill", rtl: rtl, pinned: pin)
                let box = try XCTUnwrap(node.readerAtomFrame(atom.location, NSMaxRange(atom)))
                let start = try XCTUnwrap(node.readerAtomFrame(atom.location, atom.location))
                let end = try XCTUnwrap(node.readerAtomFrame(NSMaxRange(atom), NSMaxRange(atom)))
                XCTAssertEqual(start.minX, rtl ? box.maxX : box.minX, accuracy: 0.1, "rtl=\(rtl), pin=\(pin)")
                XCTAssertEqual(end.minX, rtl ? box.minX : box.maxX, accuracy: 0.1, "rtl=\(rtl), pin=\(pin)")
                XCTAssertEqual(start.width, 0); XCTAssertEqual(end.width, 0)
            }
        }
    }

    func testWrappedAtomCaretsUseTheFirstAndLastLine() throws {
        for wrap in ["break-word", "anywhere"] {
            let node = try reader("symbol:sf/circle.fill")
            let atom = "[Extraordinary](badge:done)"
            node.props["text"] = atom
            node.props["atomFaces"] = node.props["atomFaces"]?.replacingOccurrences(of: "[Ab](badge:done)", with: atom)
                .replacingOccurrences(of: "\"Ab\"", with: "\"Extraordinary\"")
            node.style["overflow_wrap"] = .string(wrap)
            node.style["hyphens"] = .string("auto")
            session.presenter.apply(wireBatch([["op": "frame", "id": 1, "x": 0, "y": 0, "w": 60, "h": 200]]))
            node.invalidateText()
            let paragraph = try XCTUnwrap(node.paragraphLayout())
            XCTAssertGreaterThan(paragraph.lines.count, 1, wrap)
            let spec = node.paragraphSpec()
            var at = 0, fragments: [CGRect] = []
            for run in spec.runs {
                let end = at + run.text.utf16.count
                fragments += node.shapedRects(at, end, empty: false, font: session.text.font(run) as CTFont)
                at = end
            }
            let first = try XCTUnwrap(fragments.map(\.minY).min())
            let last = try XCTUnwrap(fragments.map(\.minY).max())
            XCTAssertGreaterThan(last, first)
            for (offset, y, leading) in [(0, first, true), (atom.utf16.count, last, false)] {
                let edge = fragments.filter { $0.minY == y }.reduce(CGRect.null) { $0.union($1) }
                let caret = try XCTUnwrap(node.readerAtomFrame(offset, offset))
                XCTAssertEqual(caret.minY, edge.minY, accuracy: 0.1, wrap)
                XCTAssertEqual(caret.height, edge.height, accuracy: 0.1, wrap)
                XCTAssertEqual(caret.minX, leading ? edge.minX : edge.maxX, accuracy: 0.1, wrap)
                XCTAssertEqual(caret.width, 0)
            }
        }
    }

    func testPerRunShadowIncludesTheIconOnce() throws {
        let node = try reader("symbol:sf/circle.fill", background: "transparent", color: "#00800080")
        let source = NSMutableAttributedString(attributedString: session.text.attributed(node.paragraphSpec()))
        let at = (source.string as NSString).range(of: "\u{FFFC}")
        source.addAttribute(.exactShadow, value: TextRunShadow([0, 16, 0, 255, 0, 0, 255]), range: at)
        let line = CTLineCreateWithAttributedString(source)
        let pixels = try picture(line)
        let (run, icon) = try iconRun(line)
        var position = CGPoint.zero
        CTRunGetPositions(run, CFRange(location: 0, length: 1), &position)
        let box = icon.frame(at: position.x)
        let x = Int(box.midX), y = Int(24 - box.midY)
        let shadow = (y + 16) * 1600 + x * 4
        XCTAssertGreaterThan(pixels[shadow], 100, "the icon casts the run's red shadow")
        XCTAssertGreaterThan(pixels[shadow + 3], 100)
        XCTAssertEqual(Int(pixels[y * 1600 + x * 4 + 3]), 128, accuracy: 2, "translucent icon paints once")
    }

    func testTextRasterPreservesTheImageIconsDisplayP3Red() throws {
        let p3 = try XCTUnwrap(CGColorSpace(name: CGColorSpace.displayP3))
        let context = try XCTUnwrap(CGContext(data: nil, width: 4, height: 4, bitsPerComponent: 8, bytesPerRow: 16,
            space: p3, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        context.setFillColor(try XCTUnwrap(CGColor(colorSpace: p3, components: [1, 0, 0, 1])))
        context.fill(CGRect(x: 0, y: 0, width: 4, height: 4))
        let bytes = NSMutableData()
        let destination = try XCTUnwrap(CGImageDestinationCreateWithData(bytes, "public.png" as CFString, 1, nil))
        CGImageDestinationAddImage(destination, try XCTUnwrap(context.makeImage()), nil)
        XCTAssertTrue(CGImageDestinationFinalize(destination))
        let node = try reader("data:image/png;base64," + (bytes as Data).base64EncodedString(), background: "transparent")
        _ = node.paragraphLayout()
        waitFor("P3 icon arrives") { node.cachedTextSpec == nil }
        let paragraph = try XCTUnwrap(node.paragraphLayout())
        let (run, icon) = try iconRun(paragraph.lines[0])
        XCTAssertEqual(icon.image?.colorSpace?.name, p3.name)
        let box = CGRect(x: 0, y: 0, width: 400, height: 40)
        let raster = try XCTUnwrap(TextRasterJob(source: session.text.attributed(node.paragraphSpec()),
            ranges: paragraph.lines.map { CTLineGetStringRange($0) }, baselines: paragraph.baselines,
            flush: 0, box: box, size: box.size, scale: 1).render())
        #if os(macOS)
        let space = IOSurfaceCopyValue(raster.surface, kIOSurfaceColorSpace).flatMap { CGColorSpace(propertyListPlist: $0) }
        let width = raster.surface.width
        XCTAssertEqual(raster.surface.bytesPerElement, 4)
        #else
        let image = try XCTUnwrap(raster.pixels)
        let space = image.colorSpace, width = image.width
        XCTAssertEqual(image.bitsPerComponent, 8)
        #endif
        XCTAssertEqual(space?.name, p3.name, "the icon's profile participates in raster format selection")
        var position = CGPoint.zero
        CTRunGetPositions(run, CFRange(location: 0, length: 1), &position)
        let iconBox = icon.frame(at: position.x)
        let x = Int(iconBox.midX - raster.frame.minX)
        let y = Int(TextEngine.paintedLine(paragraph.baselines[0], top: 0, scale: 1, lineScale: 1) - iconBox.midY - raster.frame.minY)
        let pixels = CapturePixels.bytes(raster), offset = (y * width + x) * 4
        let components = [CGFloat(pixels[offset + 2]), CGFloat(pixels[offset + 1]), CGFloat(pixels[offset]), CGFloat(pixels[offset + 3])].map { $0 / 255 }
        let actual = try XCTUnwrap(CGColor(colorSpace: XCTUnwrap(space), components: components)?.converted(to: p3, intent: .relativeColorimetric, options: nil)?.components)
        for (actual, expected) in zip(actual, [CGFloat(1), 0, 0, 1]) { XCTAssertEqual(actual, expected, accuracy: 0.02) }
    }

    func testSelectedSymbolKeepsFixedInkAndTransparentInk() throws {
        for color in ["#ff0000", "transparent"] {
            let node = try reader("symbol:sf/circle.fill", background: "transparent", color: color)
            let main = try XCTUnwrap(node.paragraphLayout()).lines[0]
            let region = CTLineCreateWithAttributedString(RegionTextSource.capture(node.paragraphSpec(), engine: session.text).attributed())
            for line in [main, region] {
                let own = try sample(picture(line), line, u: 0.5, v: 0.5)
                let selected = try sample(picture(line, selected: true, keepsFixed: true), line, u: 0.5, v: 0.5)
                XCTAssertEqual(selected, own, color)
                if color == "transparent" {
                    XCTAssertEqual(try sample(picture(line, selected: true), line, u: 0.5, v: 0.5)[3], 0)
                } else {
                    XCTAssertGreaterThan(own[0], 200)
                }
            }
        }
    }

    func testSymbolHasClearCornersAndAnOpaqueCentre() throws {
        let node = try reader("symbol:sf/circle.fill", background: "transparent")
        let line = try XCTUnwrap(node.paragraphLayout()).lines[0]
        let pixels = try picture(line)
        XCTAssertEqual(try sample(pixels, line, u: 0.05, v: 0.05)[3], 0)
        XCTAssertGreaterThan(try sample(pixels, line, u: 0.5, v: 0.5)[3], 240)
        var centreInk = 0, clearCorners = 0
        for u in [CGFloat(0.4), 0.5, 0.6] {
            for v in [CGFloat(0.4), 0.5, 0.6] {
                if try sample(pixels, line, u: u, v: v)[3] > 240 { centreInk += 1 }
            }
        }
        for u in [CGFloat(0.05), 0.95] {
            for v in [CGFloat(0.05), 0.95] {
                if try sample(pixels, line, u: u, v: v)[3] == 0 { clearCorners += 1 }
            }
        }
        XCTAssertEqual(centreInk, 9); XCTAssertEqual(clearCorners, 4)
        #if os(iOS) || os(tvOS)
        print("reader circle.fill: centre ink \(centreInk)/9, clear corners \(clearCorners)/4, UIImage.cgImage present: \(AtomIcons.symbol("symbol:sf/circle.fill", size: 12)?.cgImage != nil)")
        #else
        print("reader circle.fill: centre ink \(centreInk)/9, clear corners \(clearCorners)/4")
        #endif
    }

    func testAsymmetricSVGAndRasterAreUpright() throws {
        let context = try XCTUnwrap(CGContext(data: nil, width: 4, height: 4, bitsPerComponent: 8, bytesPerRow: 16,
            space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        // The PNG has red upper rows and blue lower rows.
        context.setFillColor(CGColor(red: 1, green: 0, blue: 0, alpha: 1))
        context.fill(CGRect(x: 0, y: 2, width: 4, height: 2))
        context.setFillColor(CGColor(red: 0, green: 0, blue: 1, alpha: 1))
        context.fill(CGRect(x: 0, y: 0, width: 4, height: 2))
        let bytes = NSMutableData()
        let destination = try XCTUnwrap(CGImageDestinationCreateWithData(bytes, "public.png" as CFString, 1, nil))
        CGImageDestinationAddImage(destination, try XCTUnwrap(context.makeImage()), nil)
        XCTAssertTrue(CGImageDestinationFinalize(destination))
        for source in [asymmetricSVG, "data:image/png;base64," + (bytes as Data).base64EncodedString()] {
            let node = try reader(source, background: "transparent")
            _ = node.paragraphLayout()
            waitFor("asymmetric icon arrives") { node.cachedTextSpec == nil }
            let spec = node.paragraphSpec()
            for line in [try XCTUnwrap(node.paragraphLayout()).lines[0], CTLineCreateWithAttributedString(RegionTextSource.capture(spec, engine: session.text).attributed())] {
                for pixels in [try picture(line), try picture(line, selected: true)] {
                    XCTAssertGreaterThan(try sample(pixels, line, u: 0.5, v: 0.2)[0], 200, "red top")
                    XCTAssertGreaterThan(try sample(pixels, line, u: 0.5, v: 0.8)[2], 200, "blue bottom")
                }
            }
        }
    }

    func testRegionReaderDoesNotDecorateTheIcon() throws {
        let node = try reader("symbol:sf/circle.fill", background: "transparent", decoration: "underline line-through")
        let spec = node.paragraphSpec()
        let captured = RegionTextSource.capture(spec, engine: session.text).attributed()
        let at = (captured.string as NSString).range(of: "\u{FFFC}").location
        XCTAssertNil(captured.attribute(.underlineStyle, at: at, effectiveRange: nil))
        XCTAssertNil(captured.attribute(.strikethroughStyle, at: at, effectiveRange: nil))
        let main = try XCTUnwrap(node.paragraphLayout()).lines[0]
        XCTAssertTrue(try picture(CTLineCreateWithAttributedString(captured)) == picture(main), "region decorations match the main painter")
    }

    func testLongReaderRegionRasterKeepsIconOrientationAndDecorationGap() throws {
        let node = try reader(asymmetricSVG, background: "transparent", decoration: "underline line-through")
        _ = node.paragraphLayout()
        waitFor("region icon arrives") { node.cachedTextSpec == nil }
        var spec = node.paragraphSpec()
        spec.runs[spec.runs.count - 1].text += "\n"
        spec.runs = Array(repeating: spec.runs, count: 2500).flatMap { $0 }
        let source = RegionTextSource.capture(spec, engine: session.text)
        XCTAssertGreaterThan(source.sourceUTF8Bytes, 65536)
        let height = Int(ceil(try XCTUnwrap(node.paragraphLayout()).height))
        let space = try XCTUnwrap(CGColorSpace(name: CGColorSpace.sRGB))
        let profile = try XCTUnwrap(NativeProfile.capture(original: space, data: space.copyICCData(), account: NativeProfileAccount()).owner)
        let request = RegionRasterRequest(serial: 1, publication: 1, generation: 1,
            rows: [RegionPaintRow(artifact: 1, box: CGRect(x: 0, y: 0, width: 400, height: 100000))],
            scroll: .zero, size: CGSize(width: 400, height: height), scale: 1, profile: profile,
            format: CGImageAlphaInfo.premultipliedLast.rawValue, background: [0, 0, 0, 0], selectionColor: [0, 0, 1, 1])
        let done = expectation(description: "region reader raster")
        DispatchQueue.global().async {
            do {
                let layout = RegionWorkerLayout.shape(source, width: 400, compact: true)
                XCTAssertTrue(layout.lines.isEmpty)
                let index = try RegionPaintIndex(request: request, lookup: { $0 == 1 ? layout : nil }, account: InkAccount())
                let raster = try index.render(request, account: RegionPixelAccount(), hits: RegionHitAccount())
                let (run, icon) = try self.iconRun(layout.line(at: 0))
                var position = CGPoint.zero
                CTRunGetPositions(run, CFRange(location: 0, length: 1), &position)
                let box = icon.frame(at: position.x), baseline = source.painted(layout.baselines[0])
                let gapX = Int(position.x + icon.side + icon.gap / 2)
                DispatchQueue.main.async {
                    defer { done.fulfill() }
                    guard let image = raster.image(), let data = image.dataProvider?.data else { return XCTFail("region has no pixels") }
                    let pixels = data as Data
                    let x = Int(box.midX), top = Int(baseline - box.maxY + box.height * 0.2)
                    let bottom = Int(baseline - box.maxY + box.height * 0.8)
                    XCTAssertGreaterThan(pixels[top * image.bytesPerRow + x * 4], 200, "red top")
                    XCTAssertGreaterThan(pixels[bottom * image.bytesPerRow + x * 4 + 2], 200, "blue bottom")
                    XCTAssertTrue((0..<height).allSatisfy { pixels[$0 * image.bytesPerRow + gapX * 4 + 3] == 0 },
                                  "underline and strike leave the icon's gap clear")
                }
            } catch { XCTFail("\(error)"); done.fulfill() }
        }
        wait(for: [done], timeout: 10)
    }

    func testSymbolReservesItsSizeAndGapWithoutGrowingTheLineAndPaintsInItsInk() throws {
        let plain = try reader("")
        let plainWidth = try width(plain), height = try XCTUnwrap(plain.paragraphLayout()).height
        let node = try reader("symbol:sf/circle.fill")
        let paragraph = try XCTUnwrap(node.paragraphLayout()), line = try XCTUnwrap(paragraph.lines.first)
        XCTAssertEqual(try width(node) - plainWidth, 15, accuracy: 0.1)
        XCTAssertEqual(paragraph.height, height)
        let (run, icon) = try iconRun(line)
        XCTAssertNotNil(icon.image)
        XCTAssertEqual(CTRunGetTypographicBounds(run, CFRange(), nil, nil, nil), 15, accuracy: 0.1)
        var point = CGPoint.zero
        CTRunGetPositions(run, CFRange(location: 0, length: 1), &point)
        let box = icon.frame(at: point.x)
        let pixels = try picture(line)
        let black = (Int(box.minX.rounded(.up))..<Int(box.maxX)).contains { x in
            (Int(24 - box.maxY)..<Int(24 - box.minY)).contains { y in
                let at = y * 1600 + x * 4
                return pixels[at] < 30 && pixels[at + 1] < 30 && pixels[at + 2] < 30 && pixels[at + 3] > 200
            }
        }
        XCTAssertTrue(black, "the symbol paints inside its reserved box")
        let invalid = try reader("symbol:sf/not.a.real.symbol")
        XCTAssertEqual(try width(invalid), plainWidth, accuracy: 0.1)
    }

    func testImageArrivalKeepsTheLabelAndFailureRemovesItsRoom() throws {
        AtomIcons.queue.suspend(); holding = true
        let node = try reader(svg)
        let waitingWidth = try width(node), waiting = try XCTUnwrap(node.paragraphLayout())
        XCTAssertEqual(red(try picture(waiting.lines[0])), 0)
        AtomIcons.queue.resume(); holding = false
        waitFor("reader invalidated on image arrival") { node.cachedTextSpec == nil }
        XCTAssertEqual(try width(node), waitingWidth, accuracy: 0.1)
        XCTAssertGreaterThan(red(try picture(try XCTUnwrap(node.paragraphLayout()).lines[0])), 100)
        let failed = try reader("data:image/svg+xml,%3Csvg")
        let pending = try width(failed)
        waitFor("reader invalidated on image failure") { failed.cachedTextSpec == nil }
        XCTAssertEqual(pending - (try width(failed)), 15, accuracy: 0.1)
        XCTAssertFalse(failed.paragraphSpec().runs.contains { $0.atomIcon != nil })
    }

    func testRasterImageLoadsThroughTheSameReaderPath() throws {
        let bytes = NSMutableData()
        let context = try XCTUnwrap(CGContext(data: nil, width: 4, height: 4, bitsPerComponent: 8, bytesPerRow: 16,
                                              space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        context.setFillColor(CGColor(red: 1, green: 0, blue: 0, alpha: 1))
        context.fill(CGRect(x: 0, y: 0, width: 4, height: 4))
        let destination = try XCTUnwrap(CGImageDestinationCreateWithData(bytes, "public.png" as CFString, 1, nil))
        CGImageDestinationAddImage(destination, try XCTUnwrap(context.makeImage()), nil)
        XCTAssertTrue(CGImageDestinationFinalize(destination))
        let node = try reader("data:image/png;base64," + (bytes as Data).base64EncodedString())
        _ = node.paragraphLayout()
        waitFor("raster image arrives") { node.cachedTextSpec == nil }
        XCTAssertGreaterThan(red(try picture(try XCTUnwrap(node.paragraphLayout()).lines[0])), 100)
    }

    func testRTLPlacesTheIconOnTheRightAndPinnedLTROnTheLeft() throws {
        for pin in ["", "ltr"] {
            let node = try reader("symbol:sf/circle.fill", rtl: true, pinned: pin)
            let paragraph = try XCTUnwrap(node.paragraphLayout()), line = paragraph.lines[0]
            let (run, icon) = try iconRun(line)
            var point = CGPoint.zero
            CTRunGetPositions(run, CFRange(location: 0, length: 1), &point)
            let label = (node.paragraphSpec().runs.map(\.text).joined() as NSString).range(of: "Ab")
            // At a bidi boundary a caret has two positions. Compare the
            // label's actual glyph advances with the icon's actual picture.
            var left = CGFloat.infinity, right = -CGFloat.infinity
            for glyphRun in CTLineGetGlyphRuns(line) as! [CTRun] {
                let count = CTRunGetGlyphCount(glyphRun)
                var indices = [CFIndex](repeating: 0, count: count)
                var positions = [CGPoint](repeating: .zero, count: count)
                var advances = [CGSize](repeating: .zero, count: count)
                CTRunGetStringIndices(glyphRun, CFRange(), &indices)
                CTRunGetPositions(glyphRun, CFRange(), &positions)
                CTRunGetAdvances(glyphRun, CFRange(), &advances)
                for i in indices.indices where NSLocationInRange(indices[i], label) {
                    left = min(left, positions[i].x)
                    right = max(right, positions[i].x + advances[i].width)
                }
            }
            XCTAssertTrue(left.isFinite && right.isFinite)
            if pin.isEmpty { XCTAssertGreaterThanOrEqual(icon.frame(at: point.x).minX, right) }
            else { XCTAssertLessThanOrEqual(icon.frame(at: point.x).maxX, left) }
            // A line break cannot strand the icon without its label.
            let narrow = session.text.paragraph(node.paragraphSpec(), width: 60)
            for line in narrow.lines {
                if let (r, _) = try? iconRun(line) {
                    let range = CTLineGetStringRange(line), at = CTRunGetStringRange(r).location
                    XCTAssertLessThanOrEqual(range.location, at)
                    XCTAssertGreaterThanOrEqual(range.location + range.length, NSMaxRange(label))
                }
            }
        }
    }

    func testSelectionCopyAndAccessibilityContainOnlyTheLabel() throws {
        let node = try reader("symbol:sf/circle.fill")
        let shown = node.paragraphSpec().runs.map(\.text).joined()
        XCTAssertTrue(shown.contains("\u{FFFC}"), "exercise a real icon placeholder")
        let expected = shown.replacingOccurrences(of: "\u{FFFC}", with: "").replacingOccurrences(of: "\u{2060}", with: "")
        XCTAssertEqual(node.copiedText(shown, from: 0), expected)
        let placeholder = (shown as NSString).range(of: "\u{FFFC}")
        XCTAssertEqual(node.copiedText("\u{FFFC}", from: placeholder.location), "")
        XCTAssertEqual(node.accessibleText, "Before Completed books after")
        XCTAssertEqual(MarkdownLink.installed?.plain(source, range: nil, atoms: kinds, faces: node.props["atomFaces"]!, spoken: false), "Before Ab after")
        #if os(macOS)
        session.presenter.selection.selectAll()
        XCTAssertEqual(session.presenter.selection.selectedText(), expected)
        session.presenter.selection.copy()
        XCTAssertEqual(NSPasteboard.general.string(forType: .string), expected)
        #else
        session.presenter.textSelection.touched(node)
        let selection = try XCTUnwrap(session.presenter.textSelection.text(for: node))
        selection.selectedTextRange = SelectableRange(NSRange(location: 0, length: shown.utf16.count))
        let board = UIPasteboard.withUniqueName()
        defer { UIPasteboard.remove(withName: board.name) }
        session.presenter.textSelection.pasteboard = board
        XCTAssertEqual(selection.selectedText, expected)
        XCTAssertTrue(session.presenter.textSelection.copy(node))
        XCTAssertEqual(board.string, expected)
        #endif
    }

    func testBackgroundReaderCapturePaintsTheSameImageAndAdvance() throws {
        let node = try reader(svg, background: "transparent")
        _ = node.paragraphLayout()
        waitFor("image ready") { node.cachedTextSpec == nil }
        let spec = node.paragraphSpec(), line = try XCTUnwrap(node.paragraphLayout()?.lines.first)
        let source = RegionTextSource.capture(spec, engine: session.text)
        let paragraph = try XCTUnwrap(node.paragraphLayout())
        let box = CGRect(x: 0, y: 0, width: 400, height: 40)
        let job = TextRasterJob(source: session.text.attributed(spec), ranges: paragraph.lines.map { CTLineGetStringRange($0) },
                                baselines: paragraph.baselines, flush: 0, box: box, size: box.size, scale: 1)
        let done = expectation(description: "worker paints captured reader")
        let expected = try picture(line)
        DispatchQueue.global().async {
            defer { done.fulfill() }
            do {
                let line = CTLineCreateWithAttributedString(source.attributed())
                XCTAssertEqual(try self.picture(line), expected)
                let raster = try XCTUnwrap(job.render())
                let bytes = CapturePixels.bytes(raster)
                let red = stride(from: 0, to: bytes.count, by: 4).filter {
                    bytes[$0] < 40 && bytes[$0 + 1] < 40 && bytes[$0 + 2] > 200 && bytes[$0 + 3] > 200
                }.count
                XCTAssertGreaterThan(red, 100, "the background raster keeps the image's own colour")
            } catch { XCTFail("\(error)") }
        }
        wait(for: [done], timeout: 10)
    }

    private func png(_ space: CGColorSpace, deep: Bool = false, corners: Bool = false) throws -> String {
        let info = CGImageAlphaInfo.premultipliedLast.rawValue | (deep ? CGBitmapInfo.byteOrder16Little.rawValue : 0)
        let ctx = try XCTUnwrap(CGContext(data: nil, width: 8, height: 8, bitsPerComponent: deep ? 16 : 8,
            bytesPerRow: 0, space: space, bitmapInfo: info))
        let colors: [[CGFloat]] = corners ? [[1, 0, 0, 1], [0, 1, 0, 1], [0, 0, 1, 1], [1, 1, 0, 1]] : Array(repeating: [1, 0, 0, 1], count: 4)
        for (i, color) in colors.enumerated() {
            ctx.setFillColor(try XCTUnwrap(CGColor(colorSpace: space, components: color)))
            ctx.fill(CGRect(x: i % 2 * 4, y: i < 2 ? 4 : 0, width: 4, height: 4))
        }
        let bytes = NSMutableData()
        let destination = try XCTUnwrap(CGImageDestinationCreateWithData(bytes, "public.png" as CFString, 1, nil))
        CGImageDestinationAddImage(destination, try XCTUnwrap(ctx.makeImage()), nil)
        XCTAssertTrue(CGImageDestinationFinalize(destination))
        return "data:image/png;base64," + (bytes as Data).base64EncodedString()
    }

    private struct IconPixels {
        let bytes: [UInt8], stride: Int, box: CGRect
        func sample(_ u: CGFloat, _ v: CGFloat) -> [UInt8] {
            let at = Int(box.minY + v * box.height) * stride + Int(box.minX + u * box.width) * 4
            return Array(bytes[at..<at + 4])
        }
        // UIKit's symbol image and its CGImage include different clear margins.
        // Compare their ink bounds so the assertion measures the arrow's shape.
        var trimmedInk: IconPixels {
            var ink = CGRect.null
            for y in Int(box.minY)..<Int(box.maxY) { for x in Int(box.minX)..<Int(box.maxX) {
                if bytes[y * stride + x * 4 + 3] > 100 {
                    ink = ink.union(CGRect(x: x, y: y, width: 1, height: 1))
                }
            } }
            return IconPixels(bytes: bytes, stride: stride, box: ink.isNull ? box : ink)
        }
        var arrowWings: (top: Int, bottom: Int) {
            func count(_ low: CGFloat, _ high: CGFloat) -> Int {
                var count = 0
                for y in Int(box.minY + low * box.height)..<Int(box.minY + high * box.height) {
                    for x in Int(box.minX)..<Int(box.maxX) {
                        let u = (CGFloat(x) + 0.5 - box.minX) / box.width
                        if (u < 0.35 || u > 0.65) && bytes[y * stride + x * 4 + 3] > 100 { count += 1 }
                    }
                }
                return count
            }
            return (count(0.05, 0.55), count(0.55, 0.95))
        }
    }

    private func normalized(_ image: CGImage, box: CGRect, space: CGColorSpace? = nil) throws -> IconPixels {
        let ctx = try XCTUnwrap(CGContext(data: nil, width: image.width, height: image.height, bitsPerComponent: 8,
            bytesPerRow: 0, space: space ?? CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        ctx.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
        return IconPixels(bytes: Array(UnsafeBufferPointer(start: ctx.data!.assumingMemoryBound(to: UInt8.self),
            count: ctx.bytesPerRow * image.height)), stride: ctx.bytesPerRow, box: box)
    }

    private func editorPixels(_ source: String) throws -> IconPixels {
        let font = PlatformFont.systemFont(ofSize: 48)
        let atom = MarkupAtom(kind: "badge", label: "Ab", icon: source, title: "", role: "", href: "badge:done",
            ink: [0, 0, 0, 255], background: [0, 0, 0, 0], selected: false, dark: false, contrast: false)
        atom.iconSize = 36; atom.gap = 3; atom.padding = 0
        let pill = AtomAttachment(atom), height = ceil(font.ascender - font.descender)
        let bounds = CGRect(x: 0, y: 0, width: 200, height: height)
        let location = NSTextContentStorage().documentRange.location
        let image = try XCTUnwrap(pill.image(for: bounds, attributes: [.font: font], location: location, textContainer: nil))
        let ctx = try XCTUnwrap(CGContext(data: nil, width: 200, height: Int(height), bitsPerComponent: 8,
            bytesPerRow: 800, space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        #if os(macOS)
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(cgContext: ctx, flipped: false)
        image.draw(in: bounds)
        NSGraphicsContext.restoreGraphicsState()
        #else
        ctx.translateBy(x: 0, y: height); ctx.scaleBy(x: 1, y: -1)
        UIGraphicsPushContext(ctx); image.draw(in: bounds); UIGraphicsPopContext()
        #endif
        let size: CGSize
        if source.hasPrefix("symbol:") { size = try XCTUnwrap(AtomIcons.symbol(source, size: 36)).size }
        else { size = CGSize(width: 1, height: 1) }
        let scale = min(36 / size.width, 36 / size.height)
        let box = CGRect(x: (36 - size.width * scale) / 2,
            y: height + font.descender - font.capHeight / 2 - size.height * scale / 2,
            width: size.width * scale, height: size.height * scale)
        return IconPixels(bytes: Array(UnsafeBufferPointer(start: ctx.data!.assumingMemoryBound(to: UInt8.self), count: 800 * Int(height))), stride: 800, box: box)
    }

    private enum PaintPath: String { case main, raster, region }
    private func checkOrientation(_ path: PaintPath) throws {
        let png = try png(XCTUnwrap(CGColorSpace(name: CGColorSpace.sRGB)), corners: true)
        for (name, source) in [("SVG", asymmetricSVG), ("PNG", png), ("arrow.up", "symbol:sf/arrow.up")] {
            let node = try reader(source, background: "transparent", fontSize: 48,
                atomKinds: #"[{"name":"badge","link":"badge:","iconSize":"36px","gap":"3px"}]"#)
            node.props["text"] = "[Ab](badge:done)"
            node.invalidateText()
            _ = node.paragraphLayout()
            if !source.hasPrefix("symbol:") { waitFor("orientation icon loaded") { node.cachedTextSpec == nil } }
            let paragraph = try XCTUnwrap(node.paragraphLayout()), line = paragraph.lines[0]
            let (run, icon) = try iconRun(line)
            var position = CGPoint.zero
            CTRunGetPositions(run, CFRange(location: 0, length: 1), &position)
            let frame = icon.frame(at: position.x)
            let baseline = TextEngine.paintedLine(paragraph.baselines[0], top: 0, scale: 1, lineScale: paragraph.lineScale)
            var box = CGRect(x: frame.minX, y: baseline - frame.maxY, width: frame.width, height: frame.height)
            let actual: IconPixels
            switch path {
            case .main:
                let ctx = try XCTUnwrap(CGContext(data: nil, width: 400, height: 80, bitsPerComponent: 8, bytesPerRow: 1600,
                    space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
                ctx.translateBy(x: 0, y: 80); ctx.scaleBy(x: 1, y: -1)
                TextLinePaint.draw(line, at: CGPoint(x: 0, y: baseline), in: ctx)
                actual = try normalized(XCTUnwrap(ctx.makeImage()), box: box)
            case .raster:
                let bounds = CGRect(x: 0, y: 0, width: 400, height: 80)
                let job = TextRasterJob(source: session.text.attributed(node.paragraphSpec()),
                    ranges: paragraph.lines.map { CTLineGetStringRange($0) }, baselines: paragraph.baselines,
                    flush: 0, box: bounds, size: bounds.size, scale: 1, lineScale: paragraph.lineScale)
                let done = expectation(description: "text raster")
                nonisolated(unsafe) var result: TextRasterImage?
                DispatchQueue.global().async { result = job.render(); done.fulfill() }
                wait(for: [done], timeout: 10)
                let raster = try XCTUnwrap(result)
                box = box.offsetBy(dx: -raster.frame.minX, dy: -raster.frame.minY)
                let bytes = CapturePixels.bytes(raster)
                #if os(macOS)
                let width = raster.surface.width
                #else
                let width = try XCTUnwrap(raster.pixels).width
                #endif
                var rgba = Array(bytes)
                for i in stride(from: 0, to: rgba.count, by: 4) { rgba.swapAt(i, i + 2) }
                actual = IconPixels(bytes: rgba, stride: width * 4, box: box)
            case .region:
                #if os(macOS)
                let (reader, window) = try longReader(node)
                defer { window.close() }
                let raster = try XCTUnwrap(reader.raster), q = CGFloat(raster.request.scale)
                let first = try XCTUnwrap(reader.accepted).metadata.baselines[0]
                let y = TextEngine.paintedLine(first, top: 0, scale: q, lineScale: session.text.lineScale)
                box.origin.y = y - frame.maxY
                box = box.offsetBy(dx: -raster.request.scroll.x, dy: -raster.request.scroll.y)
                box = CGRect(x: box.minX * q, y: box.minY * q, width: box.width * q, height: box.height * q)
                actual = try normalized(XCTUnwrap(MainActor.assumeIsolated { raster.image() }), box: box)
                #else
                throw XCTSkip("the region reader is macOS-only")
                #endif
            }
            let editor = try editorPixels(source)
            if name == "arrow.up" {
                let a = actual.arrowWings, e = editor.arrowWings
                XCTAssertGreaterThan(a.top, a.bottom + 5, "\(path) arrow points up")
                XCTAssertGreaterThan(e.top, e.bottom + 5, "editor arrow points up")
                let readerInk = actual.trimmedInk, editorInk = editor.trimmedInk
                var matches = 0
                for y in 0..<12 { for x in 0..<12 {
                    let u = (CGFloat(x) + 0.5) / 12, v = (CGFloat(y) + 0.5) / 12
                    if (readerInk.sample(u, v)[3] > 100) == (editorInk.sample(u, v)[3] > 100) { matches += 1 }
                } }
                XCTAssertGreaterThan(matches, 120, "the reader's arrow mask matches the editor's")
                print("orientation \(path.rawValue) arrow.up reader wings top/bottom=\(a), editor=\(e), matching mask samples=\(matches)/144")
            } else {
                let points: [(CGFloat, CGFloat)] = name == "SVG" ? [(0.5, 0.2), (0.5, 0.8)] : [(0.2, 0.2), (0.8, 0.2), (0.2, 0.8), (0.8, 0.8)]
                let expected: [[UInt8]] = name == "SVG" ? [[255, 0, 0, 255], [0, 0, 255, 255]] : [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [255, 255, 0, 255]]
                for ((u, v), want) in zip(points, expected) {
                    let a = actual.sample(u, v), e = editor.sample(u, v)
                    for i in 0..<4 {
                        XCTAssertEqual(Int(a[i]), Int(want[i]), accuracy: 3, "\(path) \(name) \(u),\(v)")
                        XCTAssertEqual(Int(a[i]), Int(e[i]), accuracy: 3, "reader matches editor")
                    }
                    print("orientation \(path.rawValue) \(name) \(u),\(v) reader=\(a) editor=\(e)")
                }
            }
        }
    }

    func testMainPainterIconsAreUprightLikeTheEditor() throws { try checkOrientation(.main) }
    func testTextRasterIconsAreUprightLikeTheEditor() throws { try checkOrientation(.raster) }

    #if os(macOS)
    private func longReader(_ node: NodeView) throws -> (RegionReaderParagraph, NSWindow) {
        _ = NSApplication.shared
        node.props["text"] = node.props["text"]! + " " + String(repeating: "More words to read. ", count: 4000)
        node.invalidateText()
        var text = node.props["text"]!
        XCTAssertGreaterThan(text.utf8.count, 65536)
        text.withUTF8 { bytes in
            var run = ExactTextRun()
            run.text = bytes.baseAddress; run.len = bytes.count; run.font_size = Float(node.paragraphSpec().runs[0].size)
            run.font_weight = 400; run.font_stretch = 100
            let strut = run
            withUnsafePointer(to: &run) { pointer in
                var request = ExactMeasureRequest()
                request.view = node.id; request.runs = pointer; request.count = 1; request.strut = strut
                request.width = 400; request.height = -1
                XCTAssertNotNil(session.text.readerMeasure(request), "the real 64 KiB admission")
            }
        }
        let reader = try XCTUnwrap(node.readerParagraph)
        let window = NSWindow(contentRect: CGRect(x: 0, y: 0, width: 400, height: 100), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = session.presenter.viewport
        let scroll = NSScrollView(frame: CGRect(x: 0, y: 0, width: 400, height: 100))
        node.frame = CGRect(x: 0, y: 0, width: 400, height: 100000)
        scroll.documentView = node; session.presenter.root.addSubview(scroll)
        reader.update(node)
        waitFor("real long reader publishes pixels") { reader.raster != nil }
        XCTAssertNil(node.paragraphLayout(), "no ordinary paragraph fallback")
        XCTAssertTrue(reader.hasPixels)
        return (reader, window)
    }

    func testLongReaderReusesProfileAcrossTwoInvalidationsWithARasterHeld() throws {
        let node = try reader("symbol:sf/circle.fill")
        let (reader, window) = try longReader(node)
        defer { window.close() }
        let active = try XCTUnwrap(reader.raster)
        RegionTextExecutor.queue.isSuspended = true
        defer { RegionTextExecutor.queue.isSuspended = false }
        for _ in 0..<2 {
            node.atomIconReady(geometryChanged: false)
            reader.update(node)
        }
        XCTAssertTrue(reader.diagnostics["failure"] is NSNull, "two arrivals must not exhaust the profile account")
        RegionTextExecutor.queue.isSuspended = false
        if reader.diagnostics["failure"] is NSNull {
            waitFor("reader repaints while old raster is retained") { reader.hasPixels }
            XCTAssertTrue(reader.raster?.request.profile === active.request.profile)
        }
        withExtendedLifetime(active) {}
    }

    func testLongReaderRecoversAfterProfileCapacityIsReleased() throws {
        let p3 = try png(XCTUnwrap(CGColorSpace(name: CGColorSpace.displayP3)))
        let deep = try png(XCTUnwrap(CGColorSpace(name: CGColorSpace.displayP3)), deep: true)
        for source in [p3, deep] {
            let loaded = expectation(description: "profile icon loaded")
            _ = AtomIcons.image(source, pixels: 36, resolver: nil) { loaded.fulfill() }
            wait(for: [loaded], timeout: 10)
        }
        let node = try reader("symbol:sf/circle.fill")
        let (reader, window) = try longReader(node)
        defer { window.close() }
        var first: RegionRaster? = try XCTUnwrap(reader.raster)
        func change(_ source: String) throws {
            node.props["atomFaces"] = String(data: try JSONSerialization.data(withJSONObject: [["source": "[Ab](badge:done)", "label": "Ab", "icon": source]]), encoding: .utf8)!
            node.atomIconReady(geometryChanged: false)
            reader.update(node)
        }
        try change(p3)
        waitFor("P3 raster arrives beside the retained sRGB raster") { reader.raster != nil }
        let second = try XCTUnwrap(reader.raster?.request.profile)
        XCTAssertFalse(first?.request.profile === second)
        try change(deep)
        XCTAssertNil(reader.raster)
        XCTAssertTrue(reader.diagnostics["failure"] is NSNull, "capacity is temporary")
        XCTAssertTrue(reader.waitingForPixels, "retry on a later frame while both owners are retained")
        first = nil
        reader.update(node, afterFrame: true)
        if reader.diagnostics["failure"] is NSNull {
            waitFor("deep raster arrives after one profile owner is released") { reader.hasPixels }
            XCTAssertEqual(reader.raster?.request.bitsPerComponent, 16)
            XCTAssertEqual(reader.raster?.request.profile.makeSpace()?.name, CGColorSpace.extendedSRGB)
        }
        withExtendedLifetime(second) {}
    }

    func testMacRegionReaderIconsAreUprightLikeTheEditor() throws { try checkOrientation(.region) }

    func testLongReaderPreservesDisplayP3AndDeepIcons() throws {
        let p3 = try XCTUnwrap(CGColorSpace(name: CGColorSpace.displayP3))
        for deep in [false, true] {
            let node = try reader(png(p3, deep: deep), background: "transparent")
            _ = node.paragraphLayout()
            waitFor("P3 icon loaded") { node.cachedTextSpec == nil }
            let paragraph = try XCTUnwrap(node.paragraphLayout())
            let (run, icon) = try iconRun(paragraph.lines[0])
            XCTAssertEqual(icon.image?.bitsPerComponent, deep ? 16 : 8)
            var position = CGPoint.zero
            CTRunGetPositions(run, CFRange(location: 0, length: 1), &position)
            let box = icon.frame(at: position.x)
            let (reader, window) = try longReader(node)
            defer { window.close() }
            let raster = try XCTUnwrap(reader.raster), surface = try XCTUnwrap(raster.pixels.surface)
            let image = try XCTUnwrap(MainActor.assumeIsolated { raster.image() })
            XCTAssertEqual(image.bitsPerComponent, deep ? 16 : 8)
            XCTAssertEqual(surface.bytesPerElement, deep ? 8 : 4)
            let space = try XCTUnwrap(IOSurfaceCopyValue(surface, kIOSurfaceColorSpace).flatMap { CGColorSpace(propertyListPlist: $0) })
            XCTAssertEqual(space.name, deep ? CGColorSpace.extendedSRGB : CGColorSpace.displayP3)
            let q = CGFloat(raster.request.scale), baseline = try XCTUnwrap(reader.accepted).metadata.baselines[0]
            let sample = CGRect(x: (box.midX - raster.request.scroll.x) * q,
                y: (baseline - box.midY - raster.request.scroll.y) * q, width: 0, height: 0)
            let actual = try normalized(image, box: sample, space: p3).sample(0, 0)
            for (got, want) in zip(actual, [255, 0, 0, 255]) { XCTAssertEqual(Int(got), want, accuracy: 3) }
            XCTAssertTrue(node.layer?.sublayers?.contains { ($0.contents as AnyObject?) === surface } == true,
                          "the profiled surface reaches the presentation layer")
            print("long reader P3 deep=\(deep): RGBA=\(actual), bits=\(image.bitsPerComponent), bytes/pixel=\(surface.bytesPerElement)")
        }
    }
    #endif

}
