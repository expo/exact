// Native controls on AppKit: a native button (LLP 1069.011), and what every
// control kind shares: the registry, faces and measuring. The other kinds
// (LLP 1069.001: switches, checkboxes, radios, ranges, selects, dates,
// progress) are `ControlLink`'s, reached through `family`.
#if os(macOS)
import AppKit

extension NSControl {
    /// The node whose box holds this control.
    package var ownerNode: NodeView? { sequence(first: self as NSView, next: \.superview).lazy.compactMap { $0 as? NodeView }.first }
    package var ariaUnavailableFromNode: Bool { ownerNode?.ariaUnavailable ?? false }
}

package final class ControlHost: NSObject {
    package unowned let presenter: Presenter
    package var controls: [UInt32: NSControl] = [:]
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
    /// Each native button's face, select's options and radio's group as the
    /// last batch applied gave it. The runtime computes them from a control's
    /// props and viewless contents: a batch that changes one carries it as the
    /// runtime made it (`Batch.controlReads`), so what shows is the batches
    /// applied so far, never a batch that has not applied (LLP 1072 §3.5). A
    /// batch the runtime did not make drops the faces it may change, and a
    /// read with none taken asks the runner.
    private var faces: [UInt32: ButtonFace] = [:]
    private var selectMenus: [UInt32: SelectMenu] = [:], radioGroups: [UInt32: RadioGroup] = [:]
    func took(_ reads: [UInt32: ControlRead]) {
        for (id, read) in reads {
            if let face = read.face { faces[id] = ButtonFace(json: face) }
            if let select = read.select { selectMenus[id] = SelectMenu(json: select) }
            if let radio = read.radio { radioGroups[id] = RadioGroup(json: radio) }
        }
    }
    package func selectMenu(_ id: UInt32) -> SelectMenu { selectMenus[id] ?? presenter.selectOptions?(id) ?? SelectMenu() }
    func group(_ id: UInt32) -> RadioGroup? { radioGroups[id] ?? radioGroup?(id) }
    func face(_ id: UInt32) -> ButtonFace {
        if let face = faces[id] { return face }
        let face = presenter.buttonFace?(id) ?? ButtonFace()
        faces[id] = face
        return face
    }
    /// Asks for the face of `id` again at its next read.
    func faceChanged(_ id: UInt32) { faces.removeValue(forKey: id) }
    /// Asks for every face again at its next read.
    func facesChanged() { faces.removeAll() }
    /// A range's bound value last written into it: written again only when
    /// it changes (LLP 1069.001 D4, amended 2026-10-04).
    package var appliedRange: [UInt32: String] = [:]
    /// A radio's group from the kernel (`exact_radio_group`; x2apps survey #2).
    var radioGroup: ((UInt32) -> RadioGroup)?
    /// A painted group's keys from the kernel (`exact_role_keys`; LLP 1166.000 D2).
    var roleKeys: ((_ view: UInt32, _ key: String?, _ focused: UInt32?) -> RoleKeys?)?
    /// Each `progress`'s spinner (ProgressMac.swift): a view, not a
    /// control, so beside `controls`; and those turning, which AppKit does
    /// not say.
    package var spinners: [UInt32: NSProgressIndicator] = [:]
    package var animating: Set<UInt32> = []
    /// Each determinate `progress`'s bar (LLP 1116 D8), beside them.
    package var bars: [UInt32: NSProgressIndicator] = [:]

    init(_ presenter: Presenter) { self.presenter = presenter }

    /// The kinds other than a native button.
    lazy var family: any ControlKinding = ControlLink.make?(self) ?? NoControlKinds()

    package func isOn(_ control: NSControl) -> Bool {
        ((control as? NSSwitch)?.state ?? (control as? NSButton)?.state) == .on
    }

    package func setOn(_ control: NSControl, _ on: Bool) {
        let state: NSControl.StateValue = on ? .on : .off
        if let s = control as? NSSwitch, s.state != state { s.state = state }
        if let b = control as? NSButton, b.state != state { b.state = state }
    }

    /// The control the node shows, made (or remade, when its kind changes).
    private func control(for node: NodeView) -> NSControl {
        let kind = ControlKinds.kind(node.props)
        if let existing = controls[node.id], kinds[node.id] == kind { return existing }
        controls.removeValue(forKey: node.id)?.removeFromSuperview()
        menus.removeValue(forKey: node.id)
        appliedRange.removeValue(forKey: node.id)
        let made: NSControl
        if kind == "button" { // LLP 1069.011
            made = makeNativeButton(node)
            made.target = self
            made.action = #selector(nativePressed(_:))
        } else {
            made = family.make(kind, for: node)
        }
        made.tag = Int(node.id)
        controls[node.id] = made
        kinds[node.id] = kind
        return made
    }

    /// Refresh the colour separately from the value. An appearance change can arrive while AppKit tracks a control.
    package func applyColors(_ control: NSControl, _ owner: NodeView) {
        let accent = owner.cgColor("accent_color").flatMap(NSColor.init(cgColor:))
        if let button = control as? NativeButtonMac {
            configureNative(button, owner, accent: accent)
        } else {
            family.applyAccent(accent, to: control)
        }
    }

    package func sync() {
        let owners = ControlKinds.indexed.flatMap { presenter.carrying($0) }.filter { $0.kind == "control" }
        let live = Set(owners.map(\.id))
        // A leaving control keeps drawing until its exit ends (LLP 1069.011 D9).
        let leaving = Set(presenter.leaving.values.flatMap { $0.members.map(\.id) })
        for id in Array(controls.keys) where !live.contains(id) && !leaving.contains(id) {
            controls.removeValue(forKey: id)?.removeFromSuperview()
            reported.removeValue(forKey: id)
            if presenter.views[id] == nil { faces[id] = nil; selectMenus[id] = nil; radioGroups[id] = nil }
            kinds.removeValue(forKey: id)
            menus.removeValue(forKey: id)
            lastRange.removeValue(forKey: id)
            appliedRange.removeValue(forKey: id)
        }
        var sizes: [(UInt32, CGSize?)] = []
        for owner in owners {
            let control = control(for: owner)
            if owner.style["appearance"]?.string == "none" {
                // The author's box is the look; the node keeps its role.
                control.removeFromSuperview()
                continue
            }
            let mount = owner.controlMount
            if control.superview !== mount { mount.addSubview(control) }
            control.isHidden = owner.cssVisibilityHidden
            // `outline: none` (LLP 1104 D6): AppKit draws the control no ring.
            let ring: NSFocusRingType = owner.outlineNone ? .none : .default
            if control.focusRingType != ring { control.focusRingType = ring; control.noteFocusRingMaskChanged() }
            applyColors(control, owner)
            if !(control is NativeButtonMac) {
                family.configure(control, owner, checked: owner.props["checked"].map { $0 == "true" })
                control.isEnabled = !owner.disabled
                control.setAccessibilityLabel(owner.authoredLabel)
                control.setAccessibilityIdentifier(owner.props["testId"])
            }
            // A native button is sized by its node's box and has answered its fitting size already (D11):
            // its intrinsic size, an AppKit measure on every batch, is read only for the other controls.
            let natural = control is NativeButtonMac ? .zero : family.naturalSize(control, owner)
            let box = control is NativeButtonMac ? owner.bounds : owner.contentBox()
            // A slider's track spans its box, as the web's does; a native
            // button fills it, its chrome inside (LLP 1069.011 D6); the
            // others keep their own size, centred.
            if control is NativeButtonMac {
                // The box is the button's alignment rect, as its natural size
                // is; its bezel's shadow and insets fall outside it.
                let frame = control.frame(forAlignmentRect: box)
                if control.frame != frame { control.frame = frame }
                if let button = control as? NativeButtonMac, let face = button.written?.face {
                    ButtonConfigurationMac.corners(face, to: button, size: box.size)
                }
            } else {
                let width = control is NSSlider ? box.width : natural.width
                control.frame = CGRect(x: box.midX - width / 2, y: box.midY - natural.height / 2,
                                       width: width, height: natural.height)
            }
            // Buttons already received their fitting answer before publication (D11).
            if !(control is NativeButtonMac), reported[owner.id] != natural {
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

    func appearanceChanged(_ owner: NodeView) {
        if let control = controls[owner.id] ?? presenter.segments.control(of: owner.id) {
            applyColors(control, owner)
        }
    }

    /// Sizes measured and not yet reported.
    var owesSizes: Bool { !measured.isEmpty }

    /// Lays out with the sizes measured since the last report, when the session
    /// says (`ExactSession.reportMeasured`): once the batch being applied is,
    /// before its frame shows, so a box under a layout transition never shows
    /// where the default size put it (#134); else, for a sync outside a batch,
    /// on the next turn or once the owner's earlier batches land. A control
    /// destroyed before then reports nothing. A layout can size a control
    /// again: a few rounds, as it settles.
    func reportMeasured() {
        for _ in 0..<3 where !measured.isEmpty {
            var latest: [UInt32: CGSize?] = [:]
            for (id, size) in measured where controls[id] != nil && presenter.views[id] != nil { latest[id] = size }
            measured = []
            if !latest.isEmpty { presenter.onIntrinsic?(latest.map { ($0.key, $0.value) }) }
        }
    }

    #if EXACT_AGENT
    /// The agent's `tap` (LLP 1069.001 D9): the control's own activation.
    package func activate(_ node: NodeView) -> Bool? {
        // A native button takes the ordinary click path (LLP 1069.011 D10).
        guard let control = controls[node.id], !(control is NativeButtonMac) else { return nil }
        guard control.window != nil, control.isEnabled, !node.inert, !control.isHiddenOrHasHiddenAncestor else { return false }
        return family.activate(control, node)
    }

    func observation(_ node: NodeView) -> [String: Any]? {
        if let progress = family.progressObservation(node) { return progress }
        guard let control = controls[node.id] else { return nil }
        if let b = control as? NativeButtonMac { return nativeObservation(b) }
        return family.observation(control)
    }
    #endif

    func reset() {
        for control in controls.values { control.removeFromSuperview() }
        controls.removeAll()
        for spinner in spinners.values { spinner.stopAnimation(nil); spinner.removeFromSuperview() }
        spinners.removeAll()
        for bar in bars.values { bar.removeFromSuperview() }
        bars.removeAll()
        animating.removeAll()
        reported.removeAll()
        kinds.removeAll()
        menus.removeAll()
        lastRange.removeAll()
        appliedRange.removeAll()
        faces.removeAll()
    }
}

#endif
