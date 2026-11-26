// LLP 1123 D2 on AppKit: an owner's update sampled from its container view as AppKit lays it out, its band, Exact's
// contributions, its map into its window and its configuration, which the kernel decodes.
#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit

final class OwnerUpdateMacTests: XCTestCase {
    private var windows: [NSWindow] = []
    private var session: ExactSession?

    override func tearDown() {
        session?.destroy(); session = nil
        windows.forEach { $0.close() }; windows = []
        super.tearDown()
    }

    private func plan(_ source: String) throws -> Data {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        try source.write(to: dir.appendingPathComponent("app.contract"), atomically: true, encoding: .utf8)
        let compiler = Process()
        compiler.executableURL = URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_CONTRACT"]))
        compiler.arguments = ["build", dir.appendingPathComponent("app.contract").path, "-o", dir.appendingPathComponent("app.plan").path]
        compiler.standardError = FileHandle.nullDevice
        try compiler.run(); compiler.waitUntilExit()
        XCTAssertEqual(compiler.terminationStatus, 0)
        return try Data(contentsOf: dir.appendingPathComponent("app.plan"))
    }

    /// A session whose pane is shown as an owner in a container filling a full-size-content window, sampling owners.
    private func owner() throws -> (ExactSession, NodeView, BlockView, NSWindow) {
        _ = NSApplication.shared
        let s = ExactApp.shared.makeSession(label: "owner-update-mac")
        session = s
        XCTAssertNil(s.boot(plan: try plan("component App\n  view\n    column testId=\"pane\"\n      text \"Pane\"\n"), size: CGSize(width: 600, height: 400)).error)
        let p = s.presenter
        let node = try XCTUnwrap(p.views.values.first { $0.props["testId"] == "pane" })
        let window = NSWindow(contentRect: NSRect(x: 100, y: 100, width: 600, height: 400), styleMask: [.titled, .resizable, .fullSizeContentView],
                              backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        windows.append(window)
        let content = try XCTUnwrap(window.contentView)
        let container = BlockView(frame: content.bounds)
        container.autoresizingMask = [.width, .height]
        content.addSubview(container)
        window.orderFront(nil)
        p.blocks.samplesOwners = true
        p.blocks.show(node, in: container, block: .size(container.bounds.size))
        return (s, node, container, window)
    }

    func testAnOwnersUpdateIsItsContainersBandMapAndConfiguration() throws {
        let (s, node, container, window) = try owner()
        let blocks = s.presenter.blocks
        let update = try XCTUnwrap(blocks.toldUpdates[node.id], "sampled as it is shown")
        XCTAssertEqual(update.band, OwnerUpdateMac.edges(container.safeAreaInsets))
        XCTAssertGreaterThan(update.band?.top ?? 0, 0, "under the title bar its full-size content spans")
        let map = try XCTUnwrap(update.windowMap)
        XCTAssertEqual([map.tx, map.ty, map.a, map.d], [0, 0, 1, 1], "the container's origin at the window's top left, y down")
        XCTAssertNotNil(update.configuration)
        XCTAssertEqual(update.keyboard, .unreported, "AppKit has no software keyboard")
        _ = blocks.takeUpdates()
        // A toolbar shown at the same size moves the band and the configuration, sampled in the container's layout
        window.toolbar = NSToolbar(identifier: "owner-update")
        window.toolbar?.isVisible = true
        window.contentView?.layoutSubtreeIfNeeded()
        let shown = try XCTUnwrap(blocks.pending[node.id], "a new sample at the same size")
        XCTAssertEqual(shown.band, OwnerUpdateMac.edges(container.safeAreaInsets))
        XCTAssertGreaterThan(shown.band?.top ?? 0, update.band?.top ?? 0, "the toolbar's depth")
        XCTAssertNotEqual(shown.configuration, update.configuration)
        _ = blocks.takeUpdates()
        // Exact's contribution to the container, with its effect, AppKit's addition
        ContributionsMac.of(container).write(.init(node: node.id, operation: .additionalInsets), NSEdgeInsets(top: 0, left: 0, bottom: 0, right: 20))
        window.contentView?.layoutSubtreeIfNeeded()
        let added = try XCTUnwrap(blocks.pending[node.id])
        XCTAssertEqual(added.contributions, [.init(node: node.id, operation: ContributionOperation.additionalInsets.rawValue, effect: .init(top: 0, right: 20, bottom: 0, left: 0))])
        XCTAssertEqual(added.band?.right ?? 0, (shown.band?.right ?? 0) + 20, accuracy: 0.5)
        // The kernel decodes it, and refuses a truncated one
        let encoded = added.encode()
        XCTAssertNil(s.runtime.ownerReports(updates: [(node.id, encoded)]).error)
        XCTAssertNotNil(s.runtime.ownerReports(updates: [(node.id, Data(encoded.prefix(encoded.count - 1)))]).error)
        // Released, its update goes
        blocks.release(node)
        XCTAssertNil(blocks.toldUpdates[node.id])
        XCTAssertNil(blocks.pending[node.id])
    }
}
#endif
