// A picture of a frame's guest into an `app:/` file (@ref LLP 1020.002 D7):
// WebDriver BiDi's `browsingContext.captureScreenshot`, answered by the
// frame's `capture` record instead of a base64 string.
import Foundation
import ImageIO
import UniformTypeIdentifiers
import WebKit

#if os(macOS)
import AppKit
#else
import UIKit
#endif

extension WebArm {
    /// `{op: "captureScreenshot", path, file, clip}`: `path` is the `app:/`
    /// path the command named, which the record carries back; `file` the
    /// file it names, empty when it names none; `clip` `[x, y, width,
    /// height]` in the guest's CSS pixels relative to its viewport, or none
    /// for the whole viewport. `done`, automation's (D9), takes the answer
    /// in place of the frame's record.
    func capture(_ op: [String: Any], done: ((Int, Int, String, String) -> Void)? = nil) {
        let path = op["path"] as? String ?? ""
        let answer: (Int, Int, String, String) -> Void = { [weak self] width, height, error, message in
            if !error.isEmpty { FileHandle.standardError.write(Data("exact: iframe \(self?.id ?? 0): captureScreenshot: \(message)\n".utf8)) }
            if let done { return done(width, height, error, message) }
            self?.emitRecord("capture", ["path": path, "width": width, "height": height, "error": error, "message": message])
        }
        guard let file = (op["file"] as? String).flatMap({ $0.isEmpty ? nil : URL(fileURLWithPath: $0) }) else {
            return answer(0, 0, "SyntaxError", "\(path) is not a file under app:/data, app:/cache or app:/tmp")
        }
        let clip = (op["clip"] as? [NSNumber]).flatMap { $0.count >= 4 ? $0.prefix(4).map(\.doubleValue) : nil }
        let write: (CGImage?, String) -> Void = { image, failure in
            guard let image else { return answer(0, 0, "UnknownError", failure) }
            FrameCaptures.encode.async {
                let written = FrameCaptures.write(image, to: file)
                DispatchQueue.main.async {
                    if let written { answer(0, 0, "UnknownError", "\(path): \(written)") } else { answer(image.width, image.height, "", "") }
                }
            }
        }
        if let pdf {
            // A PDF frame draws in-process, its clip in the view's points.
            let bounds = pdf.view.bounds
            let rect = clip.map { CGRect(x: $0[0], y: $0[1], width: $0[2], height: $0[3]) }?.intersection(bounds) ?? bounds
            guard bounds.width > 0, bounds.height > 0 else { return answer(0, 0, "InvalidStateError", "the frame has no box") }
            guard FrameCaptures.drawable(rect) else { return answer(0, 0, "IndexSizeError", "the clip is outside the frame's viewport") }
            return write(pdf.picture(rect), "the PDF view drew nothing")
        }
        let web = webView
        guard web.bounds.width > 0, web.bounds.height > 0 else { return answer(0, 0, "InvalidStateError", "the frame has no box") }
        guard let clip else { return snap(web, web.bounds, attempt: 0, write) }
        // A rect from `getBoundingClientRect()` is in client coordinates, the
        // document's less `scrollX` and `scrollY`; the web view shows the
        // visual viewport, at `pageLeft` and `pageTop` in the document and
        // scaled by a pinch. So the clip's offset is `pageLeft − scrollX`:
        // Chrome's `offsetLeft`, where `scrollX` is the layout viewport's, and
        // 0 in WebKit on macOS, whose `scrollX` and client coordinates follow
        // the visual viewport. `WKSnapshotConfiguration.rect` is in the view's
        // points, y down.
        web.evaluateJavaScript("[visualViewport.pageLeft - scrollX, visualViewport.pageTop - scrollY, visualViewport.scale]", in: nil, in: world) { [weak self, weak web] result in
            guard let self, let web, !self.invalidated, self.built === web else {
                return answer(0, 0, "NotFoundError", "the frame went away before its picture was taken")
            }
            let viewport = ((try? result.get()) as? [NSNumber])?.map(\.doubleValue) ?? []
            let (left, top, scale) = viewport.count == 3 ? (viewport[0], viewport[1], viewport[2]) : (0, 0, 1)
            let points = scale * Double(web.pageZoom)
            var rect = CGRect(x: (clip[0] - left) * points, y: (clip[1] - top) * points, width: clip[2] * points, height: clip[3] * points)
            #if os(iOS)
            rect.origin.y += web.scrollView.adjustedContentInset.top
            #endif
            rect = rect.intersection(web.bounds)
            guard FrameCaptures.drawable(rect) else { return answer(0, 0, "IndexSizeError", "the clip is outside the frame's viewport") }
            self.snap(web, rect, attempt: 0, write)
        }
    }

