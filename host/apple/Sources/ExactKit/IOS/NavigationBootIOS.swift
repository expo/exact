// A launch with no native chrome can commit its content before UIKit makes
// the navigation machinery. The same content views supply both frames.
#if os(iOS) || os(tvOS)
import UIKit

extension NavigationHost {
    private func canDeferOwner(_ p: Projection, batch: Batch) -> Bool {
        #if os(tvOS)
        return false
        #else
        guard !ExactEnv.agentMode, !presenter.menus.inTransition,
              let window = presenter.session?.view?.window, !window.isHidden,
              let scene = window.windowScene, scene.activationState != .background, scene.activationState != .unattached,
              presenter.session?.dataPreparing != true, presenter.session?.preparedGeneration == nil, presenter.session?.dataGeneration == nil,
              presenter.session?.natives.hatchesConnected != true, presenter.session?.natives.firstFrameTags.isEmpty == true,
              p.stacks.flatMap({ $0 }).allSatisfy({ $0.props["navigationBar"] == "none" && ($0.props["navigationPresentation"] ?? "").isEmpty }),
              p.tabs.map({ $0.panels.count <= 5 && $0.tablist.wrote("appearance") && $0.tablist.style["appearance"]?.string == "none" }) ?? true else { return false }
        // Offset routes need the native owner's sibling and safe-area projection.
        func frame(_ node: NodeView) -> CGRect {
            guard let op = batch.ops.last(where: { $0.op == .frame && $0.id == node.id }) else { return node.frame }
            return CGRect(x: op.x, y: op.y, width: op.w, height: op.h)
        }
        let bounds = CGRect(origin: .zero, size: frame(p.root).size)
        guard !bounds.isEmpty, let route = p.chosen.last, frame(route.node) == bounds else { return false }
        // No presentation or bar may acquire an owner in the partial batch.
        return !batch.ops.contains { $0.op == .command } && !presenter.views.values.contains { node in
            node.kind == "list" && (node.hasRowCells || node.style["appearance"]?.string != "none") || node.kind == "video" || node.kind == "audio" || node.kind == "iframe" || PackagePictures.mayServe(node.imageSource ?? node.props["imageSource"] ?? "")
                || node.props["accessibilityRole"] == "toolbar" || node.props["type"] == "search"
                || node.props["accessibilityRole"] == "search"
                || node.props["popover"] != nil || node.props["tip"] != nil
                || node.props["semanticTag"] == "dialog" && node.props["accessibilityRole"] != "alertdialog"
                || node.props["accessibilityRole"] == "alertdialog" && presenter.menus.isOpen(node)
        }
        #endif
    }

    func deferInitialOwner(_ p: Projection, batch: Batch) -> Bool {
        // The hand-off's run-loop order is observed, not documented, and was qualified on iOS 27.
        guard #available(iOS 27.0, tvOS 18.0, *), presenter.session?.firstDrawMs == nil,
              canDeferOwner(p, batch: batch), presenter.session?.hatchesOwnWindow == true, let view = presenter.session?.view, let window = view.window, let parent = ViewPresentation.controller(for: p.root, layout: true),
              parent === window.rootViewController, view.frame == parent.view.bounds, parent.additionalSafeAreaInsets == .zero else { return false }
        #if os(iOS)
        // Rotation-capable launches use ordinary ownership; a bounds change also ends a wait below.
        let orientations = UIApplication.shared.supportedInterfaceOrientations(for: window).intersection(parent.supportedInterfaceOrientations)
        guard window.traitCollection.userInterfaceIdiom == .phone, orientations == .portrait,
              let scene = window.windowScene, OrientationLock.bits(orientations) == OrientationLock.bit(scene.effectiveGeometry.interfaceOrientation),
              parent.transitionCoordinator == nil else { return false }
        #endif
        let holder = PlainTab()
        holder.host = self
        holder.bootContent = true
        bootOwner = holder
        presenter.session?.runtime.beforeEvent = { [weak self] in self?.finishBootOwner() }
        // The draw receipt schedules a run-loop block after the content flush;
        // this prearmed observer after CA also catches a draw in its observer pass.
        // Exact's activation stays held until either opportunity installs the owner.
        let observer = CFRunLoopObserverCreateWithHandler(nil, CFRunLoopActivity.beforeWaiting.rawValue | CFRunLoopActivity.exit.rawValue, true, 2_000_001) { [weak self] _, _ in
            guard let self, self.presenter.session?.firstDrawMs != nil else { return }
            self.finishBootOwner()
        }
        bootOwnerFallback = observer
        CFRunLoopAddObserver(CFRunLoopGetMain(), observer, .commonModes)
        if let tabs = p.tabs {
            tabPanels = tabs.panels.map(\.id)
            adoptedTablist = tabs.tablist.id
            tablistAppearanceNone = true
        }
        prepareRoutes(p.chosen, in: holder)
        holder.show(p.chosen.last)
        parent.addChild(holder)
        holder.view.frame = ownerFrame(holder, in: p.root)
        holder.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        p.root.addSubview(holder.view)
        holder.view.setPaintForeground(aboveAuthored: false)
        holder.didMove(toParent: parent)
        fitOwner()
        placeOwner()
        holder.view.layoutIfNeeded()
        presenter.owners.publish()
        reportCovers()
        refitForBars()
        guard bootOwner === holder else { return false }
        let center = NotificationCenter.default
        for name in [UIScene.willDeactivateNotification, UIScene.didEnterBackgroundNotification, UIWindow.didBecomeHiddenNotification] {
            bootOwnerNotifications.append(center.addObserver(forName: name, object: nil, queue: .main) { [weak self, weak window = view.window] note in
                guard let self, let window, (note.object as? UIWindow) === window || (note.object as? UIScene) === window.windowScene else { return }
                self.finishBootOwner()
            })
        }
        return true
    }

