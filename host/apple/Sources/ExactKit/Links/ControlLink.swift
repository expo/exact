// @ref LLP 1047.001 D10 — the optional form controls (the roster's
// `controls`: switches, checkboxes, radios, ranges, selects, dates and
// progress) are a family the composition selects. Native buttons are not:
// `ControlHost` keeps them, its registry, faces, parking and measuring, and
// reaches every other kind only through what this file declares; the
// presenter reaches segmented tablists the same way. Without the
// family it holds `NoControlKinds`, which makes a bare control that shows and
// reports nothing, so a plan of one text and one button links no date picker.
// `ExactControls.install()` sets the slots.
#if os(macOS)
import AppKit
package typealias PlatformControl = NSControl
#else
import UIKit
package typealias PlatformControl = UIControl
#endif

/// What `ControlHost` asks of the control kinds other than a native button.
package protocol ControlKinding: AnyObject {
    /// The control of `kind` for `node`, its actions aimed at the host.
    func make(_ kind: String, for node: NodeView) -> PlatformControl
    /// Shows `owner`'s props: its value, accent and availability, and
    /// `checked` unless nil (which leaves a switch's or checkbox's state alone).
    func configure(_ control: PlatformControl, _ owner: NodeView, checked: Bool?)
    func naturalSize(_ control: PlatformControl, _ owner: NodeView) -> CGSize
    #if os(iOS)
    /// A select's height when its box holds it narrower than its natural width.
    func selectHeight(_ button: UIButton, width: CGFloat) -> CGFloat
    /// A date picker's alignment in its box.
    func valueAlignment(_ owner: NodeView) -> UIControl.ContentHorizontalAlignment
    #endif
    /// Each `progress`'s indicator, made and updated.
    func syncProgress()
    #if os(macOS)
    /// Apply a resolved accent without changing the control's value or interrupting tracking.
    func applyAccent(_ accent: NSColor?, to control: NSControl)
    #endif
    /// Whether a radio's control shows it checked, and showing it so.
    func radioOn(_ control: PlatformControl) -> Bool
    func showRadio(_ id: UInt32, _ on: Bool)
    // The agent's alone, so a production build keeps none of them: a
    // requirement keeps its implementation alive through the witness table.
    #if EXACT_AGENT
    /// The agent's tap on the control: its own activation.
    func activate(_ control: PlatformControl, _ node: NodeView) -> Bool
    func observation(_ control: PlatformControl) -> [String: Any]
    /// What a `progress` shows.
    func progressObservation(_ node: NodeView) -> [String: Any]?
    /// The agent's `type` into a value control, and the reason a control it
    /// cannot open refuses; nil where the node is no such control.
    func type(_ node: NodeView, _ value: String) -> [String: Any]?
    #if os(iOS) || os(tvOS)
    func unopened(_ node: NodeView) -> [String: Any]?
    #endif
    #if os(iOS)
    /// A key on a range (the agent's `key`).
    func rangeKey(_ node: NodeView, _ key: String, phase: String?, repeats: Bool) -> [String: Any]?
    #endif
    #endif
}

#if os(iOS) && EXACT_AGENT
extension ControlKinding {
    func rangeKey(_ node: NodeView, _ key: String, phase: String? = nil) -> [String: Any]? { rangeKey(node, key, phase: phase, repeats: false) }
}
#endif

/// No optional controls: a bare control that draws nothing and measures zero.
package final class NoControlKinds: ControlKinding {
    package init() {}
    package func make(_ kind: String, for node: NodeView) -> PlatformControl { PlatformControl() }
    package func configure(_ control: PlatformControl, _ owner: NodeView, checked: Bool?) {}
    package func naturalSize(_ control: PlatformControl, _ owner: NodeView) -> CGSize { .zero }
    #if os(iOS)
    package func selectHeight(_ button: UIButton, width: CGFloat) -> CGFloat { 0 }
    package func valueAlignment(_ owner: NodeView) -> UIControl.ContentHorizontalAlignment { .center }
    #endif
    package func activate(_ control: PlatformControl, _ node: NodeView) -> Bool { false }
    package func observation(_ control: PlatformControl) -> [String: Any] { [:] }
    package func syncProgress() {}
    #if os(macOS)
    package func applyAccent(_ accent: NSColor?, to control: NSControl) {}
    #endif
    package func progressObservation(_ node: NodeView) -> [String: Any]? { nil }
    package func radioOn(_ control: PlatformControl) -> Bool { false }
    package func showRadio(_ id: UInt32, _ on: Bool) {}
    package func type(_ node: NodeView, _ value: String) -> [String: Any]? { nil }
    #if os(iOS) || os(tvOS)
    package func unopened(_ node: NodeView) -> [String: Any]? { nil }
    #endif
    #if os(iOS)
    package func rangeKey(_ node: NodeView, _ key: String, phase: String?, repeats: Bool) -> [String: Any]? { nil }
    #endif
}

