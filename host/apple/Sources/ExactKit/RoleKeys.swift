// A group Exact paints in the page takes keys (LLP 1166.000): a `radiogroup`
// of radios and a `tablist` of tabs that no segmented control, sidebar,
// toolbar or tab bar stands in for (a vertical one, or one
// `appearance: none`), each one Tab stop, and a `menu` whose buttons, links
// and `menuitem`s are in the page, which sets no stop. Which members a group
// has, which of them a key can reach, where an arrow goes, whether it
// presses and which member is the stop are the kernel's (`exact_role_keys`).
// The kernel answers in order and this host takes the first member it can
// focus: its views are the main thread's and the kernel runs on the
// owner's. The host keeps what D3 gives it: which views it can focus, how
// the focus is given, pressing through its own button path, and a menu
// NSMenu or UIMenu shows, whose keys are theirs (its views are not in the
// window to take one). On tvOS the focus engine moves by geometry, and no
// group has keys.
import Foundation
#if canImport(AppKit)
import AppKit
#else
import UIKit
#endif

/// The kernel's answer for a member of a group with keys (`exact_role_keys`).
struct RoleKeys {
    /// The member's group's Tab stop.
    struct Stop {
        var group: UInt32
        var role: String
        /// A member has the focus: the first of `order` is the stop whatever
        /// the host can focus (none when that member's `tabindex` is negative).
        var held: Bool
        /// The members that can be the stop, in the order they are tried.
        var order: [UInt32]
    }
    /// The move a key makes at the member, in the group whose key it is.
    struct Step {
        var group: UInt32
        var role: String
        /// The members the key moves to, in the order they are tried.
        var to: [UInt32]
        /// Whether the move presses the member it reaches.
        var press: Bool
    }
    /// Nil for a group that sets no stop (a menu).
    var stop: Stop?
    /// Nil for no key, or a key that is not the group's.
    var step: Step?

    init?(json: Data) {
        guard let o = try? JSONSerialization.jsonObject(with: json) as? [String: Any] else { return nil }
        let ids = { (value: Any?) in (value as? [Int] ?? []).map { UInt32($0) } }
        if let s = o["stop"] as? [String: Any], let group = s["group"] as? Int, let role = s["role"] as? String {
            stop = Stop(group: UInt32(group), role: role, held: s["held"] as? Bool ?? false, order: ids(s["order"]))
        }
        if let s = o["step"] as? [String: Any], let group = s["group"] as? Int, let role = s["role"] as? String {
            step = Step(group: UInt32(group), role: role, to: ids(s["to"]), press: s["press"] as? Bool ?? false)
        }
        if stop == nil, step == nil { return nil }
    }
}

extension NodeView {
    /// The kernel's answer for this node with `key` pressed: nil for a node
    /// that is no member of a group with keys, and on tvOS. Only a node that
    /// can be one is asked (`mayJoinGroup`).
    func roleKeys(_ key: String? = nil) -> RoleKeys? {
        #if os(tvOS)
        return nil
        #else
        guard mayJoinGroup, let presenter else { return nil }
        return presenter.roleKeys(id, key)
        #endif
    }

    /// Whether this node can be a member of a group with keys, by the
    /// members the schema's `roles` rows take: a radio or a tab, and inside
    /// an authored `role="menu"` a button, a link or a `menuitem`. An
    /// `input type="radio"` is none: its group is HTML's, by `name`
    /// (`radioKey`).
    private var mayJoinGroup: Bool {
        if isRadio { return false }
        let role = props["accessibilityRole"] ?? ""
        if role == "radio" || role == "tab" { return true }
        guard isButton || props["href"] != nil || role.hasPrefix("menuitem"), let presenter else { return false }
        return presenter.inMenu(presenter.eventParent(of: self))
    }

    /// The group whose Tab stop this node's is when Exact paints the group:
    /// nil for a node that is no member, a member of a group that sets no
    /// stop (a menu), and one of a group the host draws as a segmented
    /// control.
    var paintedGroup: (stop: RoleKeys.Stop, owner: NodeView)? {
        guard let stop = roleKeys()?.stop, presenter?.paints(stop.group, stop.role) == true,
              let owner = presenter?.views[stop.group] else { return nil }
        return (stop, owner)
    }

