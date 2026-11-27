// The language op carries the resolved language and applied layout direction.
#if os(macOS)
import AppKit
#else
import UIKit
#endif

extension Presenter {
    /// Accessibility can ask for a hyphenated paragraph while its run table is being applied.
    func takeLanguage(_ batch: Batch) -> Bool {
        guard let language = batch.ops.last(where: { $0.op == .language }) else { return false }
        documentLanguage = language.payload["lang"] as? String ?? ""
        documentDirection = language.payload["dir"] as? String ?? "ltr"
        return true
    }

    /// Invalidate live auto-hyphenated paragraphs whose cached spec was built
    /// in an earlier document language. Runs while `applying` is true so
    /// same-batch text consumers (`changedText`, visible text refresh) see it.
    func invalidateHyphenatedParagraphsForLanguage() {
        for node in views.values where node.isParagraph {
            guard let spec = node.cachedTextSpec, spec.automaticHyphens, spec.language != documentLanguage else { continue }
            node.invalidateText()
            #if os(macOS)
            node.needsDisplay = true
            #else
            node.setNeedsDisplay()
            #endif
        }
    }

    func applyLanguage(_ batch: Batch, documentChanged: Bool) {
        guard documentChanged || batch.ops.contains(where: { [.create, .style, .props, .children].contains($0.op) }) else { return }
        let changed: [NodeView]
        if documentChanged {
            changed = Array(views.values)
        } else {
            var ids = Set<UInt32>()
            for op in batch.ops where [.create, .style, .props, .children].contains(op.op) {
                if let id = op.nodeID { ids.insert(id) }
            }
            guard !ids.isEmpty else { return }
            changed = ids.compactMap { views[$0] }
        }
        #if os(macOS)
        root.userInterfaceLayoutDirection = documentDirection == "rtl" ? .rightToLeft : .leftToRight
        #else
        root.semanticContentAttribute = documentDirection == "rtl" ? .forceRightToLeft : .forceLeftToRight
        root.accessibilityLanguage = documentLanguage.isEmpty ? nil : documentLanguage
        if documentChanged { turnChrome() }
        #endif
        for node in changed {
            let direction = node.style["direction"]?.string ?? documentDirection
            #if os(macOS)
            node.userInterfaceLayoutDirection = direction == "rtl" ? .rightToLeft : .leftToRight
            node.scroll?.tile()
            node.fitScroll()
            #else
            node.semanticContentAttribute = direction == "rtl" ? .forceRightToLeft : .forceLeftToRight
            #endif
            #if !os(macOS)
            for child in node.subviews where !(child is NodeView) { setLanguage(child) }
            #endif
        }
    }

    #if !os(macOS)
    /// UIKit's own chrome (a navigation bar's Back, the tab bar's order, a
    /// sheet's bar) follows the `layoutDirection` trait, which a view's
    /// `semanticContentAttribute` does not set. The controllers Exact owns
    /// take the document's direction once it is told one: the navigation
    /// owner and each presented sheet. The embedder's window and its other
    /// controllers, another session's among them, keep their own.
    package func turnChrome() {
        let direction: UITraitEnvironmentLayoutDirection = documentDirection == "rtl" ? .rightToLeft : .leftToRight
        guard chromeDirection != direction else { return }
        chromeDirection = direction
        for controller in navigation.chromeOwners + modals.controllers { turnChrome(controller) }
    }

    /// `controller` takes the document's direction, as it is installed or
    /// presented and when the direction changes.
    package func turnChrome(_ controller: UIViewController) {
        guard let direction = chromeDirection,
              !controller.traitOverrides.contains(UITraitLayoutDirection.self) || controller.traitOverrides.layoutDirection != direction else { return }
        controller.traitOverrides.layoutDirection = direction
    }

    /// A platform view a node holds (a field, a control) is told the
    /// language; a node reads it (`NodeView.accessibilityLanguage`).
    private func setLanguage(_ view: UIView) {
        view.accessibilityLanguage = documentLanguage.isEmpty ? nil : documentLanguage
        for child in view.subviews where !(child is NodeView) { setLanguage(child) }
    }
    #endif
}

extension NodeView {
    /// Offstage text can draw or answer an accessibility read before its document is revealed.
    var paragraphLanguage: String {
        #if os(macOS)
        if case .preparing(let publication) = stage, !publication.committed, let language = publication.language { return language }
        #endif
        return presenter?.documentLanguage ?? ""
    }
}

#if !os(macOS)
extension NodeView {
    /// The document's language (`lang`), which VoiceOver speaks the node in:
    /// read from the presenter. UIKit stores a set one per view (an
    /// associated object: 20 KB of malloc over euxy's 250 nodes), and an
    /// unset one is the user's language, not an ancestor's (`UIAccessibility.h`).
    package override var accessibilityLanguage: String? {
        get {
            if let own = super.accessibilityLanguage { return own }
            guard let language = presenter?.documentLanguage, !language.isEmpty else { return nil }
            return language
        }
        set { super.accessibilityLanguage = newValue }
    }
}
#endif
