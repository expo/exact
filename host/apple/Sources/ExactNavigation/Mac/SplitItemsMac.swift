// @ref LLP 1123 D15 — a Mac split item's controls, each one AppKit property of the item, read on the
// element that projects it: the sidebar's on the root tablist, the page's on its stack's first route,
// the detail's on the pane's first route, and the inspector's on the route's `aside`. Exact writes
// each after the pass that reads it, and owns it only while the attribute is set: it writes AppKit's default when the
// attribute goes and otherwise leaves the property to the window's owner.
//
// An accessory is the element its item names by `id`, shown in an `NSSplitViewItemAccessoryViewController`
// at the item's top or bottom (macOS 26): the element's view moves into the accessory's view and the
// kernel lays it out there as a content owner (LLP 1123 D2) at the width AppKit gives the accessory
// and its own height, as the sidebar's footer is. The element's own attributes set the accessory's
// content insets and its scroll edge effect.
#if os(macOS)
import AppKit
import ExactKit

/// What Exact shows on each split item: its accessories and the properties it owns, by item, so a change replaces only
/// its own.
final class SplitItemControls {
    fileprivate var accessories: [ObjectIdentifier: (top: SplitAccessory?, bottom: SplitAccessory?)] = [:]
    fileprivate var owned: [ObjectIdentifier: Set<String>] = [:]
}

/// One accessory Exact shows: the element, the accessory's view controller and the block its view is.
final class SplitAccessory {
    let node: NodeView
    let controller: NSViewController
    let pane: BlockView
    private let height: NSLayoutConstraint

    fileprivate init(node: NodeView, controller: NSViewController, pane: BlockView, height: NSLayoutConstraint) {
        (self.node, self.controller, self.pane, self.height) = (node, controller, pane, height)
    }

    /// The block the accessory's view gives the element: its width and, at most, the window's height.
    fileprivate var block: HostBlock { HostBlock(width: .size(pane.bounds.width), height: .atMost(max(1, pane.window?.contentView?.bounds.height ?? 10_000))) }

    fileprivate func fit(_ size: CGSize) { if height.constant != size.height { height.constant = size.height } }
}

extension ExactSidebarController {
    /// Write each item's controls from the element that projects it, as the last pass read them.
    func applyItemControls() {
        let presenter = exactView.session.presenter
        let split = presenter.navigationHost.split
        let items = [sidebarItem, pageItem, detailItem, inspectorItem].compactMap { $0 }
        // An item gone, as an inspector its route no longer has, gives back the accessories it showed
        releaseItemControls(except: Set(items.map(ObjectIdentifier.init)))
        if let sidebarItem { apply(sidebarItem, from: presenter.sidebarHost.tablist) }
        apply(pageItem, from: split.pageRoute)
        apply(detailItem, from: split.detailBase)
        if let inspectorItem { apply(inspectorItem, from: inspectorNode) }
    }

    /// Give back every item's accessories and properties but those of `items`: all of them as the controller leaves.
    func releaseItemControls(except items: Set<ObjectIdentifier> = []) {
        let presenter = exactView.session.presenter
        for (key, held) in itemControls.accessories where !items.contains(key) {
            for accessory in [held.top, held.bottom].compactMap({ $0 }) {
                presenter.blocks.release(accessory.node)
                accessory.controller.removeFromParent()
            }
            itemControls.accessories[key] = nil
        }
        itemControls.owned = itemControls.owned.filter { items.contains($0.key) }
    }

    private func apply(_ item: NSSplitViewItem, from node: NodeView?) {
        let props = node?.props ?? [:], key = ObjectIdentifier(item)
        // A property Exact owns while its attribute is set, AppKit's default once it goes, else the owner's
        func own(_ name: String, _ write: (String?) -> Void) {
            let said = props[name]
            guard said != nil || itemControls.owned[key]?.contains(name) == true else { return }
            write(said)
            if said == nil { itemControls.owned[key]?.remove(name) } else { itemControls.owned[key, default: []].insert(name) }
        }
        if item.behavior == .sidebar || item.behavior == .inspector {
            own("navigationSplitItemFullHeight") { said in
                let full = said != "false"
                if item.allowsFullHeightLayout != full { item.allowsFullHeightLayout = full }
            }
        }
        guard #available(macOS 26, *) else { return }
        own("navigationSplitItemAdjustsForOverlays") { said in
            let adjusts = said == "true"
            if item.automaticallyAdjustsSafeAreaInsets != adjusts { item.automaticallyAdjustsSafeAreaInsets = adjusts }
        }
        let held = itemControls.accessories[key]
        let top = accessory(named: props["navigationSplitItemTopAccessory"], kept: held?.top, on: item, top: true)
        let bottom = accessory(named: props["navigationSplitItemBottomAccessory"], kept: held?.bottom, on: item, top: false)
        itemControls.accessories[key] = top == nil && bottom == nil ? nil : (top, bottom)
    }

    /// The accessory showing the element named `id` on `item`: `kept` while it shows that element still, with its
    /// own controls written again; else a new one, `kept` given back first.
    @available(macOS 26, *)
    private func accessory(named id: String?, kept: SplitAccessory?, on item: NSSplitViewItem, top: Bool) -> SplitAccessory? {
        let presenter = exactView.session.presenter
        // Of several elements with the `id`, the first made, as elsewhere an `id` names one
        let node = id.flatMap { id in presenter.views.values.sorted { $0.id < $1.id }.first { $0.props["id"] == id } }
        if let kept, let node, kept.node === node, presenter.blocks.owns(node) {
            configure(kept)
            return kept
        }
        if let kept {
            presenter.blocks.release(kept.node)
            kept.controller.removeFromParent()
        }
        guard let node else { return nil }
        let pane = BlockView(frame: NSRect(x: 0, y: 0, width: item.viewController.view.bounds.width, height: node.frame.height))
        let height = pane.heightAnchor.constraint(equalToConstant: node.frame.height)
        height.isActive = true
        let controller = NSSplitViewItemAccessoryViewController()
        controller.view = pane
        if top { item.addTopAlignedAccessoryViewController(controller) } else { item.addBottomAlignedAccessoryViewController(controller) }
        let made = SplitAccessory(node: node, controller: controller, pane: pane, height: height)
        configure(made)
        pane.sizeChanged = { [weak presenter, weak made] _ in
            guard let made else { return }
            presenter?.blocks.resize(made.node, made.block)
        }
        pane.pointerTracking = { [weak presenter] event in presenter?.trackPointer(event) }
        presenter.blocks.show(node, in: pane, block: made.block) { [weak made] size in made?.fit(size) }
        return made
    }

    /// The accessory's own controls, from its element.
    @available(macOS 26, *)
    private func configure(_ accessory: SplitAccessory) {
        guard let controller = accessory.controller as? NSSplitViewItemAccessoryViewController else { return }
        let props = accessory.node.props
        let insets = props["navigationAccessoryContentInsets"] != "false"
        if controller.automaticallyAppliesContentInsets != insets { controller.automaticallyAppliesContentInsets = insets }
        guard #available(macOS 26.1, *) else { return }
        let style: NSScrollEdgeEffectStyle = switch props["navigationAccessoryScrollEdgeEffect"] {
        case "soft": .soft
        case "hard": .hard
        default: .automatic
        }
        if controller.preferredScrollEdgeEffectStyle != style { controller.preferredScrollEdgeEffectStyle = style }
    }
}
#endif
