#if os(macOS)
import AppKit
#else
import UIKit
#endif
import XCTest
import ExactCore
@testable import ExactKit

final class FieldShadowTests: XCTestCase {
    #if os(macOS)
    private var window: NSWindow!
    #else
    private var window: UIWindow!
    #endif
    private var session: ExactSession!

    override func setUp() {
        #if os(macOS)
        _ = NSApplication.shared
        #endif
    }

    override func tearDown() {
        #if os(macOS)
        window?.close()
        #else
        window?.isHidden = true
        #endif
        session?.destroy(); window = nil; session = nil
    }

    private func fixture() throws -> (NodeView, TextArea) {
        session = ExactApp.shared.makeSession(label: "field-shadow")
        let p = session.presenter
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view"],
            ["op": "create", "id": 2, "kind": "textarea", "props": ["value": "HHHH HHHH"],
             "style": ["font_size": 30, "text_color": [0, 0, 0, 255]]],
            ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 320, "h": 180],
            ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 300, "h": 140],
        ]))
        #if os(macOS)
        window = NSWindow(contentRect: CGRect(x: 0, y: 0, width: 320, height: 180), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.viewport
        #else
        let scene = try XCTUnwrap(UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.first)
        window = UIWindow(windowScene: scene)
        window.frame = scene.coordinateSpace.bounds
        let controller = UIViewController()
        window.rootViewController = controller
        controller.view.addSubview(p.viewport)
        window.makeKeyAndVisible()
        window.layoutIfNeeded()
        #endif
        let node = try XCTUnwrap(p.views[2]), field = try XCTUnwrap(node.textArea as? TextArea)
        return (node, field)
    }

    private func pixels(_ field: PlatformView) throws -> [UInt8] {
        let image: CGImage
        #if os(macOS)
        field.layoutSubtreeIfNeeded()
        let bitmap = try XCTUnwrap(field.bitmapImageRepForCachingDisplay(in: field.bounds))
        field.cacheDisplay(in: field.bounds, to: bitmap)
        image = try XCTUnwrap(bitmap.cgImage)
        #else
        field.setNeedsDisplay()
        window.layoutIfNeeded()
        let format = UIGraphicsImageRendererFormat(); format.scale = 1
        let bitmap = UIGraphicsImageRenderer(bounds: field.bounds, format: format).image { _ in
            XCTAssertTrue(field.drawHierarchy(in: field.bounds, afterScreenUpdates: true))
        }
        image = try XCTUnwrap(bitmap.cgImage)
        #endif
        let width = Int(field.bounds.width), height = Int(field.bounds.height)
        let context = try XCTUnwrap(CGContext(data: nil, width: width, height: height,
            bitsPerComponent: 8, bytesPerRow: width * 4, space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
        let data = try XCTUnwrap(context.data)
        return Array(UnsafeBufferPointer(start: data.assumingMemoryBound(to: UInt8.self), count: width * height * 4))
    }

    private func red(_ pixels: [UInt8]) -> Int {
        stride(from: 0, to: pixels.count, by: 4).filter {
            Int(pixels[$0]) > Int(pixels[$0 + 1]) + 20 && Int(pixels[$0]) > Int(pixels[$0 + 2]) + 20
        }.count
    }

    func testTextKitShadowRenderingAttributeWithColourAndStorageControls() throws {
        #if os(iOS)
        guard hostedWindowScene() != nil else { throw missing("scene", "TextKit shadows need a scene-backed capture") }
        #endif
        let (_, field) = try fixture()
        let manager = try XCTUnwrap(field.textLayoutManager), content = try XCTUnwrap(manager.textContentManager)
        manager.ensureLayout(for: content.documentRange)
        manager.addRenderingAttribute(.foregroundColor, value: PlatformColor.red, for: content.documentRange)
        XCTAssertGreaterThan(red(try pixels(field)), 40, "the capture sees rendering attributes")
        manager.setRenderingAttributes([:], for: content.documentRange)
        XCTAssertEqual(red(try pixels(field)), 0, "the previous foreground has left the capture")
        let shadow = NSShadow(); shadow.shadowColor = PlatformColor.red
        shadow.shadowOffset = CGSize(width: 40, height: 0); shadow.shadowBlurRadius = 2
        manager.addRenderingAttribute(.shadow, value: shadow, for: content.documentRange)
        XCTAssertEqual(red(try pixels(field)), 0, "TextKit ignores .shadow rendering attributes")
        manager.setRenderingAttributes([:], for: content.documentRange)
        let storage = try XCTUnwrap((content as? NSTextContentStorage)?.textStorage)
        storage.addAttribute(.shadow, value: shadow, range: NSRange(location: 0, length: storage.length))
        XCTAssertGreaterThan(red(try pixels(field)), 40, "the same shadow in storage is visible")
    }

    func testADecorationShadowLeavesStorageAndUndoUntouched() throws {
        #if os(iOS)
        guard hostedWindowScene() != nil else { throw missing("scene", "decoration shadows need a scene-backed capture") }
        #endif
        let (node, field) = try fixture()
        let before = try pixels(field)
        node.props["decorations"] = #"{"value":"HHHH HHHH","spans":[{"name":"shadow","start":0,"end":4}]}"#
        node.props["decorationStyles"] = ##"{"shadow":[[151,"8px 25px 2px #ff0000"]]}"##
        node.restyleDecorations()
        XCTAssertEqual(try pixels(field), before, "the unsupported shadow leaves the field unchanged")
        let storage = try XCTUnwrap((field.textLayoutManager?.textContentManager as? NSTextContentStorage)?.textStorage)
        XCTAssertNil(storage.attribute(.shadow, at: 0, effectiveRange: nil))
        XCTAssertNil(field.typingAttributes[.shadow])
        XCTAssertFalse(field.undoManager?.canUndo ?? false)
    }

}
