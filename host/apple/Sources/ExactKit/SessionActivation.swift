// What follows first pixel: the app module, the data source, native views.
// @ref LLP 1027 D4 — app logic loads after first pixel.
// @ref LLP 1024 D3 — native views attach after the paint gate.
import Foundation
import QuartzCore
import ExactCore

extension ExactSession {
    func firstDrawn(generation drawnGeneration: Int, token: UInt64) {
        guard state != .destroyed, generation == drawnGeneration else { return }
        if firstDrawMs == nil { firstDrawMs = ExactEnv.wall() }
        app.sharedStore.firstPixel()
        ExactLaunch.shared.drawn(self)
        guard activatedGeneration != drawnGeneration else { return }
        activatedGeneration = drawnGeneration
        DispatchQueue.main.async { [weak self] in
            guard let self, state != .destroyed, generation == drawnGeneration else { return }
            dataPreparing = true
            declareNotificationCategories { [weak self] in
                guard let self, state != .destroyed, generation == drawnGeneration else { return }
                dataPreparing = false
                // Prepare before any source calls the module (LLP 1067.000 D8).
                let started = CACurrentMediaTime()
                natives.prepareAppModule()
                ExactLaunch.shared.modulePrepared(self, seconds: CACurrentMediaTime() - started)
                presenter.releaseLaunchAutofocus() // the next turn, never waiting on a loading source
                restoreKeptDocuments() // before any answer reads a kept document (LLP 1069.010.000 D4)
                if preparedGeneration != drawnGeneration { prepareData(drawnGeneration, token: token) } else { activateData(drawnGeneration, token: token) }
            }
        }
    }

    /// Whether activation still has work queued: the agent's settle waits for it.
    var activationInFlight: Bool { natives.activationQueued || dataPreparing }

    /// The data source activates on the owner thread, where a TypeScript
    /// module evaluates its top level, while this thread goes on: images
    /// land, native views attach and input is handled. Waiting here instead
    /// held this thread for the whole evaluation (325 ms for a 2 MB catalog
    /// on an iPhone 17 Pro Max). A call this thread makes to the runtime
    /// meanwhile still waits its turn behind it: on a cold launch, the
    /// page's facts when the app becomes active do.
    private func prepareData(_ drawnGeneration: Int, token: UInt64) {
        preparedGeneration = drawnGeneration
        dataPreparing = true
        ExactLaunch.shared.waitingForData(self)
        openNativeViews(drawnGeneration)
        let runtime = runtime
        Owner.shared.post {
            runtime.prepareData()
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                dataPreparing = false
                guard state != .destroyed, generation == drawnGeneration else { return }
                activateData(drawnGeneration, token: token)
            }
        }
    }

    func activateData(_ drawnGeneration: Int, token: UInt64) {
        // A restart holding the runtime: the activation waits as a pending one does, retried after (ResetGate).
        if runtime.holds("activateData(_:token:)") { pendingActivation = (drawnGeneration, token); return }
        let started = CACurrentMediaTime()
        let batch = runtime.dataReady()
        ExactLaunch.shared.dataStep(self, appModule: 0, ready: CACurrentMediaTime() - started, pending: batch.pending)
        if batch.pending {
            ExactLaunch.shared.waitingForData(self)
            pendingActivation = (drawnGeneration, token) // the generation stays activated: only the source's wake retries
            return
        }
        appFileRoots = AppFiles.learn(runtime) // the roots storage configured
        let applying = CACurrentMediaTime()
        apply(batch)
        ExactLaunch.shared.dataApplied(self, CACurrentMediaTime() - applying)
        landColdShare() // a share claimed before the boot lands in the storage just configured
        observeLeaving()
        dataGeneration = drawnGeneration
        ExactLaunch.shared.activated(self, ok: batch.error == nil)
        activityOutput?.drain()
        storeRefresh.activated = { [weak self] in
            guard let self, state != .destroyed, generation == drawnGeneration else { return }
            presenter.collections.dataReady(); app.firstPixel(token)
        }
        storeRefresh.record(batch)
        retryStoreActivation()
        deliverNotificationResponses()
        deliverModuleEvents() // a launching activation, once the module and the data are up (LLP 1133.004 D6)
        NotificationCenter.default.post(name: Self.dataActivated, object: self)
        canvases.loadIfNeeded()
        openNativeViews(drawnGeneration)
        drainSurfaceWorkNow()
        frames.run(frames.hasWork)
        Owner.shared.releaseIdleStack() // after the data's batch: the deepest the boot goes
    }

    /// Native views attach after this turn's commit, once per generation;
    /// they read no data, so they do not wait for it.
    private func openNativeViews(_ drawnGeneration: Int) {
        guard nativesOpened != drawnGeneration else { return }
        nativesOpened = drawnGeneration
        natives.activateAfterCommit { [weak self] in self.map { $0.state != .destroyed && $0.generation == drawnGeneration } ?? false }
    }
}

