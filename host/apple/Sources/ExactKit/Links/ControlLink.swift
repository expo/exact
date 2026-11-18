// @ref LLP 1047.001 D10 — the optional form controls (the roster's
// `controls`: switches, checkboxes, radios, ranges, selects, dates and
// progress) are a family the composition selects. Native buttons are not:
// `ControlHost` keeps them, its registry, faces, parking and measuring, and
// reaches every other kind only through what this file declares; the
// presenter reaches segmented tablists the same way. Without the
// family it holds `NoControlKinds`, which makes a bare control that shows and
// reports nothing, so a plan of one text and one button links no date picker.
// The slot's default is today's built-in kinds, so every app still links
// them; the family's own target will install them instead.
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
    /// The agent's tap on the control: its own activation.
    func activate(_ control: PlatformControl, _ node: NodeView) -> Bool
    func observation(_ control: PlatformControl) -> [String: Any]
    /// Each `progress`'s indicator, made and updated, and what one shows.
    func syncProgress()
    func progressObservation(_ node: NodeView) -> [String: Any]?
    /// Whether a radio's control shows it checked, and showing it so.
    func radioOn(_ control: PlatformControl) -> Bool
    func showRadio(_ id: UInt32, _ on: Bool)
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
}

#if os(iOS)
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
    func observation(_ node: NodeView) -> [String: Any]?
    /// The inspection walk's accounting.
    func inspectionOwns(_ view: PlatformView) -> Bool
    func hides(_ node: NodeView) -> Bool
    #if os(macOS)
    func control(of id: UInt32) -> NSSegmentedControl?
    func activate(_ node: NodeView) -> Bool?
    /// Whether a segment shows `node`, when one is its own.
    func shown(_ node: NodeView) -> Bool?
    #else
    func control(of id: UInt32) -> UISegmentedControl?
    func bar(of id: UInt32) -> UITabBar?
    /// Whether a native tablist shows `id`.
    func projects(_ id: UInt32) -> Bool
    func barItem(of node: NodeView) -> (bar: UITabBar, item: UITabBarItem)?
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
    func activate(_ node: NodeView) -> Bool? { activate(node, before: { _ in true }) }
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
    package static var makeSegments: ((Presenter) -> any SegmentHosting)? = { SegmentHost($0) }
    /// A control host's optional kinds.
    package static var make: ((ControlHost) -> any ControlKinding)? = { BuiltInControlKinds($0) }
    /// The agent's reading of a drawn checkbox or radio: whether `object` is
    /// one, a radio, and on.
    package static var drawnToggle: ((AnyObject) -> (radio: Bool, on: Bool)?)? = BuiltInControlKinds.drawnToggle
}
