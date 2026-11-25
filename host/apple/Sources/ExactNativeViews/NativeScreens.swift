// Containment for app and package screens (LLP 1075.003 §3.6).
#if os(iOS) || os(tvOS)
import ExactKit
import UIKit

extension NativeViews {
    var hasScreens: Bool { screens.count > 0 }

    func mountView(_ entry: NativeEntry, table: NativeTable) {
        guard let owner = entry.owner, let view = entry.view else { return }
        entry.safeArea = entry.handle.flatMap { table.safeArea?($0) }.flatMap {
            NativeSafeArea(Unmanaged<NSDictionary>.fromOpaque($0).takeRetainedValue())
        }
        guard let handle = entry.handle, let raw = table.platformController?(handle) else {
            return owner.addSubview(view)
        }
        entry.screen = Unmanaged<UIViewController>.fromOpaque(raw).takeUnretainedValue()
        screens.setObject(entry, forKey: owner)
        contain(entry)
        if entry.screen?.parent == nil {
            owner.presenter?.afterBatch { [weak self, weak entry] in
                guard let self, let entry, entries[entry.id] === entry, entry.screen?.parent == nil else { return }
                contain(entry)
            }
        }
    }

    private func contain(_ entry: NativeEntry) {
        guard let screen = entry.screen, let owner = entry.owner, let view = entry.view else { return }
        guard let parent = owner.screenParent() else {
            waitingScreens.insert(entry.id)
            if screen.parent != nil { detach(entry) }
            return
        }
        waitingScreens.remove(entry.id)
        let adopting = screen.parent !== parent
        if adopting {
            if screen.parent != nil {
                releaseSafeArea(entry)
                detach(entry)
            }
            parent.addChild(screen)
        }
        if view.superview !== owner {
            if !entry.sizing { view.frame = owner.contentBox() }
            owner.addSubview(view)
        }
        if adopting {
            screen.didMove(toParent: parent)
            log("\(entry.name) #\(entry.id): a screen in \(type(of: parent))")
        }
    }

    /// UIKit may move an ordinary ancestor after the batch that created the screen.
    func retryWaitingScreens(in subtree: NodeView) {
        for id in waitingScreens {
            if let entry = entries[id], entry.owner?.isDescendant(of: subtree) == true { contain(entry) }
        }
    }

    /// Search a small moving subtree directly. After one visit per screen,
    /// check the screen owners' ancestry instead.
    private func screens(in subtree: UIView) -> [NativeEntry] {
        guard screens.count > 0 else { return [] }
        if screens.count > 1 {
            var remaining = screens.count, found: [NativeEntry] = []
            func visit(_ view: UIView) -> Bool {
                guard remaining > 0 else { return false }
                remaining -= 1
                if let entry = screens.object(forKey: view) { found.append(entry) }
                return view.heldSubviews.allSatisfy(visit)
            }
            if visit(subtree) { return found }
        }
        var found: [NativeEntry] = []
        for case let entry as NativeEntry in screens.objectEnumerator()! where entry.owner?.isDescendant(of: subtree) == true {
            found.append(entry)
        }
        return found
    }

    func setScreensMounted(_ mounted: Bool, in subtree: UIView) {
        for entry in screens(in: subtree) where entries[entry.id] === entry {
            if mounted { contain(entry) } else { detach(entry) }
        }
    }

    func prepareScreenMove(_ subtree: UIView, into container: UIView) -> (() -> Void)? {
        // A dialog explicitly releases its controller while keeping the root view
        // in its box. Its sheet calls moved after the entire dialog is in place.
        let moving = screens(in: subtree).filter { $0.screen?.parent != nil || $0.view?.superview == nil }
        guard !moving.isEmpty else { return nil }
        for entry in moving where entries[entry.id] === entry && entry.screen?.parent !== entry.owner?.screenParent(moving: subtree, into: container) {
            detach(entry)
        }
        return { [self] in
            for entry in moving where entries[entry.id] === entry { contain(entry) }
        }
    }

    /// A dialog's content stays in its node as the node moves into its sheet or back.
    func leaving(_ subtree: NodeView) {
        for entry in screens(in: subtree) where entries[entry.id] === entry {
            if let screen = entry.screen { leaveParent(screen) }
        }
    }

    func moved(_ subtree: NodeView) {
        for entry in screens(in: subtree) where entries[entry.id] === entry {
            contain(entry)
        }
    }

    private func leaveParent(_ screen: UIViewController) {
        guard screen.parent != nil else { return }
        screen.willMove(toParent: nil)
        screen.removeFromParent()
    }

    func detach(_ entry: NativeEntry, moving: Bool = true) {
        // Without the bracket UIKit can defer didDisappear until after reattachment.
        let visible = moving && entry.view?.window != nil && entry.screen?.parent != nil ? entry.screen : nil
        visible?.beginAppearanceTransition(false, animated: false)
        if let screen = entry.screen { leaveParent(screen) }
        entry.view?.removeFromSuperview()
        visible?.endAppearanceTransition()
    }
}
#endif