    /// `takeSnapshot` after the screen updates, three tries 120 ms apart, as
    /// the oracle and the clone try.
    func snap(_ web: WKWebView, _ rect: CGRect, attempt: Int, _ done: @escaping (CGImage?, String) -> Void) {
        FrameCaptures.rawSnapshot(web, rect: rect) { [weak self, weak web] image, error in
            if let image { return done(image, "") }
            guard attempt < 2, let self, let web, !self.invalidated, self.built === web else {
                return done(nil, error ?? "WebKit returned no snapshot")
            }
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.12) { self.snap(web, rect, attempt: attempt + 1, done) }
        }
    }

    /// Snapshot `web` for host compositing (`Agent.sample` and screenshots).
    /// On macOS, `WKWebView.takeSnapshot` fills pixels outside a scaled-down
    /// (`magnification < 1`) document with opaque white even when `drawsBackground`
    /// is false and `underPageBackgroundColor` is clear. For a transparent frame,
    /// verify the guest document's scrolling-area bounds before and after a single
    /// full-bounds capture and clear only the verified outside margin on the
    /// returned bitmap's pixel grid.
    func snapshotForHost(_ web: WKWebView, _ done: @escaping (CGImage?, String?) -> Void) {
        #if os(macOS)
        guard transparent, web.magnification < 1 else {
            return FrameCaptures.rawSnapshot(web, rect: web.bounds, done)
        }
        snapshotWithoutMargin(web, attempt: 0, done)
        #else
        FrameCaptures.rawSnapshot(web, rect: web.bounds, done)
        #endif
    }

    #if os(macOS)
    private func snapshotWithoutMargin(_ web: WKWebView, attempt: Int, _ done: @escaping (CGImage?, String?) -> Void) {
        guard !invalidated, built === web else {
            return done(nil, "the frame went away before its picture was taken")
        }
        measureDocument(web) { [weak self, weak web] before, measureError in
            guard let self, let web, !self.invalidated, self.built === web else {
                return done(nil, "the frame went away before its picture was taken")
            }
            guard let before else {
                return done(nil, measureError ?? "could not measure iframe document bounds")
            }
            FrameCaptures.rawSnapshot(web, rect: before.bounds) { [weak self, weak web] cg, snapError in
                guard let self, let web, !self.invalidated, self.built === web else {
                    return done(nil, "the frame went away before its picture was taken")
                }
                guard let cg else {
                    return done(nil, snapError ?? "WebKit returned no snapshot")
                }
                self.measureDocument(web) { [weak self, weak web] after, recheckError in
                    guard let self, let web, !self.invalidated, self.built === web else {
                        return done(nil, "the frame went away before its picture was taken")
                    }
                    guard let after else {
                        return done(nil, recheckError ?? "could not verify iframe document bounds after snapshot")
                    }
                    if after != before {
                        if attempt < 2 {
                            return self.snapshotWithoutMargin(web, attempt: attempt + 1, done)
                        }
                        return done(nil, "the iframe viewport moved during snapshot")
                    }
                    guard let cleared = FrameCaptures.clearOutsideMargin(cg, geometry: before) else {
                        return done(nil, "could not clear transparent iframe margin")
                    }
                    done(cleared, nil)
                }
            }
        }
    }

    private func measureDocument(_ web: WKWebView, _ done: @escaping (FrameCaptures.DocumentGeometry?, String?) -> Void) {
        web.evaluateJavaScript(FrameCaptures.geometryScript, in: nil, in: world) { [weak self, weak web] result in
            guard let self, let web, !self.invalidated, self.built === web else {
                return done(nil, "the frame went away before its picture was taken")
            }
            let raw: Any
            switch result {
            case .success(let value): raw = value
            case .failure(let error): return done(nil, "could not measure iframe document bounds: \(error.localizedDescription)")
            }
            guard let dict = raw as? [String: Any],
                  let docLeft = (dict["docLeft"] as? NSNumber)?.doubleValue,
                  let docTop = (dict["docTop"] as? NSNumber)?.doubleValue,
                  let docRight = (dict["docRight"] as? NSNumber)?.doubleValue,
                  let docBottom = (dict["docBottom"] as? NSNumber)?.doubleValue,
                  let pageLeft = (dict["pageLeft"] as? NSNumber)?.doubleValue,
                  let pageTop = (dict["pageTop"] as? NSNumber)?.doubleValue,
                  let href = dict["href"] as? String else {
                return done(nil, "iframe document bounds are unavailable")
            }
            let scale = Double(web.magnification * web.pageZoom)
            let bounds = web.bounds
            let finite = [docLeft, docTop, docRight, docBottom, pageLeft, pageTop, scale].allSatisfy(\.isFinite)
            guard finite, scale > 0, bounds.width > 0, bounds.height > 0,
                  docRight > docLeft, docBottom > docTop else {
                return done(nil, "iframe document bounds are empty or non-finite")
            }
            done(FrameCaptures.DocumentGeometry(
                docLeft: docLeft, docTop: docTop, docRight: docRight, docBottom: docBottom,
                pageLeft: pageLeft, pageTop: pageTop, scale: scale, bounds: bounds, href: href
            ), nil)
        }
    }
    #endif
}

