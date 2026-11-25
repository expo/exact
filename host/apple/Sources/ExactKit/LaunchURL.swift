// A launch URL (`openURL` before boot) the app cannot hear is said once,
// as a URL arriving while it runs is (`navigate refused: …`): an app with
// no navigation root otherwise drops it without a word.
//
// @ref LLP 1038 D8
import Foundation
import UniformTypeIdentifiers

extension ExactSession {
    /// @ref LLP 1038 D8/D11 — pre-boot URLs are launch facts; the adapter consumes development links first.
    @discardableResult public func openURL(_ url: URL) -> Bool {
        guard state != .destroyed else { return false }
        return openIncoming(incomingLocation(url))
    }
    /// A link the reader followed — a `link href`, a text run's `href`, a
    /// Markdown link. A path naming one of the app's routes is a location
    /// for the navigation root, as the web's same-document link is (LLP 1038
    /// §7); anything else — a page, a file beside a document — is the
    /// containing app's to open (`openURL`).
    @discardableResult public func follow(_ href: String, target: String? = nil) -> Bool {
        guard state != .destroyed, !href.isEmpty else { return false }
        let href = resolvedHref(href)
        if href.hasPrefix("/"), !href.hasPrefix("//"), booted, runtime.routeMatches(href) { return navigate(href, target: target) }
        delegate?.exactSession(self, command: "openURL", args: [href])
        return true
    }
    /// `followed`: a person followed a link to it, so it applies as their input.
    @discardableResult public func navigate(_ location: String, followed: Bool = false) -> Bool {
        guard state != .destroyed, booted else { return false }
        let go = { [self] () -> Batch? in
            if let node = navigationRoot { return runtime.navigate(node.id, location, now: now()) }
            if runtime.routeMatches(location) { return runtime.hostGo(location, now: now()) }
            log("navigate refused: no navigation root handler or matching route")
            return nil
        }
        if followed { return applyInput(go).map { $0.error == nil } ?? false }
        guard let batch = go() else { return false }
        apply(batch)
        return batch.error == nil
    }

    /// The location the agent's `type <root> <text>` delivers: a path as
    /// written, and a URL with a scheme as the system's would arrive
    /// (`openURL`), so a drive opens a deep link without iOS's "Open in …?"
    /// confirmation that `simctl openurl` raises.
    func typedLocation(_ text: String) -> String {
        guard !text.hasPrefix("/"), let url = URL(string: text), url.scheme != nil else { return text }
        return incomingLocation(url)
    }

    func incomingLocation(_ url: URL) -> String {
        let href = url.absoluteString
        if isWebPageFile(url), let template = webTemplate, let location = runtime.protocolLocation(of: href, protocol: "file", template: template) {
            return location
        }
        return protocolHandlers.lazy.compactMap { handler in
            guard let name = handler["protocol"], let template = handler["url"] else { return nil }
            return self.runtime.protocolLocation(of: href, protocol: name, template: template)
        }.first ?? runtime.location(of: appRelativeURL(url))
    }

    private func appRelativeURL(_ url: URL) -> String {
        guard let base = webMount["base"], base != "/",
              let origin = webMount["origin"].flatMap(URL.init(string:)),
              let scheme = url.scheme?.lowercased(), ["http", "https"].contains(scheme),
              scheme == origin.scheme?.lowercased(), url.host?.lowercased() == origin.host?.lowercased(),
              (url.port ?? (scheme == "https" ? 443 : 80)) == (origin.port ?? (scheme == "https" ? 443 : 80)),
              var parts = URLComponents(url: url, resolvingAgainstBaseURL: false) else { return url.absoluteString }
        let path = parts.percentEncodedPath
        if path == String(base.dropLast()) { parts.percentEncodedPath = "/" }
        else if path.hasPrefix(base) { parts.percentEncodedPath = "/" + path.dropFirst(base.count) }
        else { return url.absoluteString }
        return parts.string ?? url.absoluteString
    }

    /// The `https` handler's template, when the app claims the web's links
    /// (a Mac browser, LLP 1030 D2).
    var webTemplate: String? {
        protocolHandlers.first { $0["protocol"]?.lowercased() == "https" }?["url"]
    }

