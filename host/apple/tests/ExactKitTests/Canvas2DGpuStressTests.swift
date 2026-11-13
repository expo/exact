import XCTest
import CoreGraphics
import IOSurface
import QuartzCore
@testable import ExactKit
@testable import ExactSurfaces

/// The Canvas 2D GPU module under concurrency (LLP 1056 §8.5): several
/// presenters' replay queues drive one process-wide module while the main
/// thread applies, forgets and shows. Run under Thread Sanitizer on a
/// simulator. Skipped without a module (`EXACT_CANVAS_GPU_DYLIB`).
final class Canvas2DGpuStressTests: XCTestCase {
    override class func setUp() { super.setUp(); ExactSurfaces.install() } // LLP 1047.001 D4
    private func list(_ records: [(Canvas2DOp, [Double])]) -> Data {
        var d = Data()
        func u32(_ v: UInt32) { withUnsafeBytes(of: v.littleEndian) { d.append(contentsOf: $0) } }
        func f64(_ v: Double) { withUnsafeBytes(of: v.bitPattern.littleEndian) { d.append(contentsOf: $0) } }
        u32(0x4432_4345); u32(1)
        for (op, n) in records { u32(op.rawValue); u32(UInt32(n.count)); n.forEach(f64) }
        return d
    }

    /// A draw: a full cover in `rgb`, then circles, a clip, a stroke and a
    /// save/restore pair — state that must persist into the next list.
    private func frame(_ rgb: (Double, Double, Double), _ k: Int) -> Data {
        var r: [(Canvas2DOp, [Double])] = [(.fillColor, [rgb.0, rgb.1, rgb.2, 1]), (.fillRect, [0, 0, 60, 40]), (.save, [])]
        r += [(.beginPath, []), (.moveTo, [2, 2]), (.lineTo, [58, 2]), (.lineTo, [58, 30]), (.closePath, []), (.clip, [0])]
        for i in 0..<20 {
            let x = Double((i * 7 + k) % 60), y = Double((i * 5 + k) % 40)
            r += [(.fillColor, [255, Double(i * 12 % 255), 0, 0.8]), (.beginPath, []), (.moveTo, [x, y]),
                  (.cubicTo, [x + 3, y, x + 3, y + 3, x, y + 3]), (.closePath, []), (.fill, [0])]
        }
        r += [(.restore, []), (.strokeColor, [0, 0, 0, 1]), (.lineWidth, [1.5]), (.strokeRect, [4, 4, 20, 10])]
        // Leaves a corner pixel at the cover colour: (59, 39) is outside every shape.
        return list(r)
    }

    private func payload(_ lifetime: UInt64, generation: UInt32, fresh: Bool, lists: [Data]) -> [String: Any] {
        ["lifetime": lifetime, "generation": generation, "fresh": fresh, "w": 180, "h": 120, "scale": 3.0,
         "lists": lists, "seq": 1, "animating": true]
    }

    private func corner(_ surface: IOSurface) -> [UInt8] {
        surface.lock(options: .readOnly, seed: nil)
        defer { surface.unlock(options: .readOnly, seed: nil) }
        let row = surface.bytesPerRow, base = surface.baseAddress.assumingMemoryBound(to: UInt8.self)
        let at = (surface.height - 1) * row + (surface.width - 1) * 4
        return [base[at + 2], base[at + 1], base[at], base[at + 3]] // BGRA → RGBA
    }

    /// A capture (LLP 1056 D9, as amended for expo/exact#188) of a canvas
    /// the module draws is its IOSurface's pixels, with the kept ones under
    /// the later list.
    func testACaptureIsTheSurfaceShown() throws {
        guard Canvas2DGpuModule.shared != nil else { throw XCTSkip("the Canvas 2D GPU module is not built") }
        let host = Canvas2DHost(), parent = CALayer()
        host.apply(7, payload(1, generation: 0, fresh: true, lists: [frame((20, 60, 200), 0)]), layer: parent)
        host.apply(7, payload(1, generation: 0, fresh: false, lists: [list([(.fillColor, [0, 200, 0, 1]), (.fillRect, [40, 0, 20, 10])])]), layer: parent)
        let png = try Canvas2DCaptureTests.capture(host, w: 180, h: 120, test: self)
        host.waitForReplays(timeout: 5)
        let surface = try XCTUnwrap(parent.sublayers?.first?.contents as? IOSurface, "the GPU draws it")
        let shown = try XCTUnwrap(canvas2DImage(of: surface, p3: false))
        let (a, b) = (Canvas2DCaptureTests.rgba(png, 180, 120), Canvas2DCaptureTests.rgba(shown, 180, 120))
        XCTAssertEqual(a, b, "the PNG is the surface shown")
        XCTAssertEqual(Array(a[(119 * 180 + 179) * 4..<(119 * 180 + 180) * 4]), [20, 60, 200, 255], "the first list's cover, kept")
        XCTAssertEqual(Array(a[(5 * 180 + 150) * 4..<(5 * 180 + 151) * 4]), [0, 200, 0, 255], "the second list over it")
        host.forget(7)
    }