enum FrameCaptures {
    /// Encoding and writing stay off the main thread.
    static let encode = DispatchQueue(label: "exact.web.capture", qos: .userInitiated)

    /// A rect a picture can be made of: at least a point each way.
    static func drawable(_ rect: CGRect) -> Bool { !rect.isNull && rect.width >= 1 && rect.height >= 1 }

    static func rawSnapshot(_ web: WKWebView, rect: CGRect, _ done: @escaping (CGImage?, String?) -> Void) {
        let configuration = WKSnapshotConfiguration()
        configuration.rect = rect
        configuration.afterScreenUpdates = true
        web.takeSnapshot(with: configuration) { image, error in
            done(image.flatMap(cgImage), error?.localizedDescription)
        }
    }

    static func cgImage(_ image: PlatformImage) -> CGImage? {
        #if os(macOS)
        image.cgImage(forProposedRect: nil, context: nil, hints: nil)
        #else
        image.cgImage
        #endif
    }

    static func png(_ image: CGImage) -> Data? {
        let data = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(data, UTType.png.identifier as CFString, 1, nil) else { return nil }
        CGImageDestinationAddImage(destination, image, nil)
        return CGImageDestinationFinalize(destination) ? data as Data : nil
    }

    #if os(macOS)
    struct DocumentGeometry: Equatable {
        let docLeft: Double
        let docTop: Double
        let docRight: Double
        let docBottom: Double
        let pageLeft: Double
        let pageTop: Double
        let scale: Double
        let bounds: CGRect
        let href: String
    }

