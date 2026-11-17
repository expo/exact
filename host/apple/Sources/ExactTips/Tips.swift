// The Tips capability (LLP 1035.006.000): an element's tip as TipKit's
// popover, over the element or over the bar or toolbar item that stands for
// it, shown while TipKit says it may and the element is on screen, one at a
// time in document order (D3). TipKit remembers what the person closed and
// what `tipDone` retired, by the element's id. An app's composition links
// this module, and TipKit and SwiftUI with it, only when its plan binds `tip`.
// tvOS has no UIKit tip popover: there it installs nothing and links neither
// framework, and no tip shows (D4).
import ExactKit
#if os(iOS) || os(macOS)
import SwiftUI
import TipKit
#endif
#if os(iOS)
import UIKit
#elseif os(macOS)
import AppKit
#endif

public enum ExactTips {
    public static func install() {
        #if os(iOS) || os(macOS)
        TipLink.installed = TipHost()
        #endif
    }
}

#if os(iOS)
extension TipUIPopoverViewController: TipPresentation {}
#endif

#if os(iOS) || os(macOS)

/// A tip as an element writes it. Its title and message are already in the
/// reader's language (`t()`), so they are shown verbatim.
struct AuthoredTip: Tip {
    let id: String
    let content: TipContent
    var title: Text { Text(verbatim: content.title) }
    var message: Text? { content.message.map { Text(verbatim: $0) } }
    var image: Image? { content.symbol.map { Image(systemName: $0) } }
}

final class TipHost: TipCapability {
    /// Whether TipKit has been configured: once a process, at the first tip.
    private var configured = false
    private var shown: [Key: Shown] = [:]
    /// Each owner's tips in document order: the first that may show does.
    private var order: [ObjectIdentifier: [String]] = [:]

    private struct Key: Hashable { let owner: ObjectIdentifier; let id: String }

    /// One element's tip: TipKit's view of it, and its popover while it shows.
    final class Shown {
        let owner: ObjectIdentifier
        var content: TipContent
        var tip: AuthoredTip
        var anchor: TipAnchor?
        /// TipKit says it may show (`shouldDisplay`).
        var eligible = false
        /// The person dismissed its popover where it stood; it may show
        /// again once its element has left the screen and come back.
        var dismissed = false
        var retired = false
        var watch: Task<Void, Never>?
        #if os(iOS)
        var popover: UIViewController?
        #else
        var popover: NSPopover?
        #endif
        /// What the popover was shown over.
        var over: AnyObject?
        init(_ content: TipContent, owner: ObjectIdentifier) {
            (self.content, self.owner, tip) = (content, owner, AuthoredTip(id: content.id, content: content))
        }
    }

    func update(_ tips: [(content: TipContent, anchor: TipAnchor?)], owner: AnyObject, store: URL?) {
        let ownerID = ObjectIdentifier(owner)
        if !tips.isEmpty { configure(store) }
        let live = Set(tips.map { Key(owner: ownerID, id: $0.content.id) })
        for (key, entry) in shown where key.owner == ownerID && !live.contains(key) {
            entry.watch?.cancel()
            close(entry)
            shown[key] = nil
        }
        for (content, anchor) in tips {
            let key = Key(owner: ownerID, id: content.id)
            let entry = shown[key] ?? start(content, key)
            if entry.content != content {
                let reworded = entry.content.title != content.title || entry.content.message != content.message || entry.content.symbol != content.symbol
                entry.content = content
                entry.tip = AuthoredTip(id: content.id, content: content)
                if reworded { close(entry) }
            }
            // D2: the lesson is done; TipKit retires the tip for good.
            if content.done && !entry.retired {
                entry.retired = true
                entry.tip.invalidate(reason: .actionPerformed)
            }
            if anchor == nil { entry.dismissed = false }
            entry.anchor = anchor
        }
        order[ownerID] = tips.isEmpty ? nil : tips.map(\.content.id)
        place(ownerID)
    }

    func withdraw(owner: AnyObject) {
        let ownerID = ObjectIdentifier(owner)
        for (key, entry) in shown where key.owner == ownerID {
            entry.watch?.cancel()
            close(entry)
            shown[key] = nil
        }
        order[ownerID] = nil
    }

    func state(owner: AnyObject) -> [[String: Any]] {
        let ownerID = ObjectIdentifier(owner)
        return (order[ownerID] ?? []).compactMap { shown[Key(owner: ownerID, id: $0)] }.map { entry in
            var row: [String: Any] = ["id": entry.content.id, "title": entry.content.title, "shows": showing(entry)]
            switch entry.tip.status {
            case .pending: row["status"] = "pending"
            case .available: row["status"] = "available"
            case .invalidated(let reason):
                row["status"] = "invalidated"
                switch reason {
                case .actionPerformed: row["reason"] = "actionPerformed"
                case .displayCountExceeded: row["reason"] = "displayCountExceeded"
                case .displayDurationExceeded: row["reason"] = "displayDurationExceeded"
                case .tipClosed: row["reason"] = "tipClosed"
                @unknown default: row["reason"] = "\(reason)"
                }
            @unknown default: row["status"] = "\(entry.tip.status)"
            }
            if let anchor = entry.anchor {
                switch anchor { case .view: row["over"] = "element"; case .item: row["over"] = "bar item" }
            }
            return row
        }
    }