    /// Whether this member of a painted group is its one Tab stop; nil for a
    /// node that is none, whose stop is its kind's.
    var roleTabStop: Bool? {
        guard let stop = paintedGroup?.stop, let controls = presenter?.controls else { return nil }
        return (stop.held ? stop.order.first : stop.order.first(where: controls.canFocusMember)) == id
    }
}

extension Presenter {
    /// The kernel's answer for member `id` (a node or an inline run) with `key` pressed.
    func roleKeys(_ id: UInt32, _ key: String?) -> RoleKeys? { controls.roleKeys?(id, key, focusEvents.focused) }

    /// Whether `view` or one of its authored ancestors is a `role="menu"`.
    func inMenu(_ view: KeyPlatformView?) -> Bool {
        var at = view
        while let view = at {
            if (view as? NodeView)?.props["accessibilityRole"] == "menu" { return true }
            at = eventParent(of: view)
        }
        return false
    }

    /// Whether the group `id` with `role` is one Exact paints: a radio group
    /// or a tablist that no segmented control stands in for (a sidebar, a
    /// toolbar and a tab bar hide the tabs they stand in for), and a menu
    /// whose member took a key, which NSMenu and UIMenu never hand one.
    func paints(_ id: UInt32, _ role: String) -> Bool {
        role == "menu" || (role == "radiogroup" || role == "tablist") && segments.control(of: id) == nil
    }
}

extension ControlHost {
    /// Whether the host can give member `id` the focus, as `focusRadio` gives
    /// it, or on the Mac an inline run's own responder. UIKit has none for
    /// an inline run, so on iOS a link in a line of text is never a member's
    /// destination.
    func canFocusMember(_ id: UInt32) -> Bool {
        #if os(macOS)
        guard let node = presenter.views[id] else { return presenter.inlineKeyView(id)?.acceptsFirstResponder == true }
        return presenter.keyView(of: node).acceptsFirstResponder
        #else
        return presenter.views[id]?.canBecomeFirstResponder == true
        #endif
    }

    /// A key's default action at a focused member of a painted group, after
    /// its `key` handlers (none prevented it): the arrows, and Home and End in
    /// a tablist or a menu, move the focus to the first member the kernel
    /// names that can take it, pressing a radio (its author's action is what
    /// checks it) and no tab or menu item (their press is the app's action,
    /// LLP 1115.000 D4.5).
    /// With none, the key is still the group's and moves nothing. A chord
    /// with Control, Option or Command is not the group's. True when the key
    /// was the group's.
    func roleKey(_ node: NodeView, _ name: String, held: String) -> Bool {
        guard !node.formDisabled, !node.inert, Self.groupKey(name, held) else { return false }
        return move(node.roleKeys(name)?.step)
    }

    #if os(macOS)
    /// `roleKey` at a focused inline run: a link in a line of text inside a menu.
    func roleKey(inline id: UInt32, _ name: String, held: String) -> Bool {
        guard Self.groupKey(name, held), let run = presenter.inlineText(id), !presenter.inlineInert(id),
              run.props["href"] != nil || (run.props["accessibilityRole"] ?? "").hasPrefix("menuitem"),
              presenter.inMenu(presenter.textHost(id)) else { return false }
        return move(presenter.roleKeys(id, name)?.step)
    }
    #endif

    private static func groupKey(_ name: String, _ held: String) -> Bool {
        (held.isEmpty || held == "Shift+") && ["ArrowDown", "ArrowRight", "ArrowUp", "ArrowLeft", "Home", "End"].contains(name)
    }

    /// The kernel's step taken: the focus to the first member the host can
    /// focus, pressed when the step says so.
    private func move(_ step: RoleKeys.Step?) -> Bool {
        guard let step, presenter.paints(step.group, step.role) else { return false }
        guard let to = step.to.first(where: canFocusMember) else { return true }
        if let node = presenter.views[to] {
            focusRadio(node, keyboard: true)
            if step.press { presenter.press(to) }
        }
        #if os(macOS)
        if presenter.views[to] == nil, let run = presenter.inlineKeyView(to) {
            run.window?.makeFirstResponder(run)
            if step.press { _ = presenter.textHost(to)?.activateInline(to) }
        }
        #endif
        return true
    }
}
