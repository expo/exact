#if os(macOS)
import AppKit
import ExactCore
import XCTest
@testable import ExactKit
@testable import ExactNativeViews

/// LLP 1123 D15 on AppKit: a native view's safe-area declaration reaches the host as it mounts; the host clears what is
/// owed in a consenting view through the view's own additional safe-area insets, beside the app's, writes nothing for
/// one that does not consent, claims its lent scroll view for D14's writer, and gives all of it back as the view goes,
/// before the declaration's release.
final class NativeSafeAreaMacTests: XCTestCase {
    private var window: NSWindow?

    override func tearDown() {
        NativeViews.uninstallTable()
        window?.close()
        window = nil
        super.tearDown()
    }

    func testANativeViewsClearanceAndLentScrollerAreTheHostsUntilTheViewGoes() throws {
        _ = NSApplication.shared
        SafeAreaModule.table.withUnsafeBytes { NativeViews.install(table: $0.baseAddress!) }
        let p = Presenter()
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 300, height: 200), styleMask: [.titled, .fullSizeContentView],
                              backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        self.window = window
        let host = try XCTUnwrap(window.contentView)
        let natives = NativeViews()
        natives.install(module: UnsafeMutableRawPointer(bitPattern: 1)!, gateOpen: true)
        natives.firstFrameTags = ["clearing-view", "plain-view"]
        func mount(_ tag: String, _ id: UInt32) -> NodeView {
            let owner = NodeView(id: id, kind: "native", presenter: p)
            owner.frame = host.bounds
            host.addSubview(owner)
            natives.create(owner: owner)
            owner.props = ["nativeViewModuleName": tag]
            natives.update(owner)
            return owner
        }
        let clearing = mount("clearing-view", 1), plain = mount("plain-view", 2)
        let declared = try XCTUnwrap(natives.safeArea(of: 1))
        XCTAssertTrue(declared.adjustsAdditionalSafeAreaInsets)
        XCTAssertEqual(declared.safeAreaRegions, 1)
        XCTAssertNotNil(natives.safeArea(of: 2), "declares a scroll view but no clearance")
        let view = try XCTUnwrap(natives.entries[1]?.view)
        XCTAssertTrue(view.superview === clearing, "the view in the node's box")
        view.additionalSafeAreaInsets = NSEdgeInsets(top: 0, left: 0, bottom: 10, right: 0)
        XCTAssertTrue(natives.clear(1, owed: NSEdgeInsets(top: 30, left: 0, bottom: 0, right: 0)))
        XCTAssertEqual(view.additionalSafeAreaInsets.top, 30, "the clearance on the view's own safe area")
        XCTAssertEqual(view.additionalSafeAreaInsets.bottom, 10, "beside the app's own")
        XCTAssertFalse(natives.clear(2, owed: NSEdgeInsets(top: 30, left: 0, bottom: 0, right: 0)), "nothing for a view that does not consent")
        XCTAssertEqual(try XCTUnwrap(natives.entries[2]?.view).additionalSafeAreaInsets.top, 0)
        // The lent scroll view is claimed for D14's writer, its layout's clearance the consumed term
        let lent = try XCTUnwrap(natives.lent(of: 1).first)
        XCTAssertTrue(lent.view.isDescendant(of: view), "the module's own scroll view")
        lent.view.automaticallyAdjustsContentInsets = true
        lent.write(in: host, terms: .init(), rows: .init())
        XCTAssertNotNil(ScrollInsetsMac.claim(of: lent.view))
        XCTAssertFalse(lent.view.automaticallyAdjustsContentInsets)
        let band = host.safeAreaInsets.top
        XCTAssertEqual(lent.view.contentInsets.top, band - min(band, 8), accuracy: 0.01, "less what its layout cleared, at most 8")
        // As the view goes: the clearance and the claim given back, then the declaration's release, once
        natives.destroy(id: 1)
        XCTAssertEqual(view.additionalSafeAreaInsets.top, 0, "the clearance given back")
        XCTAssertEqual(view.additionalSafeAreaInsets.bottom, 10, "the app's kept")
        XCTAssertNil(ScrollInsetsMac.claim(of: lent.view))
        XCTAssertTrue(lent.view.automaticallyAdjustsContentInsets, "its own adjustment back")
        XCTAssertEqual(SafeAreaModule.released, 1, "the release ran once")
        natives.destroy(id: plain.id)
    }
}