    /// TipKit's store: TipKit's own place, or the one the core names (the
    /// agent's drive). Configuring twice is TipKit's refusal, and an
    /// embedding app may have configured it itself; either way its tips work.
    private func configure(_ store: URL?) {
        guard !configured else { return }
        configured = true
        var options: [Tips.ConfigurationOption] = []
        if let store {
            try? FileManager.default.createDirectory(at: store, withIntermediateDirectories: true)
            options.append(.datastoreLocation(.url(store)))
        }
        try? Tips.configure(options)
    }

    private func start(_ content: TipContent, _ key: Key) -> Shown {
        let entry = Shown(content, owner: key.owner)
        shown[key] = entry
        let tip = entry.tip
        entry.watch = Task { @MainActor [weak self, weak entry] in
            for await eligible in tip.shouldDisplayUpdates {
                guard let self, let entry else { return }
                entry.eligible = eligible
                self.place(entry.owner)
            }
        }
        return entry
    }

    /// Show the first of `owner`'s tips that may show, where it stands now,
    /// and close every other.
    private func place(_ owner: ObjectIdentifier) {
        let entries = (order[owner] ?? []).compactMap { shown[Key(owner: owner, id: $0)] }
        entries.forEach(noteDismissal)
        let first = entries.first { $0.eligible && !$0.dismissed && !$0.retired && $0.anchor != nil }
        for entry in entries where entry !== first { close(entry) }
        guard let first, let anchor = first.anchor else { return }
        if first.popover != nil, first.over === target(anchor) { return }
        close(first)
        present(first, at: anchor)
    }

    private func target(_ anchor: TipAnchor) -> AnyObject {
        #if os(iOS)
        switch anchor { case .view(let view, _): return view; case .item(let item, _): return item }
        #else
        switch anchor { case .view(let view): return view; case .item(let item): return item }
        #endif
    }

    #if os(iOS)
    private func showing(_ entry: Shown) -> Bool { entry.popover?.presentingViewController != nil }

    /// A popover gone from the screen that nothing here closed: the person
    /// tapped outside it.
    private func noteDismissal(_ entry: Shown) {
        guard let popover = entry.popover, popover.presentingViewController == nil, !popover.isBeingPresented else { return }
        entry.popover = nil; entry.over = nil
        entry.dismissed = true
    }

    private func present(_ entry: Shown, at anchor: TipAnchor) {
        let source: UIPopoverPresentationControllerSourceItem, controller: UIViewController
        switch anchor {
        case .view(let view, let from): source = view; controller = from
        case .item(let item, let from): source = item; controller = from
        }
        guard controller.viewIfLoaded?.window != nil, controller.presentedViewController == nil else { return }
        // A push or a sheet still moving: it shows once the move ends.
        if let moving = controller.transitionCoordinator {
            moving.animate(alongsideTransition: nil) { [weak self] _ in self?.place(entry.owner) }
            return
        }
        let popover = TipUIPopoverViewController(entry.tip, sourceItem: source)
        entry.popover = popover; entry.over = source
        controller.present(popover, animated: true)
    }

    /// Close `entry`'s popover; once it has gone, the next tip may show.
    private func close(_ entry: Shown) {
        guard let popover = entry.popover else { return }
        entry.popover = nil; entry.over = nil
        guard popover.presentingViewController != nil else { return }
        popover.dismiss(animated: true) { [weak self] in self?.place(entry.owner) }
    }
    #else
    private func showing(_ entry: Shown) -> Bool { entry.popover?.isShown == true }

    /// A popover that closed with nothing here closing it: the person
    /// dismissed it.
    private func noteDismissal(_ entry: Shown) {
        guard let popover = entry.popover, !popover.isShown else { return }
        entry.popover = nil; entry.over = nil
        entry.dismissed = true
    }

    private func present(_ entry: Shown, at anchor: TipAnchor) {
        let popover = TipNSPopover(entry.tip)
        switch anchor {
        case .view(let view):
            guard view.window?.isVisible == true else { return }
            popover.show(relativeTo: view.bounds, of: view, preferredEdge: view.isFlipped ? .maxY : .minY)
            entry.over = view
        case .item(let item):
            popover.show(relativeTo: item)
            entry.over = item
        }
        entry.popover = popover
    }

    private func close(_ entry: Shown) {
        guard let popover = entry.popover else { return }
        entry.popover = nil; entry.over = nil
        if popover.isShown { popover.close() }
    }
    #endif
}
#endif
