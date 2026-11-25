import ExactCore
// @ref LLP 1114.008 D1
#if os(macOS)
import AppKit
package typealias PlatformAction = NSAccessibilityCustomAction
#else
import UIKit
package typealias PlatformAction = UIAccessibilityCustomAction
#endif

/// Identifies actions Exact installed on a native field or control so its
/// own actions survive a change to the referenced elements.
private final class ReferencedAction: PlatformAction {
    weak var owner: NodeView?
    var generation: UInt64 = 0
    var reference = ""
    var targetID: UInt32 = 0
}

extension NodeView {
    private func actionRefs(_ run: UInt32?) -> [String] {
        let attrs = run.map { presenter?.inlineText($0)?.props ?? [:] } ?? props
        return Self.idRefs(attrs["accessibilityActions"] ?? "")
    }

    private var actionVisible: Bool {
        #if os(macOS)
        return accessibilityVisible && !accessibilityHiddenByProp
        #else
        guard !authoredLine.contains(where: { $0.props["accessibilityElementsHidden"] == "true" }) else { return false }
        if accessibilityVisible && !accessibilityElementsHidden { return true }
        guard accessibilityExposed, !authoredInert else { return false }
        return presenter?.navigation.tipItem(self)?.0.isEnabled == true || projectedReorderResponder(accessibility: true) != nil
        #endif
    }

    private func canOfferActions(from run: UInt32?) -> Bool {
        guard actionVisible else { return false }
        guard let run else { return true }
        guard let presenter, let source = presenter.inlineText(run) else { return false }
        return !source.hidden && !presenter.inlineHiddenFromAccessibility(source)
    }

    private func actionTarget(_ ref: String, from source: UInt32?) -> (node: NodeView, id: UInt32, name: String)? {
        guard let presenter, presenter.views[id] === self, let id = presenter.chrome.accessibilityID(ref),
              id != (source ?? self.id), let target = presenter.textHost(id), target.actionVisible else { return nil }
        if let run = presenter.inlineText(id) {
            guard target.canActivateInline(id), !presenter.inlineAriaUnavailable(id), !run.hidden,
                  !presenter.inlineHiddenFromAccessibility(run) else { return nil }
            let name = target.inlineName(run, text: target.paragraphText as NSString)
            return (target, id, name)
        }
        guard target.handlers.contains("press") || target.defaultLink != nil, !target.disabled, !target.ariaUnavailable else { return nil }
        return (target, id, target.actionName)
    }

    private var focusedActionID: UInt32? {
        #if os(macOS)
        guard let responder = window?.firstResponder else { return nil }
        return (responder as? InlineKeyView)?.id ?? presenter?.keyTarget(responder)?.id
        #else
        return presenter?.focusedNode?.id
        #endif
    }

    private func referencedActions(from run: UInt32? = nil, offeredBy: NSObject? = nil) -> [ReferencedAction] {
        guard canOfferActions(from: run) else { return [] }
        let ownerID = id
        #if os(macOS)
        let generation: UInt64 = 0
        #else
        let generation = incarnation
        #endif
        var seen = Set<String>()
        return actionRefs(run).compactMap { ref in
            guard seen.insert(ref).inserted, let (_, id, name) = actionTarget(ref, from: run), !name.isEmpty else { return nil }
            let currentTarget = { [weak self] () -> (node: NodeView, id: UInt32, name: String)? in
                guard let self, self.id == ownerID, self.canOfferActions(from: run), self.actionRefs(run).contains(ref),
                      let target = self.actionTarget(ref, from: run), !target.name.isEmpty else { return nil }
                #if !os(macOS)
                guard self.incarnation == generation else { return nil }
                #endif
                return target
            }
            let press = { [weak self, weak offeredBy] (expected: UInt32?) in
                guard let self, let (node, id, _) = currentTarget(), expected == nil || expected == id, let presenter = self.presenter else { return false }
                let focusBefore = self.focusedActionID
                if id == node.id { presenter.press(id) }
                else if !node.activateInline(id) { return false }
                let focusAfter = self.focusedActionID
                var restore = focusBefore != id && focusAfter == id
                #if !os(macOS)
                // Disabling the pressed control may resign it before its handler returns.
                if focusAfter == nil, presenter.focusEvents.blurring == id, node.formDisabled {
                    let responder = FirstResponder.current
                    restore = responder == nil || responder === self.window
                }
                #endif
                if restore, presenter.views[self.id] === self {
                    #if os(macOS)
                    let source = run.flatMap(presenter.inlineKeyView) ?? presenter.keyView(of: self)
                    self.window?.makeFirstResponder(source.acceptsFirstResponder ? source : nil)
                    let sourceElement: NSAccessibilityProtocol = (offeredBy as? NSAccessibilityProtocol) ?? self
                    sourceElement.setAccessibilityFocused(true)
                    #else
                    presenter.focusNode(self)
                    if presenter.focusedNode === node { self.window?.endEditing(true) }
                    UIAccessibility.post(notification: .layoutChanged, argument: offeredBy ?? self)
                    #endif
                }
                return true
            }
            #if os(macOS)
            let action = ReferencedAction(name: name) {
                guard let accepted = currentTarget()?.id else { return false }
                // Updating a control's AX properties inside AppKit's action callback can fail the client's request.
                DispatchQueue.main.async { _ = press(accepted) }
                return true
            }
            #else
            let action = ReferencedAction(name: name) { _ in press(nil) }
            #endif
            action.reference = ref
            action.owner = self
            action.generation = generation
            action.targetID = id
            return action
        }
    }

