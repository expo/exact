// System notices (the app became active, a window moved, a preference
// changed) told to the runtime without the main thread waiting.
// @ref LLP 1027.007 P1
#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif
import ExactCore
import QuartzCore

extension ExactSession {
    func pumpReplies() {
        guard state != .destroyed else { return }
        let began = CACurrentMediaTime()
        let captured = generation, time = now()
        repliesInFlight += 1
        // Keep parsing, settlement and batch decoding on the existing owner.
        // Publication joins fills and ticks before any later synchronous batch.
        runtime.pumpAsync(now: time) { [weak self] batch in
            self?.publish(batch == nil ? .cancelledReply : .reply, batch ?? Batch(ops: [], timers: false, motion: false, clock: nil, error: nil), captured)
        }
        replySubmissions.append((CACurrentMediaTime() - began) * 1000)
    }

    #if DEBUG
    /// Tests: a reply the owner answered as cancelled, published as `pumpReplies` publishes one.
    func publishCancelledReplyForTests() {
        repliesInFlight += 1
        replySubmissions.append(0)
        publish(.cancelledReply, Batch(ops: [], timers: false, motion: false, clock: nil, error: nil), generation)
    }
    #endif

    func retireReply() -> Double {
        repliesInFlight -= 1
        return replySubmissions.removeFirst()
    }

    func retryActivation(generation replyGeneration: Int) {
        if let p = pendingActivation, p.generation == replyGeneration {
            pendingActivation = nil
            if p.generation == generation {
                activatedGeneration = nil
                firstDrawn(generation: p.generation, token: p.token)
            }
        }
    }

    /// What a system notice tells the runtime (the app became active, a
    /// window moved, a preference changed) while the owner activates the data
    /// source, sent there without main waiting (LLP 1027.007 P1): on a cold
    /// launch iOS posts these at first pixel. The batches land in the owner's
    /// order, before any batch made after them (T4). At any other time a
    /// notice is told at once, so it has taken effect when it returns.
    func tellSoon(_ calls: [(Runtime) -> Batch]) {
        guard !runtime.holds("tellSoon(_:)") else { return } // (a notice a restart's reset sets off: held before it is counted or queued, ResetGate)
        noticesInFlight += calls.count
        let captured = generation
        runtime.later(calls) { [weak self] batch in self?.publish(.notice, batch, captured) }
    }

    /// `tellPage` for a notice; at once unless the data source is activating
    /// and this generation's `onLine` reading is known.
    func tellPageSoon() {
        guard booted, state != .destroyed else { return }
        guard dataPreparing, let reads = readsOnLineKnown, reads.generation == generation, !Owner.shared.isServingMain else { return tellPage() }
        #if canImport(UIKit)
        if view?.window?.windowScene?.activationState != .foregroundActive { presenter.supersedePress() }
        #endif
        let bits = PageFacts.bits(pageWindow, page: view?.window, readsOnLine: reads.value)
        let font = PageFacts.rootFontSize, scale = PageFacts.displayScale(presenter.viewport)
        tellSoon([{ $0.setPage(bits) }, { $0.setRootFontSize(font) }, { $0.setDisplayScale(scale) }])
    }

    /// @ref LLP 1061 D5 — told after every boot, as the date is, and on each
    /// change: in the same main-thread turn as the boot batch, so the first
    /// frame on screen already reads the user's preferences.
    func tellPreferences() {
        guard booted, state != .destroyed else { return }
        apply(runtime.setPreferences(preferenceBits()))
        // Increased Contrast changes what every platform colour resolves to; the build channel reports it (LLP 1122 D4).
        reportColors(); app.scheduleStatus()
        #if os(macOS)
        controlTextChanged()
        #endif
    }

