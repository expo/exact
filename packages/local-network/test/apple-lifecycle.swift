import ExactPackage
import Foundation
import Network

@main
private struct LocalNetworkLifecycleTest {
    static func pump(until condition: () -> Bool, timeout: TimeInterval = 2.0) -> Bool {
        let deadline = Date(timeIntervalSinceNow: timeout)
        while !condition(), Date() < deadline {
            RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.01))
        }
        return condition()
    }

    static func main() {
        func makeModule(_ id: String) -> LocalNetworkModule {
            let ctx = ExactPackageContext(agent: false, session: id, changed: { _ in }, log: { _ in }, now: { 0 })
            return LocalNetworkModule(context: ctx)
        }

        // 1. Synchronous later("browseServices") immediately followed by destroy leaks no browser.
        let m1 = makeModule("s1")
        m1.later("browseServices", ["type": "_exacttest._tcp"], preferred: nil) { _ in }
        guard !LocalNetworkBrowse.all.isEmpty else { fail("expected browse registered synchronously") }
        var destroyed = false
        m1.destroy { destroyed = true }
        guard destroyed, LocalNetworkBrowse.all.isEmpty else {
            fail("destroy leaked browse: count=\(LocalNetworkBrowse.all.count)")
        }
        // A later call after destroy must not re-register a browser.
        var afterDestroy: [String: Any]?
        m1.later("browseServices", ["type": "_exacttest._tcp"], preferred: nil) { afterDestroy = $0 }
        guard afterDestroy?["error"] as? String == "AbortError", LocalNetworkBrowse.all.isEmpty else {
            fail("post-destroy later leaked browse: \(String(describing: afterDestroy))")
        }

        // 2. Concurrent resolveService callers honor independent timeouts without aborting each other.
        let m2 = makeModule("s2")
        let m3 = makeModule("s3")
        var shortReply: [String: Any]?
        var longReply: [String: Any]?
        m2.later("resolveService", ["name": "printer", "type": "_exacttest._tcp", "timeout": 50.0], preferred: nil) { shortReply = $0 }
        m3.later("resolveService", ["name": "printer", "type": "_exacttest._tcp", "timeout": 5000.0], preferred: nil) { longReply = $0 }
        guard pump(until: { shortReply != nil }, timeout: 2.0) else { fail("short timeout did not fire") }
        guard shortReply?["error"] as? String == "NotFoundError" else {
            fail("expected NotFoundError for short waiter, got \(String(describing: shortReply))")
        }
        guard longReply == nil else {
            fail("short timeout aborted longer waiter: \(String(describing: longReply))")
        }
        let key = LocalNetworkResolve.key(["name": "printer", "type": "_exacttest._tcp"])
        guard let resolver = LocalNetworkResolve.all[key] else { fail("resolver disappeared while long waiter active") }
        resolver.took("printer.local.", 8080, [])
        resolver.took(address: "192.0.2.10", added: true)
        guard longReply?["host"] as? String == "printer.local", longReply?["port"] as? Int == 8080 else {
            fail("long waiter did not receive resolved info: \(String(describing: longReply))")
        }

        // 3. Stale timer from an already-answered resolve does not abort a fresh resolve after pause/resume.
        let m4 = makeModule("s4")
        var firstResolve: [String: Any]?
        m4.later("resolveService", ["name": "speaker", "type": "_exacttest._tcp", "timeout": 60.0], preferred: nil) { firstResolve = $0 }
        let key2 = LocalNetworkResolve.key(["name": "speaker", "type": "_exacttest._tcp"])
        guard let r2 = LocalNetworkResolve.all[key2] else { fail("expected resolver for speaker") }
        r2.took("speaker.local.", 9000, [])
        r2.took(address: "192.0.2.20", added: true)
        guard firstResolve?["port"] as? Int == 9000 else { fail("first resolve failed: \(String(describing: firstResolve))") }
        // Background + foreground clears info and starts a fresh resolve with a longer timeout.
        m4.foreground(false)
        m4.foreground(true)
        var secondResolve: [String: Any]?
        m4.later("resolveService", ["name": "speaker", "type": "_exacttest._tcp", "timeout": 5000.0], preferred: nil) { secondResolve = $0 }
        // Wait past the first resolve's 60 ms deadline.
        let pastFirst = Date(timeIntervalSinceNow: 0.15)
        while Date() < pastFirst {
            RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.01))
        }
        guard secondResolve == nil else {
            fail("stale timer from first resolve aborted second resolve: \(String(describing: secondResolve))")
        }
        r2.took("speaker.local.", 9001, [])
        r2.took(address: "192.0.2.21", added: true)
        guard secondResolve?["port"] as? Int == 9001 else {
            fail("second resolve did not succeed: \(String(describing: secondResolve))")
        }

        // 4. Stopped NWBrowser clears its handlers so late callbacks cannot fire after pause/restart.
        var capturedBrowsers: [NWBrowser] = []
        LocalNetworkModule.browser = { capturedBrowsers.append($0) }
        defer { LocalNetworkModule.browser = nil }
        m4.later("browseServices", ["type": "_exacttest._tcp"], preferred: nil) { _ in }
        guard capturedBrowsers.count == 1 else { fail("expected first NWBrowser") }
        let b1 = capturedBrowsers[0]
        m4.foreground(false)
        guard b1.browseResultsChangedHandler == nil, b1.stateUpdateHandler == nil else {
            fail("stop() did not clear NWBrowser handlers")
        }
        m4.foreground(true)
        m4.later("browseServices", ["type": "_exacttest._tcp"], preferred: nil) { _ in }
        guard capturedBrowsers.count == 2 else { fail("expected second NWBrowser after foreground") }

        m2.destroy {}
        m3.destroy {}
        m4.destroy {}
        guard LocalNetworkBrowse.all.isEmpty, LocalNetworkResolve.all.isEmpty else {
            fail("cleanup left active entries: browses=\(LocalNetworkBrowse.all.count) resolves=\(LocalNetworkResolve.all.count)")
        }
        print("local-network lifecycle ok")
    }

    private static func fail(_ message: String) -> Never {
        FileHandle.standardError.write(Data("\(message)\n".utf8))
        exit(1)
    }
}
