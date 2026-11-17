#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit

final class LaunchColorIOSTests: XCTestCase {
    private func pixels(_ color: UIColor, traits: UITraitCollection, space: CFString) throws -> [UInt8] {
        var bytes = [UInt8](repeating: 0, count: 16)
        try bytes.withUnsafeMutableBytes { storage in
            let context = try XCTUnwrap(CGContext(data: storage.baseAddress, width: 2, height: 2,
                bitsPerComponent: 8, bytesPerRow: 8, space: try XCTUnwrap(CGColorSpace(name: space)),
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
            context.setFillColor(color.resolvedColor(with: traits).cgColor)
            context.fill(CGRect(x: 0, y: 0, width: 2, height: 2))
        }
        return bytes
    }

    private func colors(_ fixture: String) throws -> (UIColor, UIColor) {
        let directory = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_LAUNCH_COLOR_FIXTURES"])
        let bundle = try XCTUnwrap(Bundle(path: directory + "/" + fixture + ".bundle"))
        let launch = try XCTUnwrap(bundle.object(forInfoDictionaryKey: "UILaunchScreen") as? [String: String])
        let name = try XCTUnwrap(launch["UIColorName"])
        let named = try XCTUnwrap(UIColor(named: name, in: bundle, compatibleWith: nil))
        let direct = try XCTUnwrap(LaunchScreenColor.make(bundle.object(forInfoDictionaryKey: "ExactLaunchColor")))
        return (named, direct)
    }

    private func compare(_ fixture: String) throws {
        let (named, direct) = try colors(fixture)
        for style: UIUserInterfaceStyle in [.unspecified, .light, .dark] {
            for contrast: UIAccessibilityContrast in [.normal, .high] {
                for level: UIUserInterfaceLevel in [.base, .elevated] {
                    let traits = UITraitCollection(traitsFrom: [UITraitCollection(userInterfaceStyle: style),
                        UITraitCollection(accessibilityContrast: contrast), UITraitCollection(userInterfaceLevel: level)])
                    for space in [CGColorSpace.sRGB, CGColorSpace.displayP3] {
                        XCTAssertEqual(try pixels(direct, traits: traits, space: space),
                                       try pixels(named, traits: traits, space: space), "\(style), \(contrast), \(level), \(space)")
                    }
                }
            }
        }
    }

    func testWhiteIntegerPlistMatchesTheCatalogue() throws {
        try compare("white")
    }

    func testOpaquePlistMatchesTheCatalogueInLightAndDark() throws {
        try compare("opaque")
    }

    func testDirectComponentsMatchTheCatalogueInEverySupportedAppearance() throws {
        try compare("translucent")
        let (named, _) = try colors("translucent")
        let directory = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_LAUNCH_COLOR_FIXTURES"])
        let bundle = try XCTUnwrap(Bundle(path: directory + "/translucent.bundle"))
        let components = try XCTUnwrap(bundle.object(forInfoDictionaryKey: "ExactLaunchColor") as? [String: [NSNumber]])
        let light = try XCTUnwrap(LaunchScreenColor.make(["light": try XCTUnwrap(components["light"])]))
        XCTAssertEqual(try pixels(light, traits: UITraitCollection(userInterfaceStyle: .dark), space: CGColorSpace.sRGB),
                       try pixels(named, traits: UITraitCollection(userInterfaceStyle: .light), space: CGColorSpace.sRGB))
        XCTAssertNil(LaunchScreenColor.make(nil))
    }
}
#endif
