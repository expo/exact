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
