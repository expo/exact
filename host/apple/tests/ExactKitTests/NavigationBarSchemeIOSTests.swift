#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

extension NavigationBarIOSTests {
    /// A drawn face is an image for one scheme: the scheme changing while
    /// it shows draws it again in the other ink, though the app sends no batch.
    func testADrawnBarItemIsDrawnAgainWhenTheSchemeChanges() throws {
        let session = try fixture("scheme-face", module: false, plan: "BAR_FACE_SCHEME")
        let nav = try XCTUnwrap(session.presenter.navigation.primaryNavigation)
        let navigationItem = try XCTUnwrap(nav.topViewController?.navigationItem)
        let title = { ((navigationItem.leftBarButtonItems ?? []) + (navigationItem.rightBarButtonItems ?? [])).first { $0.accessibilityIdentifier == "title-home" } }
        // The brightest red among the face's opaque pixels: near 0 in black ink, near 255 in white.
        let ink = { () -> UInt8? in
            guard let image = title()?.image?.cgImage else { return nil }
            var bytes = [UInt8](repeating: 0, count: image.width * image.height * 4)
            guard let context = CGContext(data: &bytes, width: image.width, height: image.height, bitsPerComponent: 8, bytesPerRow: image.width * 4,
                                          space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return nil }
            context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
            return stride(from: 0, to: bytes.count, by: 4).filter { bytes[$0 + 3] > 200 }.map { bytes[$0] }.max()
        }
        let window = try XCTUnwrap(self.window)
        window.overrideUserInterfaceStyle = .light
        until("the title is drawn in the light ink") { ink().map { $0 < 60 } == true }
        window.overrideUserInterfaceStyle = .dark
        defer { window.overrideUserInterfaceStyle = .unspecified }
        until("the title is drawn again in the dark ink") { ink().map { $0 > 200 } == true }
    }
}
#endif
