// @ref LLP 1047.001 D10 — the app's native module, its native views and its
// hatches are a family the composition selects (the roster's
// `native_views`). The core reaches it only through what this file
// declares. `ExactNativeViews.install()` sets the slots; until it runs the
// session and the presenter hold `NoNativeModule` and `NoElementHatches`,
// which own no view and call no hatch. A binary whose linked set lacks
// `native_views` refuses a plan with a module tag or a hatch at boot
// (`Unlinked("native_views")`), and one with a package view, which needs the
// runner's module seam too, as `Unlinked("modules, native_views")`.
// Images' package pictures ask the module for its image entry, and video and
// web views tell element hatches of their platform objects, through these
// protocols.
#if os(macOS)
import AppKit
#else
import UIKit
#endif

#if os(macOS)
package typealias NativePlatformView = NSView
package typealias NativeImage = NSImage
#else
package typealias NativePlatformView = UIView
package typealias NativeImage = UIImage
#endif

/// A package's image entry: what `PackagePictures` calls to draw one.
package typealias PackageImageFn = @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, Int, UnsafeMutableRawPointer?,
                                           (@convention(c) (UnsafeMutableRawPointer?, UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, Int) -> Void)?) -> Void

/// A route or element hatch's moment; `configuring` precedes a frame's web
/// view or a video's item (LLP 1133 D3).
package enum RouteHatchEvent: UInt32 {
    case built = 0, changed = 1, ended = 2, configuring = 3
    package var name: String { switch self { case .built: "built"; case .changed: "changed"; case .ended: "ended"; case .configuring: "configuring" } }
}

/// What the core asks of a session's native module and its views.
package protocol NativeModuleHosting: AnyObject {
    var session: ExactSession? { get set }
    /// What Exact times of each hatch call (development only).
    var diagnostics: (any HatchDiagnosing)? { get }
    /// How many times its hatches have connected: a record's incarnation.
    var hatchIncarnation: UInt32 { get }
    var isEmpty: Bool { get }
    var hatchesConnected: Bool { get }
    var onHatchesConnected: (() -> Void)? { get set }
    var activationQueued: Bool { get }
    var quitWorkPending: Bool { get }
    /// The embedder's words that the session's window, and the process, are its own.
    var ownsWindow: Bool { get set }
    var ownsProcess: Bool { get set }
    /// Why the app's module artifact did not load, or nil.
    func artifactFailure() -> String?
    /// The modules whose views a first frame waits for.
    var firstFrameTags: Set<String> { get }
    /// Whether a heavy leaf holds a view back, and what each creation cost.
    var holds: ((NodeView) -> Bool)? { get set }
    var measured: ((String, TimeInterval) -> Void)? { get set }

    // The module.
    func installAppModule()
    func prepareAppModule()
    func activateAfterCommit(_ live: @escaping () -> Bool)
    func loadIfNeeded()
    func destroyModule()
    func sessionViewChanged()
    func willQuit()
    func formaMoved()
    func imageEntry() -> (UnsafeMutableRawPointer, PackageImageFn)?
    func authenticator() -> (UnsafeMutableRawPointer, PresenceAuthenticateFn)?

    // The hatches' clock (HatchClock.swift).
    func hatchSeq(_ seq: UInt64)
    func resetHatchClock()
    func beginHatchCommand()
    func refreshHatchClock(at now: Double) -> String?
    /// The next `after` due, and the next instant anything on the clock is.
    var hatchNextAfter: Double? { get }
    var hatchNextInstant: Double? { get }
    func fireHatchClock(at now: Double) -> String?
    @discardableResult func hatchesPresented(_ now: Double) -> Bool
    var liveTickets: Int { get }

    // Scopes and hatches.
    func scopesChanged()
    func windowLeaving(_ window: FocusWindow?)
    /// Whether the module has tab hatches (`tabs` and `tabContainer`).
    var hasTabHatches: Bool { get }
    func tabsHatch(_ controller: AnyObject?, event: UInt32, index: Int)
    func tabContainerHatch(names: [String], nodes: [UInt32], selected: Int, controllers: [AnyObject]) -> AnyObject?
    func tabNodesChanged(_ nodes: [UInt32])
    func navigationHatch(_ controller: AnyObject, built: Bool, showsBar: Bool, label: String) -> Bool
    func routeHatch(_ event: RouteHatchEvent, controller: AnyObject, navigation: AnyObject?, scroll: AnyObject?, key: String, dataset: String?)
    func toolbarHatch(_ toolbar: AnyObject, window: AnyObject)

    // Views.
    func create(owner: NodeView)
    func update(_ owner: NodeView)
    func destroy(id: UInt32)
    func laidOut(_ owner: NodeView)
    func release(_ owner: NodeView)
    func recycleFar(hide: CGFloat, release: CGFloat, _ distance: (NodeView) -> CGFloat?) -> [NodeView]
    var snapshotViews: [NativePlatformView] { get }
    func snapshots() -> [UInt32: NativeImage]
    func decorate(_ tree: [String: Any]) -> [String: Any]
    #if canImport(UIKit)
    func redrawForCapture()
    #endif
    #if os(iOS) || os(tvOS)
    var observation: [String: Any] { get }
    func speaksForItself(_ owner: NodeView) -> Bool
    func declined(_ owner: NodeView, at point: CGPoint, with event: UIEvent?) -> Bool
    func refreshWorldGeometry()
    func routeLoaded(_ route: NodeView)
    #else
    func ownsFocus(_ owner: NodeView) -> Bool
    @discardableResult func focus(_ owner: NodeView) -> Bool
    func focusTarget(_ owner: NodeView) -> NSView?
    func inputToken(_ owner: NodeView) -> UInt32?
    func input(_ owner: NodeView, request: [String: Any], token: UInt32?) -> [String: Any]
    #endif
}