    func testCurrentAndSavedFontsDrawInLaterJobs() throws {
        guard Canvas2DGpuModule.shared != nil else { throw XCTSkip("the Canvas 2D GPU module is not built") }
        let host = Canvas2DHost(), parent = CALayer(), engine = TextEngine(resolve: { _ in nil })
        host.textEngine = { engine }
        func font(_ size: Double) -> (Canvas2DOp, [Double]) {
            (.font, [size, 400, 0, 100, 0, 0, 0, 0, 0] + "system-ui".unicodeScalars.map { Double($0.value) })
        }
        let cover: [(Canvas2DOp, [Double])] = [(.fillColor, [255, 255, 255, 1]), (.fillRect, [0, 0, 60, 40])]
        let text: [(Canvas2DOp, [Double])] = [(.fillColor, [0, 0, 0, 1]), (.fillText, [2, 30, 1, 0, 72, 105])]
        func draw(_ records: [(Canvas2DOp, [Double])], fresh: Bool = false) throws -> Data {
            host.apply(7, payload(1, generation: 0, fresh: fresh, lists: [list(cover + records + text)]), layer: parent)
            waitFor("GPU text replay") { host.loadingCount == 0 }
            let surface = try XCTUnwrap(parent.sublayers?.first?.contents as? IOSurface)
            surface.lock(options: .readOnly, seed: nil)
            defer { surface.unlock(options: .readOnly, seed: nil) }
            var data = Data()
            for y in 0..<surface.height {
                data.append(Data(bytes: surface.baseAddress.advanced(by: y * surface.bytesPerRow), count: surface.width * 4))
            }
            XCTAssertTrue(data.contains(0), "black glyphs draw over the white canvas")
            return data
        }
        let large = try draw([font(20), (.save, [])], fresh: true)
        let small = try draw([font(12)])
        XCTAssertNotEqual(large, small)
        XCTAssertEqual(try draw([]), small, "the current font survives a job with no font record")
        XCTAssertEqual(try draw([(.restore, [])]), large, "a later job restores the saved font")
        host.forget(7)
    }

    func testPresentersReplayConcurrentlyThroughOneModule() throws {
        guard Canvas2DGpuModule.shared != nil else { throw missing("module", "no Canvas 2D GPU module (EXACT_CANVAS_GPU_DYLIB)") }
        let presenters = 4, canvases = 8, rounds = 120
        let hosts = (0..<presenters).map { _ in Canvas2DHost() }
        let parents = (0..<presenters).map { _ in CALayer() }
        var lifetimes = [[UInt64]](repeating: [UInt64](repeating: 1, count: canvases), count: presenters)
        var last = [[(Double, Double, Double)]](repeating: [(Double, Double, Double)](repeating: (0, 0, 0), count: canvases), count: presenters)
        for round in 0..<rounds {
            for p in 0..<presenters {
                for c in 0..<canvases {
                    let id = UInt32(c + 1)
                    // Every so often a canvas is retired and comes back fresh.
                    let retire = (round + c * 3 + p) % 37 == 0
                    if retire { hosts[p].forget(id); lifetimes[p][c] += 1 }
                    let fresh = round == 0 || retire
                    let rgb = (Double((round * 11 + c * 29) % 256), Double((p * 60 + c * 13) % 256), Double((round * 3) % 256))
                    last[p][c] = rgb
                    hosts[p].apply(id, payload(lifetimes[p][c], generation: 0, fresh: fresh, lists: [frame(rgb, round)]), layer: parents[p])
                }
            }
            if round % 3 == 0 { RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.001)) }
        }
        for h in hosts { h.waitForReplays(timeout: 30) }
        // Every canvas shows its last draw: the cover colour at the corner.
        for p in 0..<presenters {
            let layers = parents[p].sublayers ?? []
            XCTAssertEqual(layers.count, canvases, "presenter \(p): one layer a canvas")
            for layer in layers {
                guard let s = layer.contents.map({ $0 as AnyObject }) as? IOSurface else {
                    XCTFail("presenter \(p): a canvas layer does not show an IOSurface: \(String(describing: layer.contents))")
                    continue
                }
                let px = corner(s)
                let expected = last[p].map { [UInt8($0.0), UInt8($0.1), UInt8($0.2), 255] }
                XCTAssertTrue(expected.contains(px), "presenter \(p): corner \(px) is no canvas's last cover")
            }
        }
        XCTAssertLessThan(Canvas2DGpuModule.shared!.memory(), 256 << 20, "the module's memory after 32 small canvases")
    }
}