/// What the core asks of a presenter's segmented tablists and radio groups
/// (`SegmentHost`). The segmented-control classes navigation shows are the
/// core's (SegmentedControlIOS.swift).
package protocol SegmentHosting: AnyObject {
    func sync()
    func reset()
    #if EXACT_AGENT
    func observation(_ node: NodeView) -> [String: Any]?
    /// The inspection walk's accounting.
    func inspectionOwns(_ view: PlatformView) -> Bool
    func hides(_ node: NodeView) -> Bool
    #endif
    #if os(macOS)
    func control(of id: UInt32) -> NSSegmentedControl?
    #if EXACT_AGENT
    func activate(_ node: NodeView) -> Bool?
    #endif
    /// Whether a segment shows `node`, when one is its own.
    func shown(_ node: NodeView) -> Bool?
    #else
    func control(of id: UInt32) -> UISegmentedControl?
    /// Whether a native tablist shows `id`.
    func projects(_ id: UInt32) -> Bool
    #if EXACT_AGENT
    func bar(of id: UInt32) -> UITabBar?
    func barItem(of node: NodeView) -> (bar: UITabBar, item: UITabBarItem)?
    #endif
    #if EXACT_INSPECT
    func activate(_ node: NodeView, before: (CGPoint?) -> Bool) -> Bool?
    func drawn(_ node: NodeView) -> (control: UIControl, segment: CGRect)?
    #endif
    #if EXACT_INSPECT && os(iOS)
    func tab(at point: CGPoint, in view: UIView) -> UInt32?
    #endif
    #endif
}

#if EXACT_INSPECT && !os(macOS)
extension SegmentHosting {
    package func activate(_ node: NodeView) -> Bool? { activate(node, before: { _ in true }) }
}
#endif

/// No segmented tablists: the author's nodes draw.
package final class NoSegments: SegmentHosting {
    package init() {}
    package func sync() {}
    package func reset() {}
    package func observation(_ node: NodeView) -> [String: Any]? { nil }
    package func inspectionOwns(_ view: PlatformView) -> Bool { false }
    package func hides(_ node: NodeView) -> Bool { false }
    #if os(macOS)
    package func control(of id: UInt32) -> NSSegmentedControl? { nil }
    package func activate(_ node: NodeView) -> Bool? { nil }
    package func shown(_ node: NodeView) -> Bool? { nil }
    #else
    package func control(of id: UInt32) -> UISegmentedControl? { nil }
    package func bar(of id: UInt32) -> UITabBar? { nil }
    package func projects(_ id: UInt32) -> Bool { false }
    package func barItem(of node: NodeView) -> (bar: UITabBar, item: UITabBarItem)? { nil }
    #if EXACT_INSPECT
    package func activate(_ node: NodeView, before: (CGPoint?) -> Bool) -> Bool? { nil }
    package func drawn(_ node: NodeView) -> (control: UIControl, segment: CGRect)? { nil }
    #endif
    #if EXACT_INSPECT && os(iOS)
    package func tab(at point: CGPoint, in view: UIView) -> UInt32? { nil }
    #endif
    #endif
}

/// The slots are set before any session is made, and read on the main thread.
package enum ControlLink {
    /// A presenter's segmented tablists and radio groups.
    package static var makeSegments: ((Presenter) -> any SegmentHosting)?
    /// A control host's optional kinds.
    package static var make: ((ControlHost) -> any ControlKinding)?
    #if EXACT_AGENT
    /// The agent's reading of a drawn checkbox or radio: whether `object` is
    /// one, a radio, and on.
    package static var drawnToggle: ((AnyObject) -> (radio: Bool, on: Bool)?)?
    #endif
}

#if os(iOS) || os(tvOS)
/// What the segment host and the value controls read of the presenter's
/// navigation and flat leaves, without reaching those hosts' types.
extension Presenter {
    /// Whether a tab container's bar takes `tablist`'s place.
    package func navigationAdopts(tablist: NodeView) -> Bool { navigation.adopts(tablist: tablist) }
    /// Whether a search bar's scope shows `tablist`.
    package func navigationScopes(tablist: NodeView) -> Bool { navigation.scopes(tablist: tablist) }
    #if EXACT_INSPECT
    /// The bar segment showing `tab`, when a route's bar shows it.
    package func navigationBarSegment(_ tab: NodeView) -> (control: UIControl, segment: CGRect)? { navigation.barSegment(tab) }
    #endif
    /// `parent`'s children's frames as laid out, flat leaves' included, in
    /// order, while its children are flat; nil otherwise. A child not shown
    /// (`shown` false, or a leaf of no area) is nil.
    package func laidOutChildren(of parent: UInt32, shown: (NodeView) -> Bool) -> [(id: UInt32, frame: CGRect?)]? {
        guard let ids = flats.children(of: parent) else { return nil }
        return ids.map { id in
            if let leaf = flats.leaves[id] { return (id, leaf.layout.width > 0 && leaf.layout.height > 0 ? leaf.layout : nil) }
            return (id, views[id].flatMap { shown($0) ? $0.frame : nil })
        }
    }
}
#endif

#if os(macOS)
extension Presenter {
    /// Whether the sidebar or the window toolbar shows `tablist` in its place.
    package func chromeAdopts(tablist: NodeView) -> Bool { sidebar.adopts(tablist) || toolbar.adopts(tablist) }
}
#endif