    /// `tellPreferences` for a notice; at once unless the data source is
    /// activating. The colours go with it unless a batch is being applied, as
    /// `reportColors` decides.
    func tellPreferencesSoon() {
        guard booted, state != .destroyed else { return }
        guard dataPreparing, !Owner.shared.isServingMain else { return tellPreferences() }
        let bits = preferenceBits()
        var calls: [(Runtime) -> Batch] = [{ $0.setPreferences(bits) }]
        #if os(iOS) || os(tvOS)
        let reports = view?.window != nil
        #else
        let reports = true
        #endif
        if reports, applying || awaitingTree { colorsOwed = true } else if reports {
            let tint = SystemColor.appTint.flatMap { SystemColor.tintPair($0) }
            calls.append { $0.reportColors(tint: tint) }
        }
        tellSoon(calls)
        app.scheduleStatus()
        #if os(macOS)
        controlTextChanged()
        #endif
    }

    /// Recheck appearances seen before the session's trait report.
    func reportDeferredAppearances(first: Bool) {
        let early = unreported
        for id in early {
            // Boot's deferred views may already have a native override.
            if first { unreported.remove(id) }
            if let node = presenter.views[id] { noteAppearance(node); continue }
            #if os(iOS) || os(tvOS)
            if let dark = presenter.flats.appearance(of: id) { noteAppearance(id, dark: dark); continue }
            #endif
            unreported.remove(id)
        }
    }

    /// A view painting motion: when its own appearance is not the one its node's colours resolve by, say so after
    /// the batch (LLP 1062 D4).
    func noteAppearance(_ view: NodeView) { noteAppearance(view.id, dark: view.drawsDark) }
    /// A node painting in `dark`: a view, or a flat leaf in its parent's appearance.
    func noteAppearance(_ id: UInt32, dark: Bool) {
        guard let session = schemeDark else { unreported.insert(id); return }
        #if os(iOS) || os(tvOS)
        // The session's own trait walk is not an independent subtree override.
        if let view, (view.traitCollection.userInterfaceStyle == .dark) != session { unreported.insert(id); return }
        // The session can report before UIKit reaches a deferred child. A
        // colour-table batch may restyle it in between. An authored scheme
        // or a native override already fixes the subtree's appearance.
        if unreported.contains(id), dark != session, !hasAppearanceOverride(id, dark: dark) { return }
        #endif
        unreported.remove(id)
        guard dark != viewDark[id] ?? session else { return }
        viewDark[id] = dark == session ? nil : dark
        pendingViewDark.append((id, dark))
    }

    #if os(iOS) || os(tvOS)
    private func hasAppearanceOverride(_ id: UInt32, dark: Bool) -> Bool {
        let node = presenter.views[id] ?? presenter.flats.host(of: id)
        if node?.style["color_scheme"] != nil { return true }
        return Self.styleOverride(above: node, session: view) == (dark ? .dark : .light)
    }

    /// The nearest appearance override on UIKit's trait path up from
    /// `start`, short of anything `session` also inherits from (an override
    /// there is the session's own). In the iOS 17 trait hierarchy a
    /// controller's root view inherits from its controller, a controller from
    /// its parent, a presented one from its presentation controller, and that
    /// from the presenting controller (WWDC23, "Unleash the UIKit trait system").
    private static func styleOverride(above start: UIView?, session: UIView?) -> UIUserInterfaceStyle? {
        func covers(_ view: UIView?) -> Bool {
            guard let view, let session else { return false }
            return session.isDescendant(of: view)
        }
        var at: AnyObject? = start
        while let step = at {
            switch step {
            case let view as UIView:
                if view is UIWindow || covers(view) { return nil }
                if view.overrideUserInterfaceStyle != .unspecified { return view.overrideUserInterfaceStyle }
                if let controller = view.next as? UIViewController, controller.viewIfLoaded === view { at = controller } else { at = view.superview }
            case let controller as UIViewController:
                if covers(controller.viewIfLoaded) { return nil }
                if controller.overrideUserInterfaceStyle != .unspecified { return controller.overrideUserInterfaceStyle }
                // `presentationController` is read only once presented: reading it makes one.
                if let parent = controller.parent { at = parent }
                else if controller.presentingViewController != nil { at = controller.presentationController }
                else { at = controller.viewIfLoaded?.superview }
            case let presentation as UIPresentationController:
                if presentation.traitOverrides.contains(UITraitUserInterfaceStyle.self) { return presentation.traitOverrides.userInterfaceStyle }
                at = presentation.presentingViewController
            default: return nil
            }
        }
        return nil
    }
    #endif
}
