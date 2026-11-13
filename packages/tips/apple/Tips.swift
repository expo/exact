// @exact/tips on Apple (LLP 1035.006 row 11.08): TipKit's popover, offered over an element.
// `showTip` makes a tip of the id and words it is given and anchors TipKit's popover
// (`TipUIPopoverViewController`, `TipNSPopover`) to the element `source` names: the item a
// navigation bar, toolbar or window toolbar shows it as, else its view
// (`ExactPackageContext.anchor`). TipKit decides whether the tip shows and keeps that across
// launches: one invalidated, closed by the person or past its display frequency does not. The
// send answers nothing either way. `invalidateTip` says the person has done what the tip
// teaches (`actionPerformed`), and a shown tip closes. TipKit is configured at the first call.
//
// Under the agent TipKit's record is in the drive's store (`context.data`), so a drive starts
// with every tip eligible and a named store keeps what it invalidated across relaunches; the
// popover shows as it would for a person.
import ExactPackage
import Foundation
import SwiftUI
import TipKit
#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// A tip of the words a call gives. TipKit keeps a tip's state by its `id`.
struct OfferedTip: Tip {
    let id: String
    var heading = ""
    var body: String?
    /// An SF Symbol's name.
    var symbol: String?
    var title: Text { Text(verbatim: heading) }
    var message: Text? { body.map { Text(verbatim: $0) } }
    var image: Image? { symbol.map { Image(systemName: $0) } }
}

public final class TipsModule: ExactPackage {
    let context: ExactPackageContext
    /// The tips offered, by id: what watches TipKit's answer for each, and its popover while shown.
    private(set) var offered: [String: Offer] = [:]
    /// `Tips.configure` may be called once in a process.
    private static var configured = false

    final class Offer {
        let source: String
        var watch: Task<Void, Never>?
        #if os(macOS)
        var popover: NSPopover?
        #else
        var controller: UIViewController?
        #endif
        init(source: String) { self.source = source }
    }

    public init(context: ExactPackageContext) { self.context = context }

    public func later(_ name: String, _ args: [String: Any], preferred: Any?, reply: @escaping ([String: Any]) -> Void) {
        switch name {
        case "showTip": MainActor.assumeIsolated { offer(args) }
        case "invalidateTip": MainActor.assumeIsolated { invalidate(args["id"] as? String ?? "") }
        default: return reply(["error": "NotSupportedError"])
        }
        reply([:])
    }

    public func destroy(_ done: @escaping () -> Void) {
        MainActor.assumeIsolated { for id in Array(offered.keys) { withdraw(id) } }
        done()
    }

    private func configure() {
        guard !Self.configured else { return }
        Self.configured = true
        var options: [Tips.ConfigurationOption] = []
        if context.agent {
            let store = context.data.appendingPathComponent("exact-agent-tips", isDirectory: true)
            try? FileManager.default.createDirectory(at: store, withIntermediateDirectories: true)
            options.append(.datastoreLocation(.url(store)))
        }
        do { try Tips.configure(options) } catch { context.log("tips: TipKit refused its configuration: \(error)") }
    }

    /// Offered again, a tip is offered anew at its new source.
    @MainActor private func offer(_ args: [String: Any]) {
        guard let id = args["id"] as? String, !id.isEmpty, let title = args["title"] as? String, let source = args["source"] as? String else {
            return context.log("showTip: refused, it needs an id, a title and a source")
        }
        configure()
        withdraw(id)
        let tip = OfferedTip(id: id, heading: title, body: args["message"] as? String, symbol: args["image"] as? String)
        let offer = Offer(source: source)
        offered[id] = offer
        offer.watch = Task { @MainActor [weak self, weak offer] in
            for await shows in tip.shouldDisplayUpdates {
                guard let self, let offer, self.offered[id] === offer else { return }
                if shows { self.present(tip, offer) } else { self.close(offer) }
                self.disclose()
            }
        }
    }

    @MainActor private func invalidate(_ id: String) {
        guard !id.isEmpty else { return context.log("invalidateTip: refused, it needs an id") }
        configure()
        OfferedTip(id: id).invalidate(reason: .actionPerformed)
        withdraw(id)
        disclose()
        context.log("invalidateTip: \(id) invalidated")
    }

    @MainActor private func withdraw(_ id: String) {
        guard let offer = offered.removeValue(forKey: id) else { return }
        offer.watch?.cancel()
        close(offer)
    }

    @MainActor private func present(_ tip: OfferedTip, _ offer: Offer) {
        guard !shown(offer) else { return }
        guard let anchor = context.anchor(offer.source) else {
            return context.log("showTip: \(tip.id) not shown, no element #\(offer.source) shows")
        }
        #if os(macOS)
        let popover = TipNSPopover(tip)
        popover.animates = !context.agent
        switch anchor {
        case .barItem(let item): popover.show(relativeTo: item)
        case .view(let view): popover.show(relativeTo: view.bounds, of: view, preferredEdge: view.isFlipped ? .maxY : .minY)
        }
        offer.popover = popover
        #else
        let item: UIPopoverPresentationControllerSourceItem
        switch anchor {
        case .barItem(let bar): item = bar
        case .view(let view): item = view
        }
        guard var over = context.presentation?.rootViewController else {
            return context.log("showTip: \(tip.id) not shown, the session is in no window")
        }
        while let next = over.presentedViewController, !next.isBeingDismissed { over = next }
        let controller = TipUIPopoverViewController(tip, sourceItem: item)
        over.present(controller, animated: !context.agent)
        offer.controller = controller
        #endif
        if case .barItem = anchor { context.log("showTip: \(tip.id) shown over #\(offer.source)'s bar item") }
        else { context.log("showTip: \(tip.id) shown over #\(offer.source)") }
    }

    @MainActor private func shown(_ offer: Offer) -> Bool {
        #if os(macOS)
        offer.popover?.isShown == true
        #else
        offer.controller.map { $0.presentingViewController != nil && !$0.isBeingDismissed } == true
        #endif
    }

    @MainActor private func close(_ offer: Offer) {
        #if os(macOS)
        offer.popover?.close()
        offer.popover = nil
        #else
        if let controller = offer.controller, controller.presentingViewController != nil, !controller.isBeingDismissed {
            controller.dismiss(animated: !context.agent)
        }
        offer.controller = nil
        #endif
    }

    /// The tips shown now, for `state.extensions` under the agent.
    @MainActor private func disclose() {
        context.state(["shown": offered.filter { shown($0.value) }.keys.sorted()])
    }
}