    /// Whether `url` is an HTML file Launch Services brought to a browser
    /// (Finder's Open With, a double-click once the app owns HTML): it
    /// arrives at the `https` template as its `file:` URL, as a page does in
    /// Safari, unless the app's own `file_handlers` take it.
    package func isWebPageFile(_ url: URL) -> Bool {
        guard url.isFileURL, webTemplate != nil else { return false }
        #if os(macOS)
        if ExactDocuments.accepts(url) { return false }
        #endif
        let type = (try? url.resourceValues(forKeys: [.contentTypeKey]).contentType) ?? UTType(filenameExtension: url.pathExtension)
        return type.map { t in [UTType.html, UTType("public.xhtml")].contains { $0.map(t.conforms(to:)) ?? false } } ?? false
    }

    /// A notification's location and a host URL share the cold and warm
    /// incoming-route path, without requiring a URL scheme.
    @discardableResult package func openLocation(_ incoming: String) -> Bool {
        guard state != .destroyed else { return false }
        return openIncoming(runtime.location(of: incoming))
    }

    /// The manifest's `start_url` when it is not the root, which the Apple
    /// build bakes as `ExactStartURL` (W3C App Manifest).
    nonisolated(unsafe) static var startLocation = ExactEnv.appMetadata["ExactStartURL"] as? String

    /// A launch no link started opens at the start location, on either
    /// boot. A session already booted is not navigated: a restart (the dev
    /// menu's) boots again at the launch location the bridge keeps.
    func openStartLocation() {
        if !booted, launchLocation == nil, let start = Self.startLocation { _ = openIncoming(start) }
    }

    /// Before boot the first incoming location is the launch location and
    /// later ones navigate in order after it; afterwards each is one event.
    func openIncoming(_ location: String) -> Bool {
        if !booted {
            if launchLocation == nil { runtime.launch(location); launchLocation = location }
            else { pendingLaunchLocations.append(location) }
            return true
        }
        return navigate(location)
    }

    /// The window this session runs in, told before boot (LLP 1030.004
    /// D2): its name, which the page's `name` reads (`main` untold), and,
    /// rooted, a router that launches at `location`'s screen alone, a
    /// window opened at a route, presented without the window's sidebar.
    public func launchWindow(name: String, at location: String?, rooted: Bool) {
        guard !booted, state != .destroyed else { return }
        runtime.launchWindow(name, rooted: rooted)
        #if os(macOS)
        presenter.sidebar.rooted = rooted
        #endif
        if let location { _ = openIncoming(location) }
    }

    /// A link that targets this rooted window from elsewhere at another
    /// location: that screen becomes its only visit (LLP 1030.004 D2), as
    /// the person's input. The host has asked `beforeUnload` first.
    @discardableResult public func reroot(_ location: String) -> Bool {
        guard state != .destroyed, booted else { return false }
        return applyInput { runtime.hostRoot(location, now: now()) }?.error == nil
    }

    /// The selected visit's location, `nil` before the first route commit.
    public var location: String? {
        guard let url = routerOp?["url"] as? String else { return nil }
        let fragment = routerOp?["fragment"] as? String ?? ""
        return fragment.isEmpty || url.contains("#") ? url : url + "#" + fragment
    }

    /// Whether the selected visit is at `location`: its location, or its
    /// url when `location` names no fragment.
    public func shows(_ location: String) -> Bool {
        guard let shown = self.location else { return false }
        return shown == location || !location.contains("#") && routerOp?["url"] as? String == location
    }

    /// Where a link to one of the app's paths goes by its `target` (LLP
    /// 1030.004 D1): `nil` in place (none, empty, `_self`, `_parent`, `_top`,
    /// matched ASCII case-insensitively), else `_blank` or a window's name.
    /// A name that starts with `_` and is no keyword is `_blank`: such a
    /// window has no name another link could find.
    static func window(of target: String?) -> String? {
        guard let target, !target.isEmpty else { return nil }
        guard target.hasPrefix("_") else { return target }
        switch target.lowercased() {
        case "_self", "_parent", "_top": return nil
        default: return "_blank"
        }
    }

