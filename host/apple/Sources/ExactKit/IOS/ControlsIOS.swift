// Native controls on UIKit: a native button (LLP 1069.011), and what every
// control kind shares: the registry, faces, parking and measuring. The other
// kinds (LLP 1069.001: switches, checkboxes, radios, ranges, selects, dates,
// progress) are `ControlLink`'s, reached through `family`.
#if os(iOS) || os(tvOS)
import UIKit

package final class ControlHost: NSObject {
    package unowned let presenter: Presenter
    package var controls: [UInt32: UIControl] = [:]
    /// Which control each node shows (`ControlKinds`), to remake it when that changes.
    package var kinds: [UInt32: String] = [:]
    /// The size last reported per control, so each is published once.
    private var reported: [UInt32: CGSize] = [:]
    /// Sizes measured and not yet reported (`reportMeasured`).
    private var measured: [(UInt32, CGSize?)] = []
    /// A select's menu as last built, so a batch that leaves it alone does not rebuild it.
    package var menus: [UInt32: SelectMenu] = [:]
    /// A range's last reported value while it moves, so each is sent once.
    package var lastRange: [UInt32: String] = [:]
    /// A range's bound value last written into it: written again only when
    /// it changes (LLP 1069.001 D4, amended 2026-10-04).
    package var appliedRange: [UInt32: String] = [:]
    /// A radio's group from the kernel (`exact_radio_group`; x2apps survey #2).
    var radioGroup: ((UInt32) -> RadioGroup)?
    /// A painted group's keys from the kernel (`exact_role_keys`; LLP 1166.000 D2).
    var roleKeys: ((_ view: UInt32, _ key: String?, _ focused: UInt32?) -> RoleKeys?)?
    /// A select's choice the bound value has not caught up with yet.
    package var picked: [UInt32: String] = [:]
    /// Each `progress`'s activity indicator (ProgressIOS.swift): a view,
    /// not a control, so beside `controls`.
    package var spinners: [UInt32: UIActivityIndicatorView] = [:]
    /// Each determinate `progress`'s bar (LLP 1116 D8), beside them.
    package var bars: [UInt32: UIProgressView] = [:]
    /// Each native button's face as the runner last gave it: what its
    /// control shows, and what `NodeView.face` tells every other reader (a
    /// tab bar or navigation bar item, a menu row, an accessibility label).
    /// A face is the button's viewless contents, which change only in a
    /// batch that says so (`Batch.controls`), and its own props, which
    /// change only in a batch that touches it: `Presenter.apply` drops it as
    /// that batch or op arrives, and the next read asks the runner. Every
    /// other batch keeps it, so a commit that changes no button waits on
    /// the owner thread for none.
    private var faces: [UInt32: ButtonFace] = [:]
    func face(_ id: UInt32) -> ButtonFace {
        if let face = faces[id] { return face }
        let face = presenter.buttonFace?(id) ?? ButtonFace()
        faces[id] = face
        return face
    }

    init(_ presenter: Presenter) { self.presenter = presenter }

    /// The kinds other than a native button.
    lazy var family: any ControlKinding = ControlLink.make?(self) ?? NoControlKinds()

    /// The control the node shows, made (or remade, when its kind changes).
    private func control(for node: NodeView) -> UIControl {
        let kind = ControlKinds.kind(node.props)
        if let existing = controls[node.id], kinds[node.id] == kind { return existing }
        // A row the pool reused brings its button back (`park`).
        if kind == "button", controls[node.id] == nil,
           let kept = node.subviews.lazy.compactMap({ $0 as? NativeButtonIOS }).first(where: { $0.owner === node }) {
            kept.tag = Int(node.id)
            controls[node.id] = kept
            kinds[node.id] = kind
            #if os(iOS)
            presenter.menus.adoptNativeMenu(kept, of: node.id)
            #endif
            return kept
        }
        controls.removeValue(forKey: node.id)?.removeFromSuperview()
        menus.removeValue(forKey: node.id)
        appliedRange.removeValue(forKey: node.id)
        let made: UIControl
        if kind == "button" { made = makeNativeButton(node) } // LLP 1069.011
        else { made = family.make(kind, for: node) }
        made.tag = Int(node.id)
        // Its natural size follows text size, weight and scale, which no batch says.
        MainActor.assumeIsolated {
            made.registerForTraitChanges([UITraitPreferredContentSizeCategory.self, UITraitLegibilityWeight.self, UITraitDisplayScale.self, UITraitUserInterfaceStyle.self, UITraitAccessibilityContrast.self]) { [weak self] (_: UIControl, _: UITraitCollection) in
                self?.presenter.requestProjectionSync()
            }
        }
        controls[node.id] = made
        kinds[node.id] = kind
        return made
    }

    /// Whether a native button parks with its row (NodePool) rather than
    /// going as a heavy leaf: drawn in its node, not glass, showing no menu,
    /// and not under a touch. A `UIButton` holds no value, and its action
    /// reaches whichever node owns it, so nothing of one row reaches the
    /// next. A button leaving the window and a new one configured cost a
    /// list's new row more than the rest of it. A pull-down keeps its menu,
    /// built again for the next row (`MenuHost.adoptNativeMenu`). One not
    /// made yet (in a closed popover, `unshown`) parks as its bare node,
    /// the shape the next row's create ops give it (`NodePool.take`).
    func parks(_ node: NodeView) -> Bool {
        guard node.isNativeButton else { return false }
        guard let b = controls[node.id] as? NativeButtonIOS else {
            return controls[node.id] == nil && !node.subviews.contains { $0 is UIControl }
        }
        guard b.owner === node, b.superview === node, !b.isGlass, !b.isTracking, !b.isHighlighted else { return false }
        #if os(iOS)
        return b.menuAtRest
        #else
        return true
        #endif
    }
    /// `id`'s node parks: its button stays in it, out of every map, until the
    /// node's next id takes it back (`control(for:)`).
    func park(_ id: UInt32) {
        #if os(iOS)
        presenter.menus.parkNativeMenu(id)
        #endif
        controls.removeValue(forKey: id)
        reported.removeValue(forKey: id); faces.removeValue(forKey: id); kinds.removeValue(forKey: id)
        menus.removeValue(forKey: id); lastRange.removeValue(forKey: id); appliedRange.removeValue(forKey: id)
        picked.removeValue(forKey: id)
    }

    /// Whether a native button's control could not show: it is in a closed
    /// `dialog` or popover (`MenuHost.inClosedBox`), which UIKit's menu and
    /// alert present from its face, or in what a native bar stands for (a
    /// lifted header, an adopted tablist: `NavigationHost.covers`), whose
    /// items are made from faces too, while the host hides it: the cover
    /// leaves after the box shows again. It gets one when that changes, as
    /// a field in a closed box waits for it to show.
    private func unshown(_ node: NodeView) -> Bool {
        if presenter.menus.inClosedBox(node) { return true }
        let covers = presenter.navigation.covers
        // A bar's hosted item (LLP 1141.004 D2) is in the bar's own view, outside the header it replaced.
        let replaced = { (cover: HostCover?) in cover == .whole || cover == .bar }
        guard covers.values.contains(where: replaced) else { return false }
        var at: UIView? = node
        while let view = at {
            if let box = view as? NodeView, replaced(covers[box.id]), box.hiddenByHost { return true }
            at = view.superview
        }
        return false
    }

    /// The native buttons the last sync left without a control (`unshown`).
    private var waiting: Set<UInt32> = []
    /// A closed box now shows in Exact's views (the agent's top layer, a
    /// sheet, a context menu's preview): its buttons get their controls now,
    /// whether or not a batch laid it out.
    package func syncShown() {
        if waiting.contains(where: { presenter.views[$0].map { !unshown($0) } ?? false }) { sync(contents: false) }
    }

    /// Asks for the face of `id` again at its next read.
    func faceChanged(_ id: UInt32) { faces.removeValue(forKey: id) }
    /// A radio's group, as the runtime gives it now: iOS stages no batch.
    func group(_ id: UInt32) -> RadioGroup? { radioGroup?(id) }
    /// Asks for every face again at its next read.
    func facesChanged() { faces.removeAll() }

    /// A native button's configuration stores colours made from the owner's tint. A value control
    /// stores a share as its own tint. Refresh them when that tint changes, except while UIKit dims
    /// it, or after a leaving control's owner has gone.
    func tintChanged(_ owner: NodeView) {
        guard owner.tintAdjustmentMode != .dimmed, presenter.views[owner.id] === owner,
              let control = controls[owner.id] else { return }
        let face = face(owner.id)
        guard owner.style["accent_color"]?.namesTint == true || [face.rows.button, face.rows.title, face.rows.subtitle, face.rows.symbol]
            .contains(where: { $0.values.contains(where: \.namesTint) }) else { return }
        if let b = control as? NativeButtonIOS { configureNative(b, owner, accent: owner.carriedAccent) }
        else { family.configure(control, owner, checked: nil) }
    }

    /// `contents`: a sync outside any batch, which reads every face again;
    /// a batch has already dropped the faces it changes (`Presenter.apply`).
    /// `changed`, `touched`: after a batch, the views it touched with their
    /// ancestors, and those whose props or style it changed or that it put
    /// under another parent; a control nothing at or in changed, nor at or
    /// above touched, stands as the last sync left it (`stands`).
    package func sync(contents: Bool = true, changed: Set<UInt32>? = nil, touched: Set<UInt32> = []) {
        if contents { faces.removeAll() }
        // A face read with no control (a tab's, a menu row's) goes with its view.
        for id in faces.keys where controls[id] == nil && presenter.views[id] == nil { faces.removeValue(forKey: id) }
        let all = ControlKinds.indexed.flatMap { presenter.carrying($0) }.filter { $0.kind == "control" }
        let owners = all.filter { !($0.isNativeButton && unshown($0)) }
        let live = Set(owners.map(\.id))
        waiting = Set(all.map(\.id)).subtracting(live)
        // A leaving control keeps drawing until its exit ends (LLP 1069.011 D9).
        let leaving = Set(presenter.leaving.values.flatMap { $0.members.map(\.id) })
        for id in Array(controls.keys) where !live.contains(id) && !leaving.contains(id) {
            controls.removeValue(forKey: id)?.removeFromSuperview()
            reported.removeValue(forKey: id)
            faces.removeValue(forKey: id)
            kinds.removeValue(forKey: id)
            menus.removeValue(forKey: id)
            lastRange.removeValue(forKey: id)
            appliedRange.removeValue(forKey: id)
            picked.removeValue(forKey: id)
        }
        var sizes: [(UInt32, CGSize?)] = []
        for owner in owners {
            if let changed, stands(owner, changed, touched) { continue }
            let native = owner.style["appearance"]?.string != "none"
            let control = control(for: owner)
            if !native {
                // The author's box is the look; the node keeps its role.
                control.removeFromSuperview()
                continue
            }
            let mount = owner.controlMount
            if control.superview !== mount { mount.addSubview(control) }
            if control.isHidden != owner.cssVisibilityHidden { control.isHidden = owner.cssVisibilityHidden }
            if let b = control as? NativeButtonIOS { configureNative(b, owner, accent: owner.carriedAccent) }
            else { family.configure(control, owner, checked: owner.props["checked"].map { $0 == "true" }) }
            if !(control is NativeButtonIOS) {
                assign(control, \.isEnabled, !owner.disabled)
                assign(control, \.accessibilityLabel, owner.controlLabel)
                // An authored label replaces UIKit's title on a pop-up button, so expose the chosen option as its value.
                if owner.props["type"] == "select" { assign(control, \.accessibilityValue, selectValue(for: owner)) }
                assign(control, \.accessibilityIdentifier, owner.props["testId"])
            }
            // A native button's box is its frame (below): its natural size is
            // the kernel's, from the measurement, and asked here of none.
            var natural = control is NativeButtonIOS ? .zero : family.naturalSize(control, owner)
            let box = control is NativeButtonIOS ? owner.bounds : owner.contentBox()
            #if os(iOS)
            // A select its box holds narrower than its natural size wraps its
            // title and grows taller, as a pop-up button does under Auto Layout,
            // rather than running past its box (unit-converter at the largest
            // text size, round 22). Its intrinsic width stays unchanged.
            if owner.props["type"] == "select", let button = control as? UIButton, box.width > 0, box.width < natural.width - 0.5 {
                natural.height = max(natural.height, family.selectHeight(button, width: box.width))
            }
            let fillsSelect = owner.props["type"] == "select"
            #else
            let fillsSelect = false
            #endif
            if fillsSelect {
                assign(control, \.frame, box)
            } else if control is NativeButtonIOS {
                // The box is the button's alignment rect, as its natural size is.
                let frame = control.frame(forAlignmentRect: box)
                if control.frame != frame { control.frame = frame }
            } else {
                #if os(tvOS)
                let width = natural.width
                #else
                let width = control is UISlider ? box.width : natural.width
                #endif
                var x = box.midX - width / 2
                #if os(iOS)
                if control is UIDatePicker {
                    switch family.valueAlignment(owner) {
                    case .left: x = box.minX
                    case .right: x = box.maxX - width
                    default: break
                    }
                }
                #endif
                assign(control, \.frame, CGRect(x: x, y: box.midY - natural.height / 2,
                                                width: width, height: natural.height))
            }
            if !(control is NativeButtonIOS), reported[owner.id] != natural {
                reported[owner.id] = natural
                sizes.append((owner.id, natural))
            }
        }
        if !sizes.isEmpty {
            measured += sizes
            DispatchQueue.main.async { [weak self] in self?.presenter.session?.reportMeasured() }
        }
        family.syncProgress()
    }

    /// Sizes measured and not yet reported.
    var owesSizes: Bool { !measured.isEmpty }

    /// Lays out with the sizes measured since the last report, when the session
    /// says (`ExactSession.reportMeasured`): once the batch being applied is,
    /// before its frame shows, so a box under a layout transition never shows
    /// where the default size put it (#134); else, for a sync outside a batch,
    /// on the next turn or once the owner's earlier batches land. A control
    /// destroyed before then reports nothing. A layout can size a control
    /// again (a select's wrapped title): a few rounds, as it settles.
    func reportMeasured() {
        for _ in 0..<3 where !measured.isEmpty {
            var latest: [UInt32: CGSize?] = [:]
            for (id, size) in measured where controls[id] != nil && presenter.views[id] != nil { latest[id] = size }
            measured = []
            if !latest.isEmpty { presenter.onIntrinsic?(latest.map { ($0.key, $0.value) }) }
        }
    }

    /// The chosen option's label when an authored `aria-label`/`aria-labelledby`
    /// replaces the pop-up button's spoken title; nil otherwise so VoiceOver does
    /// not read the choice twice.
    func selectValue(for owner: NodeView) -> String? {
        guard owner.controlLabel != nil, let menu = menus[owner.id] else { return nil }
        let chosen = picked[owner.id].flatMap { value in menu.options.firstIndex { $0.value == value } } ?? menu.chosen
        guard let chosen, menu.options.indices.contains(chosen) else { return nil }
        return menu.options[chosen].label
    }

    /// Whether a batch left `owner`'s control as it was: neither the owner
    /// nor anything in it changed (each touched view's ancestors are in
    /// `changed`; a button's face stands), nothing at or above it was
    /// restyled or moved (an inherited disabled or a closing box), and it
    /// is mounted where it was. A live list's tick then configures its
    /// changed rows' controls, not every row's.
    private func stands(_ owner: NodeView, _ changed: Set<UInt32>, _ touched: Set<UInt32>) -> Bool {
        guard !changed.contains(owner.id), !touched.contains(owner.id), let control = controls[owner.id],
              control.superview === owner.controlMount,
              !(control is NativeButtonIOS) || faces[owner.id] != nil else { return false }
        // A grouped toggle row's switch is named by the row's words (`controlLabel`).
        if let row = owner.groupedToggleRow, ([row] + row.container.subviews.compactMap { $0 as? NodeView })
            .contains(where: { changed.contains($0.id) || touched.contains($0.id) }) { return false }
        var at = presenter.accessibilityParent(of: owner)
        while let view = at {
            if let node = view as? NodeView, touched.contains(node.id) { return false }
            at = presenter.accessibilityParent(of: view)
        }
        return true
    }

    #if EXACT_AGENT
    /// The agent's `tap` (LLP 1069.001 D9): the control's own activation.
    package func activate(_ node: NodeView, before: (CGPoint?) -> Bool = { _ in true }) -> Bool? {
        // A native button takes the ordinary tap path (LLP 1069.011 D10).
        guard let control = controls[node.id], !(control is NativeButtonIOS) else { return nil }
        guard control.window != nil, control.isEnabled, !node.inert else { return false }
        let point = control.convert(CGPoint(x: control.bounds.midX, y: control.bounds.midY), to: nil)
        guard before(point), controls[node.id] === control, control.window != nil, control.isEnabled, !node.disabled, !node.inert else { return false }
        return family.activate(control, node)
    }

    func observation(_ node: NodeView) -> [String: Any]? {
        if let progress = family.progressObservation(node) { return progress }
        guard let control = controls[node.id] else { return nil }
        if let b = control as? NativeButtonIOS { return nativeObservation(b) }
        return family.observation(control)
    }
    #endif

    func reset() {
        for control in controls.values { control.removeFromSuperview() }
        controls.removeAll()
        for spinner in spinners.values { spinner.removeFromSuperview() }
        spinners.removeAll()
        for bar in bars.values { bar.removeFromSuperview() }
        bars.removeAll()
        reported.removeAll()
        kinds.removeAll()
        menus.removeAll()
        lastRange.removeAll()
        appliedRange.removeAll()
        picked.removeAll()
        faces.removeAll()
    }
}