    /// Reads the CSSOM View scrolling-area edges (`docLeft..docRight`, `docTop..docBottom`)
    /// in layout CSS pixels, accounting for quirks-mode `scrollingElement` and leftward or
    /// upward overflow in RTL and vertical writing modes.
    static let geometryScript = """
        (() => {
            const se = document.scrollingElement || document.documentElement || document.body;
            if (!se) return null;
            const styleSource = document.body || document.documentElement || se;
            const cs = getComputedStyle(styleSource);
            const wm = cs.writingMode || "";
            const vertical = wm.startsWith("vertical-") || wm.startsWith("sideways-");
            const isRTL = cs.direction === "rtl";
            const rtlX = (!vertical && isRTL) || wm === "vertical-rl" || wm === "sideways-rl";
            const rtlY = vertical && isRTL;
            const docLeft = rtlX ? Math.min(0, se.clientWidth - se.scrollWidth) : 0;
            const docTop = rtlY ? Math.min(0, se.clientHeight - se.scrollHeight) : 0;
            const vv = window.visualViewport;
            return {
                docLeft,
                docTop,
                docRight: docLeft + se.scrollWidth,
                docBottom: docTop + se.scrollHeight,
                pageLeft: vv ? vv.pageLeft : window.scrollX,
                pageTop: vv ? vv.pageTop : window.scrollY,
                href: String(location.href || "")
            };
        })()
        """

    /// Clears only the pixels strictly outside the document's painted region on
    /// `image`'s own backing-pixel grid, preserving pixel dimensions and color space.
    static func clearOutsideMargin(_ image: CGImage, geometry g: DocumentGeometry) -> CGImage? {
        let width = image.width, height = image.height
        guard width > 0, height > 0, g.bounds.width > 0, g.bounds.height > 0 else { return nil }
        let scaleX = Double(width) / Double(g.bounds.width)
        let scaleY = Double(height) / Double(g.bounds.height)
        // WebKit aligns the scaled document layer to integer view points (verified on macOS 27.2
        // at 1× and 2× Retina), filling opaque white only outside [floor(leftPt), ceil(rightPt))
        // × [floor(topPt), ceil(bottomPt)).
        let eps = 1e-6
        let minXPt = floor((g.docLeft - g.pageLeft) * g.scale + eps)
        let minYPt = floor((g.docTop - g.pageTop) * g.scale + eps)
        let maxXPt = ceil((g.docRight - g.pageLeft) * g.scale - eps)
        let maxYPt = ceil((g.docBottom - g.pageTop) * g.scale - eps)
        let pxLeft = max(0, min(width, Int(floor(minXPt * scaleX + eps))))
        let pxTop = max(0, min(height, Int(floor(minYPt * scaleY + eps))))
        let pxRight = max(0, min(width, Int(ceil(maxXPt * scaleX - eps))))
        let pxBottom = max(0, min(height, Int(ceil(maxYPt * scaleY - eps))))
        if pxLeft == 0, pxTop == 0, pxRight == width, pxBottom == height {
            return image
        }
        let srgb = CGColorSpace(name: CGColorSpace.sRGB) ?? CGColorSpaceCreateDeviceRGB()
        let space = image.colorSpace ?? srgb
        let info = CGImageAlphaInfo.premultipliedLast.rawValue
        let context = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4, space: space, bitmapInfo: info)
            ?? (space === srgb ? nil : CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4, space: srgb, bitmapInfo: info))
        guard let context else { return nil }
        if pxRight > pxLeft, pxBottom > pxTop {
            // `pxTop`/`pxBottom` are y-down in the snapshot's pixel grid; `CGContext` is y-up.
            let clip = CGRect(x: pxLeft, y: height - pxBottom, width: pxRight - pxLeft, height: pxBottom - pxTop)
            context.clip(to: clip)
            context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
        }
        return context.makeImage()
    }
    #endif

    /// The PNG at `file`, its folder made first; nil, or why it failed.
    static func write(_ image: CGImage, to file: URL) -> String? {
        guard let data = png(image) else { return "the PNG did not encode" }
        do {
            // A symbolic link to a directory under the app's roots is followed:
            // only the app can have made one there. `.atomic` renames over the
            // file, so a link in the file's place is replaced, not followed.
            try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
            try data.write(to: file, options: .atomic)
            return nil
        } catch {
            return error.localizedDescription
        }
    }
}
