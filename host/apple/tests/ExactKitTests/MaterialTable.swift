import XCTest
@testable import ExactKit

/// LLP 1053.000 D4: every row of the schema's `materials` table reaches this
/// platform as the name the table gives, and that name is one this platform
/// draws (a glass effect, or a `UIBlurEffect.Style` / `NSVisualEffectView.Material`).
enum MaterialTable {
    static func rows() throws -> [[String]] {
        let url = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .appendingPathComponent("../../../../kernel/tables/schema.json").standardized
        let schema = try JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any]
        return try XCTUnwrap(schema?["materials"] as? [[String]])
    }

    static func check(_ column: Int, draws: (String) -> Bool) throws {
        let rows = try rows()
        XCTAssertGreaterThan(rows.count, 30)
        for row in rows {
            let (apple, standIn) = try XCTUnwrap(Materials.platform(row[0]), row[0])
            XCTAssertEqual((standIn ? "~" : "") + apple, row[column], row[0])
            XCTAssertTrue(apple == "glass" || apple == "glassClear" || draws(apple), "\(row[0]): \(apple)")
        }
        XCTAssertNil(Materials.platform("not-a-material"))
        var logged: [String] = []
        XCTAssertEqual(Materials.resolve("not-a-material-\(column)") { logged.append($0) }, Materials.platform("ultra-thin")!.apple)
        XCTAssertEqual(Materials.resolve("not-a-material-\(column)") { logged.append($0) }, Materials.platform("ultra-thin")!.apple)
        XCTAssertEqual(logged.count, 1, "said once")
        // A table name drawn as a stand-in is silent; the agent's state shows it.
        let standIn = try XCTUnwrap(rows.first { $0[column].hasPrefix("~") }?[0])
        XCTAssertNotNil(Materials.resolve(standIn) { logged.append($0) })
        XCTAssertEqual(logged.count, 1, "a stand-in is not logged (Charlie: \"ok, (b)\")")
        XCTAssertEqual(Materials.agentMaterial(standIn)["standIn"] as? Bool, true)
        XCTAssertEqual(Materials.agentMaterial("not-a-material")["unknown"] as? Bool, true)
    }
}

#if os(macOS)
import AppKit

final class MaterialTableTests: XCTestCase {
    private func capture(_ window: NSWindow, marker ink: NSColor) throws -> NSBitmapImageRep {
        let content = try XCTUnwrap(window.contentView)
        let marker = PaintMarker(frame: NSRect(x: content.bounds.maxX - 12, y: content.bounds.midY - 4, width: 8, height: 8), ink: ink)
        content.addSubview(marker, positioned: .above, relativeTo: nil)
        defer { marker.removeFromSuperview() }
        content.layoutSubtreeIfNeeded()
        window.displayIfNeeded()
        CATransaction.flush()
        let expected = try XCTUnwrap(ink.usingColorSpace(.sRGB))
        let target = [expected.redComponent, expected.greenComponent, expected.blueComponent].map { Int(($0 * 255).rounded()) }
        let top = window.frame.height - window.contentLayoutRect.height
        var picture: NSBitmapImageRep?, seen: [Int]?, answered = false, failure = ""
        waitFor("the WindowServer to show the paint marker", late: 0.1, cap: 5,
                facts: { "marker \(seen.map { "\($0)" } ?? "not sampled") expected \(target)" },
                onFailure: { failure = $0 }) {
            guard let image = Agent.ownWindowImage(window.windowNumber) else { return false }
            answered = true
            let candidate = NSBitmapImageRep(cgImage: image)
            seen = try? colour(candidate, x: marker.frame.midX, y: top + content.bounds.midY, width: window.frame.width)
            // WindowServer colour management moves even an opaque sRGB marker in device space.
            let matchesMarker = seen.map { sampled in
                zip(sampled, target).allSatisfy { value, intended in
                    intended >= 128 ? value >= 200 : value <= 96
                }
            } ?? false
            if matchesMarker { picture = candidate; return true }
            return false
        }
        guard answered else {
            throw XCTSkip("the window server gave no picture (the screen is locked or the display is asleep)")
        }
        return try XCTUnwrap(picture, failure)
    }

    private func colour(_ rep: NSBitmapImageRep, x: CGFloat, y: CGFloat, width: CGFloat) throws -> [Int] {
        let scale = CGFloat(rep.pixelsWide) / width
        let c = try XCTUnwrap(rep.paintedColorAt(x: Int(x * scale), y: Int(y * scale))?.usingColorSpace(.sRGB))
        return [c.redComponent, c.greenComponent, c.blueComponent].map { Int(($0 * 255).rounded()) }
    }