extension NodeView {
    /// A control's accessible name: its own; but where a grouped list shows its authored nodes
    /// (tvOS, and any list UIKit does not draw), the switch or checkbox that ends one of its rows
    /// is the row's one element, as UIKit's list cell is on iOS (`GroupedCell.toggle`), named by
    /// the row's label, else its own, else the row's words.
    var controlLabel: String? {
        guard let row = groupedToggleRow else { return authoredLabel }
        let name = row.authoredLabel ?? authoredLabel ?? row.accessibleName
        return name.isEmpty ? nil : name
    }

    /// The row of a grouped list (`listStyle`) whose trailing switch or checkbox this is: the
    /// last child of a row of a section's group, as the kernel reads a toggle row
    /// (`Kernel::grouped_toggle`).
    var groupedToggleRow: NodeView? {
        guard kind == "control", ["switch", "checkbox"].contains(ControlKinds.kind(props)) else { return nil }
        func parent(_ view: UIView?) -> NodeView? {
            var at = view?.superview
            while let v = at { if let node = v as? NodeView { return node }; at = v.superview }
            return nil
        }
        guard let row = parent(self), let group = parent(row), !["header", "footer"].contains(group.props["semanticTag"]),
              let list = parent(parent(group)), list.kind == "list", list.props["listStyle"] != nil,
              row.container.subviews.last(where: { $0 is NodeView }) === self else { return nil }
        return row
    }
}

/// Exact writes what it owns on a UIKit object only when the value changes
/// (LLP 1075.003 §3.5, from James's review): a control set again to what it
/// already shows can restart its own animation — a Liquid Glass switch's
/// thumb wobbled when every batch re-set its colour, frame and state.
@inline(__always) package func assign<O: AnyObject, V: Equatable>(_ object: O, _ key: ReferenceWritableKeyPath<O, V>, _ value: V) {
    if object[keyPath: key] != value { object[keyPath: key] = value }
}
#endif
