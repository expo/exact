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
        // End a palette a failed test left open, so the next test's window is not under its sheet
        for window in windows {
            if let sheet = window.attachedSheet { window.endSheet(sheet) }
            window.close()
        }
        windows = []
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

    /// AppKit reads which optional methods a toolbar's delegate answers
    /// once, when it is set, and later calls any method it answered then,
    /// even if the delegate no longer answers it: the customization palette
    /// asks for the immovable items. Inserting or removing an item in code
    /// does not ask for them.
    func testAToolbarDelegatesAnswersAreReadOnceWhenItIsSet() throws {
        _ = NSApplication.shared
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 700, height: 400), styleMask: [.titled, .closable, .resizable], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        windows.append(window)
        let toolbar = NSToolbar(identifier: "answers-read-once")
        toolbar.allowsUserCustomization = true
        let app = Immovable()
        toolbar.delegate = app
        window.toolbar = toolbar
        XCTAssertEqual(app.queries, 1, "asked once, when it was set")
        app.answers = false
        toolbar.insertItem(withItemIdentifier: .flexibleSpace, at: 0)
        toolbar.removeItem(at: 0)
        XCTAssertEqual(app.calls, 0, "an insertion or a removal in code does not ask")
        toolbar.runCustomizationPalette(nil)
        if let sheet = window.attachedSheet { window.endSheet(sheet) }
        XCTAssertEqual(app.queries, 1, "and never asked again")
        XCTAssertEqual(app.calls, 1, "the palette called it though it no longer answers for it")
    }

    /// The app's toolbar delegate is kept until the app gives another or
    /// nil, and Exact's delegate sends it every call Exact does not implement,
    /// whatever the delegate answers at the call: AppKit calls what Exact's
    /// delegate answered for when the app's was given
    /// (`testAToolbarDelegatesAnswersAreReadOnceWhenItIsSet`), and the app need
    /// not hold its delegate for Exact's to answer.
    func testAForwardedToolbarDelegateIsKeptAndCalledUntilTheAppClearsIt() throws {
        let (p, window) = fixture(ids: ["compose", "share", "delete"])
        let toolbar = try XCTUnwrap(window.toolbar)
        let immovable = #selector(NSToolbarDelegate.toolbarImmovableItemIdentifiers(_:))
        weak var given: Immovable?
        autoreleasepool {
            let app = Immovable()
            given = app
            p.toolbar.setAppDelegate(app)
        }
        // Drain what the calls and the assertions autorelease before asking whether the delegate is gone
        autoreleasepool {
            XCTAssertNotNil(given, "kept, though the app holds it no longer")
            given?.answers = false
            // Sent as AppKit sends it, past `responds(to:)`, which AppKit asked when the delegate was given
            _ = (toolbar.delegate as? NSObject)?.perform(immovable, with: toolbar)
            XCTAssertEqual(given?.calls, 1, "Exact's delegate sent the call on")
            p.toolbar.setAppDelegate(nil)
        }
        waitForMainQueue()
        XCTAssertNil(given, "released once the app clears it")
    }

    /// The app's toolbar delegate goes with its toolbar, once the toolbar's
    /// slot is empty, so AppKit asks nothing of it as it goes, and what its
    /// `deinit` does (clearing the app's delegate, as a delegate keeping its
    /// handle may) runs once the reset has ended.
    func testAForwardedToolbarDelegateGoesWithItsToolbar() throws {
        let (p, window) = fixture(ids: ["compose", "share", "delete"])
        let toolbar = try XCTUnwrap(window.toolbar)
        weak var given: Immovable?
        var slotAsItWent: Bool?
        autoreleasepool {
            let app = Immovable()
            given = app
            app.goes = { [weak p] in
                slotAsItWent = toolbar.delegate != nil
                p?.toolbar.setAppDelegate(nil)
            }
            p.toolbar.setAppDelegate(app)
        }
        autoreleasepool { p.toolbar.reset() }
        waitForMainQueue()
        XCTAssertNil(given, "released with its toolbar")
        XCTAssertEqual(slotAsItWent, false, "the toolbar's slot was empty as the delegate went")
        XCTAssertNil(toolbar.delegate, "and stays empty")
    }

    /// A toolbar let go while its customization palette is open leaves the
    /// palette to AppKit: nothing raises as the palette ends.
    func testAToolbarLetGoWithItsPaletteOpenEndsThePaletteQuietly() throws {
        let (p, window) = fixture(ids: ["compose", "share", "delete"])
        let toolbar = try XCTUnwrap(window.toolbar)
        p.toolbar.setAppDelegate(Immovable())
        toolbar.runCustomizationPalette(nil)
        XCTAssertNotNil(window.attachedSheet, "the palette is open")
        p.toolbar.reset()
        waitForMainQueue("the app's delegate goes")
        if let sheet = window.attachedSheet { window.endSheet(sheet) }
        waitFor("the palette ends") { window.attachedSheet == nil }
        waitForMainQueue("what the palette's end queued")
    }

    /// A delegate that clears the app's delegate from inside its own
    /// `responds(to:)`, while AppKit asks it what it answers, leaves AppKit no
    /// answer for a method nothing would receive: the palette runs quietly.
    func testADelegateClearedWhileAppKitAsksItLeavesNoAnswerWithoutAReceiver() throws {
        let (p, window) = fixture(ids: ["compose", "share", "delete"])
        let toolbar = try XCTUnwrap(window.toolbar)
        let app = Immovable()
        app.asked = { [weak p] in p?.toolbar.setAppDelegate(nil) }
        p.toolbar.setAppDelegate(app)
        XCTAssertFalse(toolbar.delegate?.responds(to: #selector(NSToolbarDelegate.toolbarImmovableItemIdentifiers(_:))) ?? true, "cleared")
        toolbar.runCustomizationPalette(nil)
        if let sheet = window.attachedSheet { window.endSheet(sheet) }
        XCTAssertEqual(app.calls, 0, "nothing reached the cleared delegate")
    }

    /// A toolbar delegate replaced by another goes after the swap: one that
    /// clears the app's delegate as it goes, as a delegate keeping its handle
    /// may, clears the new one, and the slot is still Exact's.
    func testAForwardedToolbarDelegateThatClearsAsItGoesClearsItsSuccessor() throws {
        let (p, window) = fixture(ids: ["compose", "share", "delete"])
        let toolbar = try XCTUnwrap(window.toolbar)
        autoreleasepool {
            let app = Immovable()
            app.goes = { [weak p] in p?.toolbar.setAppDelegate(nil) }
            p.toolbar.setAppDelegate(app)
        }
        autoreleasepool { p.toolbar.setAppDelegate(Immovable()) }
        waitForMainQueue()
        let proxy = try XCTUnwrap(p.toolbar.delegateProxy)
        XCTAssertTrue(toolbar.delegate === proxy, "the slot is Exact's")
        XCTAssertNil(proxy.appDelegate, "the successor, cleared")
    }

    /// A toolbar that comes as the old toolbar's delegate goes (the view moved
    /// to another window), and the delegate given to it then, keep the proxy
    /// the new toolbar got: the old delegate goes after the detach ends, which
    /// would otherwise take the window from the host.
    func testAToolbarThatComesAsTheOldDelegateGoesKeepsItsProxy() throws {
        let (p, _) = fixture(ids: ["compose", "share", "delete"])
        let next = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 700, height: 400), styleMask: [.titled, .closable, .resizable], backing: .buffered, defer: false)
        next.isReleasedWhenClosed = false
        windows.append(next)
        let successor = Immovable()
        autoreleasepool {
            let app = Immovable()
            app.goes = { [weak p] in
                guard let p else { return }
                next.contentView = p.viewport
                p.toolbar.attach(to: next)
                p.toolbar.setAppDelegate(successor)
            }
            p.toolbar.setAppDelegate(app)
        }
        autoreleasepool { p.toolbar.detach() }
        waitForMainQueue()
        let proxy = try XCTUnwrap(p.toolbar.delegateProxy, "the new toolbar's proxy is the host's")
        XCTAssertTrue(next.toolbar?.delegate === proxy, "and in the new toolbar's slot")
        XCTAssertTrue(proxy.appDelegate === successor, "with the delegate given to it")
    }

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