extension NativeModuleHosting {
    package func tabsHatch(_ controller: AnyObject?, event: UInt32) { tabsHatch(controller, event: event, index: 0) }
}

/// No native module: no view, no hatch, nothing in flight.
final class NoNativeModule: NativeModuleHosting {
    weak var session: ExactSession?
    var diagnostics: (any HatchDiagnosing)? { nil }
    var hatchIncarnation: UInt32 { 0 }
    var isEmpty: Bool { true }
    var hatchesConnected: Bool { false }
    var onHatchesConnected: (() -> Void)?
    var activationQueued: Bool { false }
    var quitWorkPending: Bool { false }
    var ownsWindow = false
    var ownsProcess = false
    func artifactFailure() -> String? { nil }
    var firstFrameTags: Set<String> { [] }
    var holds: ((NodeView) -> Bool)?
    var measured: ((String, TimeInterval) -> Void)?
    func installAppModule() {}
    func prepareAppModule() {}
    func activateAfterCommit(_ live: @escaping () -> Bool) {}
    func loadIfNeeded() {}
    func destroyModule() {}
    func sessionViewChanged() {}
    func willQuit() {}
    func formaMoved() {}
    func imageEntry() -> (UnsafeMutableRawPointer, PackageImageFn)? { nil }
    func authenticator() -> (UnsafeMutableRawPointer, PresenceAuthenticateFn)? { nil }
    func hatchSeq(_ seq: UInt64) {}
    func resetHatchClock() {}
    func beginHatchCommand() {}
    func refreshHatchClock(at now: Double) -> String? { nil }
    var hatchNextAfter: Double? { nil }
    var hatchNextInstant: Double? { nil }
    func fireHatchClock(at now: Double) -> String? { nil }
    func hatchesPresented(_ now: Double) -> Bool { false }
    var liveTickets: Int { 0 }
    func scopesChanged() {}
    func windowLeaving(_ window: FocusWindow?) {}
    var hasTabHatches: Bool { false }
    func tabsHatch(_ controller: AnyObject?, event: UInt32, index: Int) {}
    func tabContainerHatch(names: [String], nodes: [UInt32], selected: Int, controllers: [AnyObject]) -> AnyObject? { nil }
    func tabNodesChanged(_ nodes: [UInt32]) {}
    func navigationHatch(_ controller: AnyObject, built: Bool, showsBar: Bool, label: String) -> Bool { showsBar }
    func routeHatch(_ event: RouteHatchEvent, controller: AnyObject, navigation: AnyObject?, scroll: AnyObject?, key: String, dataset: String?) {}
    func toolbarHatch(_ toolbar: AnyObject, window: AnyObject) {}
    func create(owner: NodeView) {}
    func update(_ owner: NodeView) {}
    func destroy(id: UInt32) {}
    func laidOut(_ owner: NodeView) {}
    func release(_ owner: NodeView) {}
    func recycleFar(hide: CGFloat, release: CGFloat, _ distance: (NodeView) -> CGFloat?) -> [NodeView] { [] }
    var snapshotViews: [NativePlatformView] { [] }
    func snapshots() -> [UInt32: NativeImage] { [:] }
    func decorate(_ tree: [String: Any]) -> [String: Any] { tree }
    #if canImport(UIKit)
    func redrawForCapture() {}
    #endif
    #if os(iOS) || os(tvOS)
    var observation: [String: Any] { [:] }
    func speaksForItself(_ owner: NodeView) -> Bool { false }
    func declined(_ owner: NodeView, at point: CGPoint, with event: UIEvent?) -> Bool { false }
    func refreshWorldGeometry() {}
    func routeLoaded(_ route: NodeView) {}
    #else
    func ownsFocus(_ owner: NodeView) -> Bool { false }
    func focus(_ owner: NodeView) -> Bool { false }
    func focusTarget(_ owner: NodeView) -> NSView? { nil }
    func inputToken(_ owner: NodeView) -> UInt32? { nil }
    func input(_ owner: NodeView, request: [String: Any], token: UInt32?) -> [String: Any] { ["error": "no native module"] }
    #endif
}

