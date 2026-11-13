// @ref LLP 1115.002 §4, Customize Toolbar (expo/exact#280 item 8) — a
// declared window toolbar whose every action button has an `id` is
// customizable: the ids are its items' identities, the authored order its
// default, and the person's arrangement AppKit's to keep.
#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

final class ToolbarCustomizationMacTests: XCTestCase {
    private var windows: [NSWindow] = []
    private let saved = "NSToolbar Configuration Exact.window.toolbar.commands"
    override func setUp() { UserDefaults.standard.removeObject(forKey: saved) }
    override func tearDown() {
        windows.forEach { $0.close() }; windows = []
        UserDefaults.standard.removeObject(forKey: saved)
    }

    private func view(_ id: Int, _ kind: String = "view", _ props: [String: String] = [:], press: Bool = false) -> [[String: Any]] {
        [["op": "create", "id": id, "kind": kind, "props": props, "handlers": press ? ["press"] : []],
         ["op": "frame", "id": id, "x": 0, "y": 0, "w": 60, "h": 30]]
    }
    private func button(_ id: Int, _ label: String, _ symbol: String, _ props: [String: String] = [:]) -> [[String: Any]] {
        view(id, "button", ["accessibilityLabel": label].merging(props) { _, new in new }, press: true)
            + view(id * 10, "image", ["symbolName": symbol]) + [["op": "children", "id": id, "ids": [id * 10]]]
    }

