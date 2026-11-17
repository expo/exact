#if os(iOS)
import UIKit
import XCTest
import ImageIO
import Darwin
@testable import ExactKit

@_silgen_name("proc_pid_rusage")
private func rasterProfileRusage(_ pid: Int32, _ flavor: Int32, _ buffer: UnsafeMutableRawPointer) -> Int32

final class RasterProfileIOSTests: XCTestCase {
    func testAnEquivalentProfileCommitsWithoutAColourConversion() throws {
        let url = Bundle.module.resourceURL!.appendingPathComponent("ColorFixtures/shop-c2ci.jpg")
        let bytes = try Data(contentsOf: url)
        let metadata = try RasterMetadata.read(prefix: bytes, encodedBytes: bytes.count)
        let plan = try RasterDecodePlan(metadata: metadata, maxPixel: 840)
        let window = UIWindow(frame: UIScreen.main.bounds)
        window.rootViewController = UIViewController()
        window.makeKeyAndVisible()
        defer { window.isHidden = true }
        let host = try XCTUnwrap(window.rootViewController?.view.layer)
        window.layoutIfNeeded()
        CATransaction.flush()

        func picture() throws -> CGImage {
            let source = try XCTUnwrap(CGImageSourceCreateWithData(bytes as CFData, nil))
            return try XCTUnwrap(CGImageSourceCreateThumbnailAtIndex(source, 0, RasterImage.thumbnailOptions(plan)))
        }
        func instructions() throws -> UInt64 {
            var info = rusage_info_v4()
            let status = withUnsafeMutablePointer(to: &info) { rasterProfileRusage(getpid(), 4, $0) }
            XCTAssertEqual(status, 0)
            return info.ri_instructions
        }
        func commit(_ image: CGImage) throws -> UInt64 {
            let layer = CALayer()
            layer.frame = CGRect(x: 0, y: 0, width: 200, height: 300)
            layer.contentsGravity = .resizeAspectFill
            layer.masksToBounds = true
            CATransaction.flush()
            let start = try instructions()
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            layer.contents = image
            host.addSublayer(layer)
            CATransaction.commit()
            CATransaction.flush()
            let cost = try instructions() - start
            layer.removeFromSuperlayer()
            CATransaction.flush()
            return cost
        }
        // Warm the renderer and colour machinery; each measured image is fresh
        // so CA cannot reuse an earlier prepared backing.
        _ = try commit(picture())
        _ = try commit(XCTUnwrap(RasterImage.normalized(picture(), plan: plan)))
        var original: [UInt64] = [], named: [UInt64] = []
        for i in 0..<3 {
            let a = try picture(), b = try XCTUnwrap(RasterImage.normalized(picture(), plan: plan))
            XCTAssertEqual(a.colorSpace.map(colorSpaceName), "icc")
            XCTAssertEqual(b.colorSpace?.name, CGColorSpace.sRGB)
            if i.isMultiple(of: 2) { original.append(try commit(a)); named.append(try commit(b)) }
            else { named.append(try commit(b)); original.append(try commit(a)) }
        }
        let before = try XCTUnwrap(original.min()), after = try XCTUnwrap(named.min())
        print("Shop first commit instructions: ICC=\(original), named=\(named)")
        XCTContext.runActivity(named: "First commit instructions: ICC=\(original), named=\(named)") { _ in }
        XCTAssertGreaterThan(before, 20_000_000, "the real shop profile exercises CA's per-pixel conversion")
        XCTAssertLessThan(after, 500_000, "no per-pixel conversion or backing copy at commit")
        XCTAssertLessThan(after * 8, before)
    }
}
#endif