    func updateBootOwner(_ p: Projection, batch: Batch) -> Bool {
        guard let holder = bootOwner else { return false }
        // Layout reports may keep waiting. A changed route, tab structure or
        // native chrome installs with the holder still in its window.
        guard canDeferOwner(p, batch: batch),
              p.chosen.last === holder.route,
              p.tabs?.panels.map(\.id) ?? [] == tabPanels,
              p.tabs?.tablist.id == adoptedTablist else { beginBootTransfer(); return false }
        prepareRoutes(p.chosen, in: holder)
        fitOwner()
        placeOwner()
        holder.view.layoutIfNeeded()
        reportCovers()
        presenter.owners.publish()
        return bootOwner === holder
    }

    /// Hold first-draw activation behind the already armed hand-off.
    func bootOwnerDrawn(_ ready: @escaping () -> Void) -> Bool {
        guard bootOwner != nil else { return false }
        bootOwnerReady = ready
        let mode = CFRunLoopCopyCurrentMode(CFRunLoopGetMain())?.rawValue ?? CFRunLoopMode.commonModes.rawValue
        let holder = bootOwner
        CFRunLoopPerformBlock(CFRunLoopGetMain(), mode) { [weak self, weak holder] in
            guard let self, let holder, self.bootOwner === holder else { return }
            self.finishBootOwner()
        }
        CFRunLoopWakeUp(CFRunLoopGetMain())
        return true
    }

    /// Boot work that can load code waits with first pixel's continuation.
    func afterBootOwner(_ work: @escaping () -> Void) {
        if bootOwner != nil || transferringBootOwner != nil { bootOwnerWork.append(work) } else { work() }
    }

    private func releaseBootWork() {
        let work = bootOwnerWork
        bootOwnerWork = []
        work.forEach { DispatchQueue.main.async(execute: $0) }
    }

    func finishBootOwner() {
        guard bootOwner != nil else { return }
        // Layout can ask for a size change while the initial install is still
        // guarded. Its caller completes the real install before retiring this holder.
        if syncing { beginBootTransfer(); return }
        UIView.performWithoutAnimation {
            beginBootTransfer()
            defer { endBootTransfer() }
            sync(Batch(ops: [], timers: false, motion: false, clock: nil, error: nil))
            presenter.session?.view?.layoutIfNeeded()
            presenter.paintVisibleText()
        }
    }

    private func cancelBootWait() {
        if let observer = bootOwnerFallback { CFRunLoopObserverInvalidate(observer) }
        bootOwnerFallback = nil
        bootOwnerNotifications.forEach(NotificationCenter.default.removeObserver)
        bootOwnerNotifications = []
    }

    private func beginBootTransfer() {
        cancelBootWait()
        if let holder = bootOwner { transferringBootOwner = holder; bootOwner = nil }
    }

    func endBootTransfer() {
        guard let holder = transferringBootOwner else { return }
        guard primaryOwner != nil else { reset(clearFocus: false); return }
        holder.releaseBootContent()
        holder.willMove(toParent: nil)
        holder.view.removeFromSuperview()
        holder.removeFromParent()
        transferringBootOwner = nil
        presenter.session?.runtime.beforeEvent = nil
        presenter.owners.publish()
        reportCovers()
        let ready = bootOwnerReady
        bootOwnerReady = nil
        releaseBootWork()
        ready?()
    }

    func retireBootOwner() {
        cancelBootWait()
        let holders = [bootOwner, transferringBootOwner].compactMap { $0 }
        bootOwner = nil
        transferringBootOwner = nil
        presenter.session?.runtime.beforeEvent = nil
        for holder in holders {
            holder.show(nil)
            holder.willMove(toParent: nil)
            holder.view.removeFromSuperview()
            holder.removeFromParent()
        }
        let ready = bootOwnerReady
        bootOwnerReady = nil
        if !holders.isEmpty || !bootOwnerWork.isEmpty { releaseBootWork() }
        ready?()
    }
}
#endif
