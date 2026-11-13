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
#endif
