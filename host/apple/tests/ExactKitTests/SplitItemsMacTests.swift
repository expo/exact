// @ref LLP 1123 D15 — a Mac split item takes its controls from the element that projects it: the
// sidebar's from the root tablist, the page's from its stack's first route and the inspector's from
// the route's `aside`; an accessory is the element its item names, laid out in AppKit's accessory.
#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

final class SplitItemsMacTests: XCTestCase {
    private var window: NSWindow?
    private var session: ExactSession?
    override func tearDown() {
        window?.close(); window = nil
        session?.destroy(); session = nil
    }

    /// A root tab bar, the window's sidebar, over a route with an inspector; `shown` says each control, and a press
    /// unsays them.
    private let app = """
    routes nav
      tab home "/"
      tab other "/other"
    component App
      state shown = true
      action pick(name: string)
        nav = select(nav, name)
      action toggle
        shown = !shown
      view
        main navigationKey=`${top(nav).id}` navigationBack="back" width="100%" height="100%" display="flex" flex-direction="column"
          column flex=1 min-height=0 position="relative"
            for t in nav.tabs key=t.name
              column role="tabpanel" id=`panel-${t.name}` position="absolute" inset=0
                for e in t.stack key=e.id
                  row navigationKey=`${e.id}` testId=`route-${e.name}` position="absolute" inset=0 display="flex" navigationSplitItemAdjustsForOverlays=(shown ? "true" : "false")
                    column flex=1
                      button press=toggle testId="toggle"
                        text "Toggle"
                      column id="tools" testId="tools" height=30 navigationAccessoryContentInsets="false" navigationAccessoryScrollEdgeEffect="hard"
                        text "Tools"
                    aside width=200 testId="inspector" navigationSplitItemFullHeight=(shown ? "false" : "true")
                      text "Inspector"
          row role="tablist" testId="tabs" aria-label="Places" navigationSplitItemFullHeight=(shown ? "false" : "true") navigationSplitItemTopAccessory=(shown ? "tools" : "")
            button role="tab" aria-controls="panel-home" aria-selected=(nav.tab == "home") press=pick("home") testId="tab-home" flex=1
              text "Home"
            button role="tab" aria-controls="panel-other" aria-selected=(nav.tab == "other") press=pick("other") testId="tab-other" flex=1
              text "Other"
    """