/// The most text a hatch's act may carry, 64 KB (`input`'s text, a tab
/// container's URL): more is refused at the asking (`refusedOverBound`).
package let hatchTextBytes = 65536

/// What the core asks of a presenter's element hatches (LLP 1075.003.000).
package protocol ElementHatching: AnyObject {
    // A batch's moments.
    func begin(_ batch: Batch)
    func created(_ node: NodeView)
    func propsChanged(_ id: UInt32)
    func destroyed(_ id: UInt32)
    func reset()
    func replay()
    // A platform object's moments (video, web).
    func realized(_ node: NodeView)
    func configuring(_ node: NodeView, _ configuration: AnyObject)
    func waits(_ node: NodeView) -> Bool
    func releaseConfigurations()
    // Acts a hatch asks of an element, queued.
    @discardableResult func later(_ id: UInt32, _ name: String, bytes: Int, _ work: @escaping () -> Void) -> Bool
    @discardableResult func later(_ what: String, _ name: String, bytes: Int, _ work: @escaping () -> Void) -> Bool
    func refusedOverBound(_ what: String, _ name: String, bytes: Int) -> Bool
    var inFlight: Int { get }
    func drain()
    func observation(_ diagnostics: (any HatchDiagnosing)?) -> [String: Any]
    #if os(iOS) || os(tvOS)
    func checkOwned()
    #endif
    // What hatches told the agent of what they made (HatchRegions.swift).
    func decorateRegions(_ tree: [String: Any]) -> [String: Any]
    func regionOwner(of view: PlatformView) -> String?
    func regionPart(owning object: AnyObject) -> String?
    func regionPart(_ id: String, of node: UInt32) -> PlatformView?
    func regionCoverage(joined: Set<String>) -> [[String: Any]]
}

extension ElementHatching {
    @discardableResult package func later(_ id: UInt32, _ name: String, _ work: @escaping () -> Void) -> Bool { later(id, name, bytes: 0, work) }
    @discardableResult package func later(_ what: String, _ name: String, _ work: @escaping () -> Void) -> Bool { later(what, name, bytes: 0, work) }
}

/// No element hatches: nothing waits, nothing is told, no act is queued.
final class NoElementHatches: ElementHatching {
    func begin(_ batch: Batch) {}
    func created(_ node: NodeView) {}
    func propsChanged(_ id: UInt32) {}
    func destroyed(_ id: UInt32) {}
    func reset() {}
    func replay() {}
    func realized(_ node: NodeView) {}
    func configuring(_ node: NodeView, _ configuration: AnyObject) {}
    func waits(_ node: NodeView) -> Bool { false }
    func releaseConfigurations() {}
    func later(_ id: UInt32, _ name: String, bytes: Int, _ work: @escaping () -> Void) -> Bool { false }
    func later(_ what: String, _ name: String, bytes: Int, _ work: @escaping () -> Void) -> Bool { false }
    func refusedOverBound(_ what: String, _ name: String, bytes: Int) -> Bool { false }
    var inFlight: Int { 0 }
    func drain() {}
    func observation(_ diagnostics: (any HatchDiagnosing)?) -> [String: Any] { [:] }
    #if os(iOS) || os(tvOS)
    func checkOwned() {}
    #endif
    func decorateRegions(_ tree: [String: Any]) -> [String: Any] { tree }
    func regionOwner(of view: PlatformView) -> String? { nil }
    func regionPart(owning object: AnyObject) -> String? { nil }
    func regionPart(_ id: String, of node: UInt32) -> PlatformView? { nil }
    func regionCoverage(joined: Set<String>) -> [[String: Any]] { [] }
}