extension ExactSession {
    /// Posted, with the session, when its data module's first commit has
    /// been applied (a host decides then what its first pixel could not:
    /// LLP 1133.025 D6).
    public static let dataActivated = Notification.Name("ExactSessionDataActivated")

    /// The scene became active (iOS): the canvases follow, and a background
    /// task the system took back is begun again while there is work to finish.
    public func becameActive() {
        frames.run(frames.hasWork)
        finishHold.becameActive()
    }

    /// How many module events the session holds before its runner can (the
    /// runner then holds each event to its own bound).
    static let heldModuleEventBound = 256

    /// A module event the platform reported for this session (LLP 1133.004
    /// D6): held from launch until first pixel and the data's activation,
    /// then raised into the runner and heard in order, once. `activation` is
    /// true only for a platform callback that reports a person's act on its
    /// own UI. `background` makes it a notification's background action
    /// the person chose (LLP 1133.004 stage 2): the host's id for it, whether
    /// the session was booted for it, and whether no view is seen.
    func raiseModuleEvent(extension name: String, event: String, args: String, activation: Bool,
                          background: (id: String, cold: Bool, hidden: Bool)? = nil) {
        guard state != .destroyed else { return }
        var body: [String: Any] = ["extension": name, "event": event, "args": (try? JSONSerialization.jsonObject(with: Data(args.utf8))) ?? [],
                                   "activation": activation, "deliver": false]
        if let background { body["background"] = background.id; body["cold"] = background.cold; body["hidden"] = background.hidden }
        guard let data = try? JSONSerialization.data(withJSONObject: body) else { return }
        if heldModuleEvents.count >= Self.heldModuleEventBound {
            heldModuleEvents.removeFirst()
            log("\(name).\(event): dropped (held before the session could hear it)")
        }
        heldModuleEvents.append(String(decoding: data, as: UTF8.self))
        if dataActivated { deliverModuleEvents() }
    }

    /// Each held module event raised into the runner, which bounds each
    /// event's own; then everything the runner queued is heard, once (an
    /// agent's tap of a listed entry too).
    func deliverModuleEvents() {
        guard state != .destroyed, booted, dataActivated, firstDrawMs != nil, !runtime.holds("deliverModuleEvents()") else { return } // (kept for after: ResetGate)
        let held = heldModuleEvents
        heldModuleEvents = []
        for json in held {
            let event = (try? JSONSerialization.jsonObject(with: Data(json.utf8))) as? [String: Any]
            if let id = event?["background"] as? String, !beginBackgroundAction(id) { continue }
            apply(runtime.moduleEvent(json))
        }
        apply(runtime.moduleEvent("{}"))
    }

    /// The session ends: each event no handler heard is journaled (D6).
    func endModuleEvents() {
        for json in heldModuleEvents {
            let v = (try? JSONSerialization.jsonObject(with: Data(json.utf8))) as? [String: Any] ?? [:]
            log("\(v["extension"] as? String ?? "").\(v["event"] as? String ?? ""): unheard at the session's end")
        }
        heldModuleEvents = []
        if booted { _ = runtime.moduleEvent("{\"unheard\":true}") }
        endBackgroundActions()
    }
}