    func inlineName(_ run: InlineText, text: NSString) -> String {
        referencedLabel(run.props["accessibilityLabelledBy"]) ?? run.props["accessibilityLabel"].flatMap { $0.isEmpty ? nil : $0 } ?? visibleInlineText(in: run.range, source: text)
    }

    func hasReferencedAction(from run: UInt32? = nil) -> Bool {
        canOfferActions(from: run) && actionRefs(run).contains { actionTarget($0, from: run)?.name.isEmpty == false }
    }

    private var actionName: String {
        #if os(macOS)
        return authoredLabel ?? presenter?.controls.controls[id]?.accessibilityLabel() ?? accessibleName
        #else
        return authoredLabel ?? presenter?.controls.controls[id]?.accessibilityLabel ?? accessibilityLabel ?? accessibleName
        #endif
    }

    package func appendingActions(to platform: [PlatformAction]?, from run: UInt32? = nil, offeredBy: NSObject? = nil) -> [PlatformAction]? {
        guard run != nil || props["accessibilityActions"] != nil else { return platform }
        let actions = referencedActions(from: run, offeredBy: offeredBy)
        return actions.isEmpty ? platform : (platform ?? []) + actions
    }

    /// A native field or control answers for itself. Reconcile its list when
    /// a batch changes either end of the relation.
    func syncControlActions(_ control: PlatformView) {
        #if os(macOS)
        func sync(_ element: NSObject & NSAccessibilityProtocol) {
            if let actions = revisedActions(element.accessibilityCustomActions(), on: element) {
                element.setAccessibilityCustomActions(actions.isEmpty ? nil : actions)
            }
        }
        sync(control)
        // AppKit serves an NSButton or text field through its cell.
        if let cell = (control as? NSControl)?.cell { sync(cell) }
        #else
        syncProjectedActions(control)
        #endif
    }

    #if !os(macOS)
    package static func clearReferencedActions(from element: NSObject) {
        if let actions = element.accessibilityCustomActions, actions.contains(where: { $0 is ReferencedAction }) {
            element.accessibilityCustomActions = actions.filter { !($0 is ReferencedAction) }
        }
    }

    package func syncProjectedActions(_ control: NSObject) {
        if let actions = revisedActions(control.accessibilityCustomActions, on: control) {
            control.accessibilityCustomActions = actions.isEmpty ? nil : actions
        }
    }
    #endif

    private func revisedActions(_ assigned: [PlatformAction]?, on element: NSObject) -> [PlatformAction]? {
        let existing = assigned ?? []
        let old = existing.compactMap { $0 as? ReferencedAction }
        let new = referencedActions(offeredBy: element)
        guard !old.elementsEqual(new, by: { $0.owner === $1.owner && $0.generation == $1.generation && $0.targetID == $1.targetID && $0.reference == $1.reference && $0.name == $1.name }) else { return nil }
        return existing.filter { !($0 is ReferencedAction) } + new
    }

    #if !os(macOS)
    func childrenWithActions(_ children: [Any]?) -> [Any]? {
        guard !actionRefs(nil).isEmpty else { actionSummary = nil; return children }
        guard !isAccessibilityElement, field == nil, textArea == nil, presenter?.controls.controls[id] == nil,
              actionSummary != nil || hasReferencedAction() else { return children }
        let summary = actionSummary ?? ParagraphAccessibility(owner: self)
        actionSummary = summary
        summary.accessibilityFrameInContainerSpace = bounds
        summary.accessibilityLabel = accessibleName
        if let children { return [summary] + children }
        let ordered = Self.accessibilityOrder(subviews).compactMap { $0 as? NSObject }
        return [summary] + nativeAccessibilityElements(declared: ordered, visibleOnly: true)
    }
    #endif
}
