// LLP 1123 D2 on AppKit: an owner's update sampled from its container view as AppKit lays it out, its band, Exact's
// contributions, its chain and relations, its margins, its map into its window and its configuration, which the
// kernel decodes.
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
        // AppKit's margins: the container's, and a plain view's those less its safe area; no minimum or readable width
        let margins = try XCTUnwrap(update.margins)
        let native = LayoutRegionsMac.margins(of: container), safe = container.safeAreaInsets
        XCTAssertEqual([margins.root.top, margins.root.right, margins.root.bottom, margins.root.left], native)
        XCTAssertEqual([margins.plain.top, margins.plain.right, margins.plain.bottom, margins.plain.left],
                       [native[0] - safe.top, native[1] - safe.right, native[2] - safe.bottom, native[3] - safe.left])
        XCTAssertGreaterThan(margins.root.top, update.band?.top ?? 0, "from the safe area under the title bar")
        XCTAssertNil(margins.minimum)
        XCTAssertNil(margins.readableWidth)
        // The screen's auxiliary areas in the owner's space, empty where the screen has no camera housing
        let screen = try XCTUnwrap(window.screen), chrome = try XCTUnwrap(update.chrome)
        XCTAssertNil(chrome.verticalBarEdge)
        XCTAssertNil(chrome.sidebarAvailable)
        if let area = screen.auxiliaryTopLeftArea {
            let local = container.convert(window.convertFromScreen(area), from: nil)
            XCTAssertEqual(chrome.auxiliaryTopLeft, local, "beside the housing, in the owner's space")
        } else {
            XCTAssertEqual(chrome.auxiliaryTopLeft, .zero, "none on this screen")
        }
        XCTAssertEqual(chrome.auxiliaryTopRight, screen.auxiliaryTopRightArea == nil ? .zero : chrome.auxiliaryTopRight)
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
        XCTAssertEqual(LayoutRegionsMac.margins(of: container)[1], (shown.margins?.root.right ?? 0) + 20, accuracy: 0.5, "AppKit adds it to the margins")
        XCTAssertEqual(added.margins?.root, shown.margins?.root, "the root's margins less the contribution, as the band is")
        XCTAssertEqual(added.margins?.plain, shown.margins?.plain)
        // A negative contribution takes the safe area past zero, as AppKit adds it, and the margins less it are as before
        ContributionsMac.of(container).write(.init(node: node.id, operation: .additionalInsets), NSEdgeInsets(top: -100, left: 0, bottom: 0, right: 20))
        window.contentView?.layoutSubtreeIfNeeded()
        let negative = try XCTUnwrap(blocks.pending[node.id])
        XCTAssertEqual(negative.margins?.root, shown.margins?.root, "the root's, less the contributions")
        XCTAssertEqual(negative.margins?.plain, shown.margins?.plain, "a plain view's")
        // The kernel decodes it, and refuses a truncated one
        let encoded = added.encode()
        XCTAssertNil(s.runtime.ownerReports(updates: [(node.id, encoded)]).error)
        XCTAssertNotNil(s.runtime.ownerReports(updates: [(node.id, Data(encoded.prefix(encoded.count - 1)))]).error)
        // Released, its update goes
        blocks.release(node)
        XCTAssertNil(blocks.toldUpdates[node.id])
        XCTAssertNil(blocks.pending[node.id])
    }

    /// D9 on AppKit: a pane's chain is its container, the split item it is, and the split controller above, and its
    /// relations are the window's root, content view and content layout rectangle, and its screen.
    func testAPanesChainAndRelationsAreAppKits() throws {
        let (s, node, container, window) = try owner()
        container.removeFromSuperview()
        let split = NSSplitViewController(), pane = NSViewController(), other = NSViewController()
        pane.view = container
        other.view = NSView()
        split.addSplitViewItem(NSSplitViewItem(viewController: other))
        split.addSplitViewItem(NSSplitViewItem(viewController: pane))
        window.contentViewController = split
        window.setContentSize(NSSize(width: 600, height: 400))
        window.contentView?.layoutSubtreeIfNeeded()
        let update = OwnerUpdateMac.sample(container, session: s.view)
        XCTAssertEqual(update.chain.map(\.kinds), [OwnerUpdateMac.Kind.column, OwnerUpdateMac.Kind.split], "the item, then the split")
        XCTAssertEqual(update.chain.map(\.target.id), [TargetIds.id(of: container), TargetIds.id(of: split.view)])
        XCTAssertEqual(update.chain.first?.target.insets, OwnerUpdateMac.edges(container.safeAreaInsets))
        let relations = Dictionary(uniqueKeysWithValues: update.relations.map { ($0.relation, $0.target) })
        XCTAssertEqual(relations[OwnerUpdateMac.Relation.root]?.id, TargetIds.id(of: split.view), "the window's root controller")
        XCTAssertEqual(relations[OwnerUpdateMac.Relation.window]?.id, TargetIds.id(of: try XCTUnwrap(window.contentView)))
        let content = try XCTUnwrap(relations[OwnerUpdateMac.Relation.windowContent])
        XCTAssertEqual(content.insets?.top ?? 0, window.frame.height - window.contentLayoutRect.maxY, accuracy: 0.5, "the title bar's depth")
        XCTAssertGreaterThan(content.insets?.top ?? 0, 0)
        if let screen = window.screen {
            XCTAssertEqual(relations[OwnerUpdateMac.Relation.screen]?.insets, OwnerUpdateMac.edges(screen.safeAreaInsets))
        }
        XCTAssertNil(relations[OwnerUpdateMac.Relation.presenter], "no sheet's parent")
        XCTAssertNil(s.runtime.ownerReports(updates: [(node.id, update.encode())]).error, "the kernel takes AppKit's relations")
    }

    /// D9's registration on AppKit, as on UIKit: a name is refused when invalid or taken, freed when disposed of, and
    /// the kernel takes the registration.
    func testARegistrationOnAppKitIsUIKits() throws {
        let (s, _, _, window) = try owner()
        let (map, other) = (NSView(), NSView())
        window.contentView?.addSubview(map)
        XCTAssertThrowsError(try ExactNativeTarget.register(name: "Map", view: map, in: s)) {
            XCTAssertEqual($0 as? ExactNativeTarget.Refusal, .invalidName("Map"))
        }
        let held = try ExactNativeTarget.register(name: "map", view: map, in: s)
        XCTAssertThrowsError(try ExactNativeTarget.register(name: "map", view: other, in: s)) {
            XCTAssertEqual($0 as? ExactNativeTarget.Refusal, .taken("map"))
        }
        held.dispose()
        XCTAssertNoThrow(try ExactNativeTarget.register(name: "map", view: other, in: s))
        let record = ExactNativeTarget.encode(name: "pane", target: OwnerUpdateMac.target(map))
        XCTAssertNil(s.runtime.ownerReports(registrations: [record]).error)
        XCTAssertNotNil(s.runtime.ownerReports(registrations: [Data(record.prefix(record.count - 1))]).error)
    }
}
#endif