/// A toolbar delegate that returns no immovable items. `answers` sets
/// whether it responds to that method; it counts those queries and calls,
/// and runs `goes` in `deinit`. Its own items are for a toolbar it is set on
/// directly.
private final class Immovable: NSObject, NSToolbarDelegate {
    var answers = true, queries = 0, calls = 0
    var goes: (() -> Void)?
    /// Run once, the first time AppKit asks whether it answers for the immovable items
    var asked: (() -> Void)?
    func toolbarAllowedItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] { [.init("one"), .flexibleSpace, .space] }
    func toolbarDefaultItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] { [.init("one")] }
    func toolbar(_ toolbar: NSToolbar, itemForItemIdentifier identifier: NSToolbarItem.Identifier, willBeInsertedIntoToolbar flag: Bool) -> NSToolbarItem? {
        let item = NSToolbarItem(itemIdentifier: identifier)
        item.label = identifier.rawValue
        item.image = NSImage(systemSymbolName: "circle", accessibilityDescription: nil)
        return item
    }
    override func responds(to selector: Selector!) -> Bool {
        guard selector == #selector(NSToolbarDelegate.toolbarImmovableItemIdentifiers(_:)) else { return super.responds(to: selector) }
        queries += 1
        if let asked { self.asked = nil; asked() }
        return answers
    }
    func toolbarImmovableItemIdentifiers(_ toolbar: NSToolbar) -> Set<NSToolbarItem.Identifier> {
        calls += 1
        return []
    }
    deinit { goes?() }
}
#endif