    /// A page (1) holding a declared window toolbar (2) of three buttons
    /// (11 Compose, 12 Share, 13 Delete), each with `ids[i]` as its `id`
    /// where given.
    private func fixture(ids: [String?]) -> (Presenter, NSWindow) {
        _ = NSApplication.shared
        let p = Presenter()
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 700, height: 400), styleMask: [.titled, .closable, .resizable], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.title = "Mail"
        window.contentView = p.viewport
        windows.append(window)
        let named = { (i: Int) -> [String: String] in ids[i].map { ["id": $0] } ?? [:] }
        var ops = view(1) + view(2, "view", ["accessibilityRole": "toolbar", "toolbarPlacement": "window"])
        ops += button(11, "Compose", "square.and.pencil", named(0)) + button(12, "Share", "square.and.arrow.up", named(1)) + button(13, "Delete", "trash", named(2))
        ops += [["op": "children", "id": 2, "ids": [11, 12, 13]], ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]]]
        p.apply(wireBatch(ops))
        XCTAssertTrue(p.toolbar.attach(to: window))
        return (p, window)
    }
    private func identifiers(_ window: NSWindow) -> [String] { window.toolbar?.items.map(\.itemIdentifier.rawValue) ?? [] }

    func testAToolbarWhoseButtonsHaveIdsIsCustomizableByThem() throws {
        let (p, window) = fixture(ids: ["compose", "share", "delete"])
        let toolbar = try XCTUnwrap(window.toolbar)
        XCTAssertTrue(toolbar.allowsUserCustomization)
        XCTAssertTrue(toolbar.autosavesConfiguration)
        XCTAssertEqual(toolbar.identifier, "Exact.window.toolbar.commands")
        XCTAssertEqual(identifiers(window), ["exact.item.compose", "exact.item.share", "exact.item.delete"])
        XCTAssertEqual(p.toolbar.toolbarAllowedItemIdentifiers(toolbar).map(\.rawValue),
                       ["exact.item.compose", "exact.item.share", "exact.item.delete", NSToolbarItem.Identifier.space.rawValue, NSToolbarItem.Identifier.flexibleSpace.rawValue])
        // The palette's picture of an item is a copy: the bar keeps its own.
        let shown = try XCTUnwrap(toolbar.items.first)
        let pictured = try XCTUnwrap(p.toolbar.toolbar(toolbar, itemForItemIdentifier: shown.itemIdentifier, willBeInsertedIntoToolbar: false))
        XCTAssertFalse(pictured === shown)
        XCTAssertEqual(pictured.label, "Compose")
        XCTAssertEqual(p.toolbar.summary["customizable"] as? Bool, true)

        // The person takes Share out and puts a space first: the app's next batches keep that.
        toolbar.removeItem(at: 1)
        toolbar.insertItem(withItemIdentifier: .flexibleSpace, at: 0)
        p.apply(wireBatch([["op": "props", "id": 13, "set": ["accessibilityLabel": "Bin"]]]))
        XCTAssertEqual(identifiers(window), [NSToolbarItem.Identifier.flexibleSpace.rawValue, "exact.item.compose", "exact.item.delete"])
        XCTAssertEqual(toolbar.items.last?.label, "Bin", "an item the person kept still follows its button")
        // A second window of the app shows the same arrangement: AppKit keeps one per identity.
        let (_, other) = fixture(ids: ["compose", "share", "delete"])
        XCTAssertEqual(identifiers(other), identifiers(window))
    }

    func testAButtonMountedAgainKeepsItsItemAndANewOneJoinsTheBar() throws {
        let (p, window) = fixture(ids: ["compose", "share", "delete"])
        let toolbar = try XCTUnwrap(window.toolbar)
        let shared = try XCTUnwrap(toolbar.items.first { $0.itemIdentifier.rawValue == "exact.item.share" })
        // Share's button is made again under its id: the bar's item is bound to the new node and still answers.
        p.apply(wireBatch([["op": "destroy", "id": 12]] + button(21, "Share", "square.and.arrow.up", ["id": "share"])
            + [["op": "children", "id": 2, "ids": [11, 21, 13]]]))
        let item = try XCTUnwrap(toolbar.items.first { $0.itemIdentifier.rawValue == "exact.item.share" })
        XCTAssertTrue(item === shared, "the person's item, by identity")
        XCTAssertEqual(item.tag, 21)
        XCTAssertTrue(p.toolbar.validateToolbarItem(item))
        XCTAssertEqual(identifiers(window), ["exact.item.compose", "exact.item.share", "exact.item.delete"])
        // The person takes Delete out; then the app adds Flag after it: Flag joins after Share, Delete stays out.
        toolbar.removeItem(at: 2)
        p.apply(wireBatch(button(15, "Flag", "flag", ["id": "flag"]) + [["op": "children", "id": 2, "ids": [11, 21, 13, 15]]]))
        XCTAssertEqual(identifiers(window), ["exact.item.compose", "exact.item.share", "exact.item.flag"])
        // Flag goes and comes again: it joins again, as an item the app newly offers.
        p.apply(wireBatch([["op": "children", "id": 2, "ids": [11, 21, 13]]]))
        XCTAssertEqual(identifiers(window), ["exact.item.compose", "exact.item.share"])
        p.apply(wireBatch([["op": "children", "id": 2, "ids": [11, 21, 13, 15]]]))
        XCTAssertEqual(identifiers(window), ["exact.item.compose", "exact.item.share", "exact.item.flag"])
    }

    func testAHiddenButtonKeepsItsPlaceHidden() throws {
        let (p, window) = fixture(ids: ["compose", "share", "delete"])
        p.apply(wireBatch([["op": "style", "id": 12, "style": ["display": "none"]], ["op": "props", "id": 12, "set": ["hidden": "true"]]]))
        let share = try XCTUnwrap(window.toolbar?.items.first { $0.itemIdentifier.rawValue == "exact.item.share" })
        XCTAssertFalse(share.isEnabled)
        if #available(macOS 15, *) { XCTAssertTrue(share.isHidden) }
        XCTAssertEqual(identifiers(window), ["exact.item.compose", "exact.item.share", "exact.item.delete"])
    }

    func testAButtonWithoutAnIdLeavesTheToolbarAsTheAuthorArrangedIt() throws {
        let (_, window) = fixture(ids: ["compose", nil, "delete"])
        let toolbar = try XCTUnwrap(window.toolbar)
        XCTAssertFalse(toolbar.allowsUserCustomization)
        XCTAssertEqual(identifiers(window), ["exact.command.11", "exact.command.12", "exact.command.13"])
        let (_, twice) = fixture(ids: ["same", "same", "delete"])
        XCTAssertFalse(try XCTUnwrap(twice.toolbar).allowsUserCustomization, "two buttons with one id have no identity each")
    }
}
#endif