    /// A link to `location`, one of the app's paths: into the window its
    /// `target` names where the host opens windows (`openWindow`), else in
    /// place, as a browser without windows does.
    func navigate(_ location: String, target: String?) -> Bool {
        guard let window = Self.window(of: target), let openWindow, booted, runtime.routeMatches(location) else { return navigate(location, followed: true) }
        openWindow(location, window, false)
        return true
    }

    /// A fragment refers to the visit shown (LLP 1153 D5).
    private func resolvedHref(_ href: String) -> String {
        href.hasPrefix("#") ? String((routerOp?["url"] as? String ?? "/").prefix { $0 != "#" }) + href : href
    }

    func presentRoutePanel(_ args: [Any]) {
        guard let openPanel else { return log("panel unsupported: this host does not present route panels") }
        guard args.count == 2, let href = args[0] as? String, let target = args[1] as? String, !target.isEmpty,
              target != "main", !target.hasPrefix("_"), !target.isUnnamedWindowLabel else {
            return log("panel refused: expected an app route and a named secondary window")
        }
        let location = resolvedHref(href)
        guard location.hasPrefix("/"), !location.hasPrefix("//"), runtime.routeMatches(location) else {
            return log("panel refused: expected an app route")
        }
        openPanel(location, target)
    }

    /// The view that hears `navigate`: the navigation root (LLP 1038 D8).
    package var navigationRoot: NodeView? {
        presenter.views.values.first { $0.props["navigationBack"] != nil && $0.handlers.contains("navigate") }
    }

    /// The platform's Back from a route with no authored Back control: the
    /// location beneath, to the root it came from (LLP 1115 D5), as a
    /// popstate is to the web's. The platform's Back and Forward, here and
    /// below, are a person's input, so each applies as one (`applyInput`).
    package func navigate(_ location: String, at root: NodeView) {
        guard state != .destroyed, booted, presenter.views[root.id] === root, root.handlers.contains("navigate") else { return }
        applyInput(runtime.navigate(root.id, location, now: now()))
    }

    /// The platform's Back from a route with no authored Back control under
    /// a root with no `navigate` handler: the runner's own `back` (LLP 1115 D5).
    package func hostBack(_ key: String) {
        guard state != .destroyed, booted else { return }
        applyInput { runtime.hostBack(key, now: now()) }
    }

    /// The platform's Forward, keeping its direction through the commit.
    package func hostForward(at root: NodeView) {
        guard state != .destroyed, booted, presenter.views[root.id] === root else { return }
        applyInput(runtime.hostForward(root.id, now: now()))
    }

    /// Complete a native history jump without intermediate route commits.
    package func hostBack(from source: String, to destination: String) {
        guard state != .destroyed, booted else { return }
        applyInput { runtime.hostBack(from: source, to: destination, now: now()) }
    }

    /// After the first frame: a launch location that matched no route and
    /// has no navigation root to hear it is journaled, then forgotten.
    func refuseUnheardLaunch() {
        defer { launchLocation = nil; deliverPendingLaunchLocations() }
        guard let location = launchLocation, navigationRoot == nil, !runtime.routeMatches(location) else { return }
        log("launch URL refused: no navigation root handler (\(location))")
    }

    func deliverPendingLaunchLocations() {
        let locations = pendingLaunchLocations
        pendingLaunchLocations = []
        for location in locations { _ = navigate(location) }
    }
}

/// The adapter's URL batch entry. Each callback runs before the next URL.
/// A file is a document unless `webPage` says it is a page for a browser's
/// links (`isWebPageFile`).
package enum IncomingURLBatch {
    package static func deliver(_ urls: [URL], development: (URL) -> Bool, webPage: (URL) -> Bool = { _ in false },
                                document: (URL) -> Void, location: (URL) -> Void) {
        for url in urls {
            if url.isFileURL, !webPage(url) { document(url) }
            else if url.isFileURL || !development(url) { location(url) }
        }
    }
}

extension String {
    package var isUnnamedWindowLabel: Bool {
        guard hasPrefix("window-") else { return false }
        let digits = utf8.dropFirst(7)
        return !digits.isEmpty && digits.allSatisfy { (48...57).contains($0) }
    }
}