    func testAnAuthoredSidebarMatchesAppKitsWindowBackdrop() throws {
        _ = NSApplication.shared
        let size = CGSize(width: 400, height: 300)
        let window = NSWindow(contentRect: NSRect(origin: .zero, size: size), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        defer { window.close() }

        let referenceRoot = NSView(frame: NSRect(origin: .zero, size: size))
        referenceRoot.wantsLayer = true
        referenceRoot.layer?.backgroundColor = NSColor.blue.cgColor
        let reference = NSVisualEffectView(frame: NSRect(x: 0, y: 0, width: 160, height: size.height))
        reference.material = .sidebar
        reference.blendingMode = .behindWindow
        reference.state = .followsWindowActiveState
        referenceRoot.addSubview(reference)
        window.contentView = referenceRoot
        window.orderFrontRegardless()
        let contentTop = window.frame.height - window.contentLayoutRect.height
        let expected = try colour(try capture(window, marker: NSColor(srgbRed: 0, green: 1, blue: 0, alpha: 1)), x: 80, y: contentTop + 150, width: size.width)

        let p = Presenter()
        p.viewport.frame = NSRect(origin: .zero, size: size)
        window.contentView = p.viewport
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "style": ["background_color": [0, 0, 255, 255]]],
            ["op": "create", "id": 2, "kind": "view", "props": ["backgroundMaterial": "sidebar"]],
            ["op": "create", "id": 3, "kind": "view", "style": ["background_color": [255, 0, 0, 255]]],
            ["op": "children", "id": 1, "ids": [2]], ["op": "children", "id": 2, "ids": [3]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 400, "h": 300],
            ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 160, "h": 300],
            ["op": "frame", "id": 3, "x": 20, "y": 20, "w": 40, "h": 40],
        ]))
        let picture = try capture(window, marker: NSColor(srgbRed: 1, green: 0, blue: 1, alpha: 1))
        let backdrop = try colour(picture, x: 80, y: contentTop + 150, width: size.width)
        let page = try colour(picture, x: 300, y: contentTop + 150, width: size.width)
        let content = try colour(picture, x: 40, y: contentTop + 40, width: size.width)
        for channel in 0 ..< 3 {
            XCTAssertEqual(backdrop[channel], expected[channel], accuracy: 2,
                           "the authored sidebar uses AppKit's desktop backdrop: \(backdrop) against \(expected)")
        }
        XCTAssertTrue(page[2] > 240 && page[0] < 16 && page[1] < 64, "the painted blue ancestor remains beside the material: \(page)")
        XCTAssertTrue(content[0] > 240 && content[1] < 64 && content[2] < 16, "authored red content remains above the material: \(content)")
    }

    func testAnAuthoredSidebarUsesTheWindowsBackdrop() throws {
        _ = NSApplication.shared
        let p = Presenter()
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 400, height: 300),
            styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = p.viewport
        defer { window.close() }
        p.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "style": ["background_color": [255, 0, 0, 255]]],
            ["op": "create", "id": 2, "kind": "view", "props": ["backgroundMaterial": "sidebar"]],
            ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
            ["op": "frame", "id": 1, "x": 0, "y": 0, "w": 400, "h": 300],
            ["op": "frame", "id": 2, "x": 0, "y": 0, "w": 160, "h": 300],
        ]))
        let owner = try XCTUnwrap(p.views[2])
        let sidebar = try XCTUnwrap(owner.materialView as? NSVisualEffectView)
        XCTAssertEqual(sidebar.material, .sidebar)
        XCTAssertEqual(sidebar.blendingMode, .behindWindow)
        XCTAssertEqual(sidebar.state, .followsWindowActiveState)
        XCTAssertEqual(p.views[1]?.color("background_color", .clear), .red, "authored ancestor paint remains")
        for name in ["ultra-thin", "popover", "sidebar"] {
            p.apply(wireBatch([["op": "props", "id": 2, "set": ["backgroundMaterial": name]]]))
            let effect = try XCTUnwrap(owner.materialView as? NSVisualEffectView)
            XCTAssertEqual(effect.blendingMode, name == "sidebar" ? .behindWindow : .withinWindow, name)
        }
    }

    func testEveryMaterialIsAnAppKitOne() throws {
        try MaterialTable.check(2) { Materials.material($0) != nil }
    }
}

private final class PaintMarker: NSView {
    let ink: NSColor
    init(frame: NSRect, ink: NSColor) { self.ink = ink; super.init(frame: frame) }
    required init?(coder: NSCoder) { nil }
    override func draw(_ dirtyRect: NSRect) { ink.setFill(); bounds.fill() }
}
#endif