    private func boot(_ source: String? = nil) throws -> (ExactSession, ExactSidebarController) {
        _ = NSApplication.shared
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let contract = dir.appendingPathComponent("app.contract"), plan = dir.appendingPathComponent("app.plan")
        try (source ?? app).write(to: contract, atomically: true, encoding: .utf8)
        let compiler = Process(), said = Pipe()
        compiler.executableURL = URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_CONTRACT"]))
        compiler.arguments = ["build", contract.path, "-o", plan.path]
        compiler.standardError = said
        try compiler.run(); compiler.waitUntilExit()
        XCTAssertEqual(compiler.terminationStatus, 0, String(decoding: said.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self))
        let session = ExactApp.shared.makeSession(label: "split-items-" + UUID().uuidString)
        self.session = session
        XCTAssertNil(session.boot(plan: try Data(contentsOf: plan), size: CGSize(width: 1000, height: 500)).error)
        let view = ExactView(session: session)
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1000, height: 500),
                              styleMask: [.titled, .resizable, .fullSizeContentView], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        self.window = window
        window.contentView = NSView(frame: NSRect(x: 0, y: 0, width: 1000, height: 500))
        let split = ExactSidebarController(view: view, autosaveName: nil, minimumPageWidth: 320, sidebar: true)
        split.install(in: window)
        window.setContentSize(NSSize(width: 1000, height: 500))
        window.layoutIfNeeded()
        return (session, split)
    }

    private func node(_ testId: String) throws -> NodeView {
        try XCTUnwrap(session?.presenter.views.values.first { $0.props["testId"] == testId }, testId)
    }

    private func settle(_ what: String, until done: () -> Bool) {
        guard let session else { return XCTFail("\(what): no session") }
        waitFor(what) { session.presenter.settlePump(); session.drainFill(); return done() }
    }

    func testEachItemTakesItsControlsFromTheElementThatProjectsIt() throws {
        let (session, split) = try boot()
        let sidebar = try XCTUnwrap(split.sidebarItem)
        settle("the inspector shows") { split.inspectorItem != nil }
        let inspector = try XCTUnwrap(split.inspectorItem)
        XCTAssertFalse(sidebar.allowsFullHeightLayout, "the tablist's false")
        XCTAssertFalse(inspector.allowsFullHeightLayout, "the aside's false")
        if #available(macOS 26, *) {
            XCTAssertTrue(split.pageItem.automaticallyAdjustsSafeAreaInsets, "the route's true")
            XCTAssertFalse(split.detailItem.automaticallyAdjustsSafeAreaInsets, "nothing splits: AppKit's default")
        }
        // Unsaid, each is AppKit's default again; and a property Exact never owned stays its owner's
        if #available(macOS 26, *) { split.detailItem.automaticallyAdjustsSafeAreaInsets = true }
        session.presenter.press(try node("toggle").id)
        settle("the defaults back") { sidebar.allowsFullHeightLayout && inspector.allowsFullHeightLayout }
        if #available(macOS 26, *) {
            XCTAssertFalse(split.pageItem.automaticallyAdjustsSafeAreaInsets)
            XCTAssertTrue(split.detailItem.automaticallyAdjustsSafeAreaInsets, "the window's owner's, untouched")
        }
    }

    /// An inspector that goes gives back the element it showed as an accessory, to its route.
    func testAnInspectorThatGoesGivesBackItsAccessory() throws {
        guard #available(macOS 26, *) else { throw XCTSkip("AppKit's split item accessories are macOS 26's") }
        let source = app
            .replacingOccurrences(of: #"aside width=200 testId="inspector" navigationSplitItemFullHeight=(shown ? "false" : "true")"#,
                                  with: #"aside width=200 testId="inspector" visibility=(shown ? "visible" : "hidden") navigationSplitItemTopAccessory="tools""#)
            .replacingOccurrences(of: #" navigationSplitItemTopAccessory=(shown ? "tools" : "")"#, with: "")
        let (session, split) = try boot(source)
        let tools = try node("tools")
        settle("the inspector shows its accessory") { split.inspectorItem?.topAlignedAccessoryViewControllers.count == 1 }
        XCTAssertTrue(session.presenter.blocks.owns(tools))
        session.presenter.press(try node("toggle").id)
        settle("the inspector goes") { split.inspectorItem == nil }
        XCTAssertFalse(session.presenter.blocks.owns(tools), "given back")
        XCTAssertTrue(tools.isDescendant(of: try node("route-home")), "back in its route")
    }

    func testAnAccessoryIsTheElementItsItemNames() throws {
        guard #available(macOS 26.1, *) else { throw XCTSkip("AppKit's accessory scroll edge effect is macOS 26.1's") }
        let (session, split) = try boot()
        let sidebar = try XCTUnwrap(split.sidebarItem), tools = try node("tools")
        settle("the accessory shows") { sidebar.topAlignedAccessoryViewControllers.count == 1 }
        let accessory = try XCTUnwrap(sidebar.topAlignedAccessoryViewControllers.first)
        XCTAssertTrue(tools.isDescendant(of: accessory.view), "the element's view in the accessory's")
        XCTAssertTrue(session.presenter.blocks.owns(tools), "laid out in the accessory's block")
        XCTAssertEqual(tools.frame.width, accessory.view.bounds.width, accuracy: 0.5, "at the width AppKit gives it")
        XCTAssertEqual(accessory.view.bounds.height, 30, accuracy: 0.5, "at its own height")
        XCTAssertFalse(accessory.automaticallyAppliesContentInsets, "its element's false")
        XCTAssertEqual(accessory.preferredScrollEdgeEffectStyle, .hard, "its element's hard")
        // Unnamed, the accessory goes and the element is the route's again
        session.presenter.press(try node("toggle").id)
        settle("the accessory goes") { sidebar.topAlignedAccessoryViewControllers.isEmpty }
        XCTAssertFalse(session.presenter.blocks.owns(tools))
        XCTAssertFalse(tools.isDescendant(of: accessory.view))
        XCTAssertTrue(tools.isDescendant(of: try node("route-home")), "back in its route")
    }
}
#endif
