import CExact
import Foundation

package enum AppRoots {
    /// Where the app and its module keep files: the app's roots, as the runtime
    /// configures storage, including a named agent store. An unnamed drive
    /// gets a process-local tree, made here: a package resolves an `app:/`
    /// path by a walk that starts at an existing root (`AppFileWalk`), and
    /// nothing else makes this one. Temporary files survive named relaunches,
    /// except the picker's separately managed `picked` directory.
    package static func of(session: ExactSession, agent: Bool = ExactEnv.agentMode,
                  namedStore: String? = ExactEnv.environment["EXACT_AGENT_STORAGE"]) -> (data: String, cache: String, temporary: String) {
        if (!agent || namedStore != nil), let data = session.appFileRoots["data"], let cache = session.appFileRoots["cache"],
           let temporary = session.appFileRoots["tmp"] {
            return (data, cache, temporary)
        }
        let id = Bundle.main.bundleIdentifier ?? "app"
        if agent {
            let base = (NSTemporaryDirectory() as NSString).appendingPathComponent("exact-agent-\(getpid())-\(session.runtime.rt)")
            let roots = (data: base + "/data", cache: base + "/cache", temporary: base + "/temporary")
            for root in [roots.data, roots.cache, roots.temporary] {
                try? FileManager.default.createDirectory(atPath: root, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
            }
            return roots
        }
        let home = NSHomeDirectory()
        let cache = home + "/Library/Caches/exact/" + id
        return (home + "/Library/Application Support/exact/" + id + "/data", cache + "/cache", cache + "/temporary")
    }
}

/// The app's directories behind `app:/`, so an `image` source can read the
/// app's own file (D7): told by the library at boot and with each picked
/// name, so a photo kept in `app:/data` shows after a relaunch with nothing
/// picked (recipes F18).
package enum AppFiles {
    private static let lock = NSLock()
    private static var roots: [String: URL] = [:]
    package static func learn(_ reply: [String: Any]) {
        guard let r = reply["roots"] as? [String: String] else { return }
        lock.lock(); defer { lock.unlock() }
        for (name, path) in r { roots[name] = URL(fileURLWithPath: path, isDirectory: true) }
    }
    /// The roots the library has now (`appRoots`), before the first frame's
    /// images load and again once storage is configured.
    @discardableResult package static func learn(_ runtime: Runtime) -> [String: String] {
        let reply = try? JSONSerialization.jsonObject(with: Data(runtime.ask(exact_app_roots, "").utf8))
        let object = reply as? [String: Any] ?? [:]
        learn(object)
        return object["roots"] as? [String: String] ?? [:]
    }
    /// Where an `app:/data|cache|tmp/…` path names, for a file the host
    /// writes there; nil for `..` or a root. A file the app names is read
    /// through `open` or `file`, never through this.
    package static func url(_ path: String) -> URL? {
        guard let at = place(path) else { return nil }
        return at.names.reduce(at.root) { $0.appendingPathComponent($1) }
    }
    /// The regular file `path` names, opened through no symbolic link
    /// (`AppFileWalk`), as `readFile` and a share open it; nil for any other.
    package static func open(_ path: String) -> FileHandle? {
        lock.lock(); let roots = roots; lock.unlock()
        return AppFileWalk.open(path, roots: roots)
    }
    /// The URL of the file `open` finds, for a reader that takes a URL (a
    /// player, picture in picture); nil where `open` refuses.
    static func file(_ path: String) -> URL? {
        guard let at = place(path), let handle = AppFileWalk.open(at.root, at.names) else { return nil }
        try? handle.close()
        return at.names.reduce(at.root) { $0.appendingPathComponent($1) }
    }
    private static func place(_ path: String) -> (root: URL, names: [String])? {
        guard path.hasPrefix("app:/") else { return nil }
        let parts = path.dropFirst(5).split(separator: "/", omittingEmptySubsequences: false).map(String.init)
        guard parts.count >= 2, !parts.dropFirst().contains(where: { $0.isEmpty || $0 == "." || $0 == ".." }) else { return nil }
        lock.lock(); let root = roots[parts[0]]; lock.unlock()
        return root.map { ($0, Array(parts.dropFirst())) }
    }
}