extension ExactSession {
    /// An iframe's element hatch connected to its web view.
    package func webViewHatched(_ id: UInt32) { webviews.hatched(id) }
}

extension NodeView {
    /// A video's player, which its element hatch is handed (LLP 1133 D3).
    package var videoPlatform: AnyObject? { video?.platform }
    /// Lay a video's view out again after its element hatch changed it.
    package func videoUpdate() { video?.update() }
}

/// What element and toolbar hatches reach of a presenter's own hosts.
extension Presenter {
    /// A node's control or segmented control.
    package func control(of id: UInt32) -> AnyObject? { controls.controls[id] ?? segments.control(of: id) }
    /// Tell the app the text an element hatch wrote into `node`'s field,
    /// which is the text it knows from here.
    package func hatchWrote(_ node: NodeView, _ value: String) {
        node.toldText = value
        if FieldSelections.tracks(node) {
            let end = value.utf16.count, selection = FieldSelection(start: end, end: end)
            fieldSelections.set(node, selection, notify: false)
            typed(node.id, value, input: false)
            fieldSelections.replacementInput(node, value, selection, data: value)
        } else {
            typed(node.id, value, input: node.handlers.contains("input"))
        }
    }
    /// Heavy leaves after a batch that moved views.
    package func leavesMoved() { leaves?.batchApplied(moved: true) }
    #if os(macOS)
    /// The window's toolbar, which toolbar hatches add to.
    package var windowToolbar: NSToolbar? { toolbar.toolbar }
    package func setToolbarAppDelegate(_ delegate: NSToolbarDelegate?) { toolbar.setAppDelegate(delegate) }
    package func addToolbarAppItem(_ item: NSToolbarItem) { toolbar.addAppItem(item) }
    #endif
}

#if os(iOS) || os(tvOS)
/// What route, tab, arrangement and element hatches reach of a presenter's
/// navigation and pool.
extension Presenter {
    /// Whether a development build journals what a hatch changed of a view
    /// Exact owns (LLP 1075.003 §3.5).
    package static var checksHatchOwnership: Bool { RouteGeometry.checksOwnership }
    /// What of `node`'s scroll view changed outside Exact, which owns it.
    package func scrollOwnedChanges(_ node: NodeView) -> [String] {
        guard let scroll = node.scrollView else { return [] }
        return RouteGeometry.ownedChanges(scroll, of: node, insetByUIKit: node.scrollOrigin > 0 || navigation.insetByUIKit(node))
    }
    /// Whether a native list is making `id`'s row now.
    package func listCreating(_ id: UInt32) -> Bool { pool.list(creating: id) != nil }
    /// The node a route hatch names by its route key and id, or nil.
    package func resolveRoute(_ key: String, id: String) -> UInt32? { navigation.resolve(route: key, id: id)?.id }
    /// A live route controller's content scroll view now (`NavigationHosting.contentScrollView(of:)`), the one a
    /// status-bar tap scrolls while the route is active; nil for a controller that is not one of this presenter's
    /// live routes.
    package func routeContentScrollView(of controller: UIViewController) -> UIScrollView? {
        guard let route = controller as? any RouteSurface, views[route.node.id] === route.node else { return nil }
        return navigation.contentScrollView(of: route)
    }
    package func tabIntent(_ kind: UInt32, _ text: String) -> Bool { navigation.tabIntent(kind, text) }
    package func setNavigationAppDelegate(_ controller: AnyObject, _ delegate: AnyObject?) { navigation.setAppDelegate(controller, delegate) }
    #if os(iOS)
    @available(iOS 27.1, *)
    package func setArrangement(_ node: UInt32, controller: UIArrangementViewController?) -> Bool {
        arrangements.set(node, controller: controller)
    }
    #endif
}

extension NodeView {
    /// `found` when it is this route's own controller, else nil.
    package func routeController(in found: UIViewController?) -> UIViewController? {
        (found as? any RouteSurface).flatMap { $0.node === self ? $0 : nil }
    }
}
#endif

/// The slots are set before any session is made, and read on the main thread.
package enum NativeModuleLink {
    /// A session's native module and its views.
    package static var make: (() -> any NativeModuleHosting)?
    /// A presenter's element hatches.
    package static var makeElements: ((Presenter) -> any ElementHatching)?
}