/// A module of two tags whose views declare their safe area: "clearing-view" consents to clearance and lends its scroll
/// view, whose layout clears up to 8 points on each edge; "plain-view" lends one and consents to none.
private enum SafeAreaModule {
    final class Instance {
        let view = NSView(frame: NSRect(x: 0, y: 0, width: 300, height: 200))
        let scroll = NSScrollView(frame: NSRect(x: 0, y: 0, width: 300, height: 200))
        let clears: Bool
        init(clears: Bool) {
            self.clears = clears
            scroll.documentView = NSView(frame: NSRect(x: 0, y: 0, width: 300, height: 1000))
            view.addSubview(scroll)
        }
    }
    nonisolated(unsafe) static var released = 0
    static let roster = strdup(#"{"clearing-view":{"snapshot":false,"firstFrame":true},"plain-view":{"snapshot":false,"firstFrame":true}}"#)!
    static let table: [UInt8] = {
        var table = [UInt8](repeating: 0, count: 272)
        func put<T>(_ value: T, _ offset: Int) {
            withUnsafeBytes(of: value) { for (i, byte) in $0.enumerated() { table[offset + i] = byte } }
        }
        let create: @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, UInt32, UnsafePointer<UInt8>?, UInt32,
            UnsafeRawPointer?, UnsafeRawPointer?, UnsafeMutableRawPointer?, UInt32, UnsafeMutablePointer<UInt8>?, UInt32) -> UnsafeMutableRawPointer? = {
            _, tag, count, _, _, _, _, _, _, _, _ in
            let name = String(decoding: UnsafeBufferPointer(start: tag, count: Int(count)), as: UTF8.self)
            return Unmanaged.passRetained(Instance(clears: name == "clearing-view")).toOpaque()
        }
        let view: @convention(c) (UnsafeMutableRawPointer?) -> UnsafeMutableRawPointer? = {
            Unmanaged.passUnretained(Unmanaged<Instance>.fromOpaque($0!).takeUnretainedValue().view).toOpaque()
        }
        let set: @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, UInt32, UnsafeMutablePointer<UInt8>?, UInt32) -> Int32 = { _, _, _, _, _ in 0 }
        let destroy: @convention(c) (UnsafeMutableRawPointer?) -> Void = { Unmanaged<Instance>.fromOpaque($0!).release() }
        let none: @convention(c) (UnsafeMutableRawPointer?) -> Void = { _ in }
        let safeArea: @convention(c) (UnsafeMutableRawPointer?) -> UnsafeMutableRawPointer? = { raw in
            typealias Layout = @convention(block) (NSEdgeInsets) -> NSEdgeInsets
            typealias Release = @convention(block) () -> Void
            let instance = Unmanaged<Instance>.fromOpaque(raw!).takeUnretainedValue()
            let layout: Layout = { NSEdgeInsets(top: min($0.top, 8), left: min($0.left, 8), bottom: min($0.bottom, 8), right: min($0.right, 8)) }
            let release: Release = { SafeAreaModule.released += 1 }
            let declaration: NSDictionary = ["adjusts": instance.clears, "scrollViews": [["view": instance.scroll, "layout": layout] as NSDictionary],
                                             "regions": 1, "release": release]
            return Unmanaged.passRetained(declaration).toOpaque()
        }
        put(NativeViews.tableMajor, 0); put(UInt32(272), 4); put(UnsafeRawPointer(roster), 8)
        put(unsafeBitCast(create, to: UnsafeRawPointer.self), 16)
        put(unsafeBitCast(view, to: UnsafeRawPointer.self), 24)
        put(unsafeBitCast(set, to: UnsafeRawPointer.self), 32)
        put(unsafeBitCast(destroy, to: UnsafeRawPointer.self), 48)
        for offset in [72, 80, 88, 96] { put(unsafeBitCast(none, to: UnsafeRawPointer.self), offset) }
        put(unsafeBitCast(safeArea, to: UnsafeRawPointer.self), 264)
        return table
    }()
}
#endif
