// Attached agent sessions (LLP 1012.000): a driver joins the development app a
// person already has open and reads the one window that person approved, on the
// app's own wall clock. Only a local development build compiles this file
// (`EXACT_ATTACH`, which `build.mjs` sets with the `ExactBuildPurpose` stamp),
// and its listener opens only when the person enables it: Develop ▸ Enable
// Agent Attach on the Mac, the developer menu's Enable Agent Attach in the iOS
// simulator. The wire is JSON lines over a Unix socket: a handshake, then
// `{"request":n,"op":…}` requests, each answered `{"request":n,…}`. What the
// person sees (the window, the approval, the indicator, the picture) is each
// platform's `AttachSurface`: `Mac/AgentAttachMac.swift`, `IOS/AgentAttachIOS.swift`.
#if EXACT_ATTACH && (os(macOS) || (os(iOS) && targetEnvironment(simulator)))
import Foundation
import Darwin
#if os(macOS)
import Security
#endif

public enum AgentAttach {
    static let version = 1
    static let requestLimit = 1 << 20, replyLimit = 32 << 20, queueLimit = 64 << 20, pendingLimit = 4
    /// Seconds: a handshake to arrive, the person to approve it, one request to
    /// be answered, one write to drain.
    static let handshakeTimeout = 10.0, approvalTimeout = 60.0, requestTimeout = 30.0, writeTimeout = 10

    /// Whether this binary may listen: a build stamped for local development
    /// with development trust baked in, said affirmatively. A missing stamp or
    /// trust leaves it off, and so does a launch the driver owns.
    public static let admitted = ExactEnv.appMetadata["ExactBuildPurpose"] as? String == "local-development"
        && (BakedCompatibility.json["embedded"] as? [String: Any])?["trust"] as? String == "development"
        && !ExactEnv.agentCarried

    nonisolated(unsafe) static var listener: AttachListener?
    /// The one connection: asking, or attached. Main thread.
    nonisolated(unsafe) static var current: AttachConnection?
    static var listening: Bool { listener != nil }
    /// Whether a driver is connected, asking or attached: Detach has something to end.
    static var connected: Bool { current.map { !$0.isRetired } ?? false }

    /// Open the listener and publish its marker, or say why not.
    static func enable() {
        guard listener == nil else { return }
        do { listener = try AttachListener.open() }
        catch { AttachSurface.couldNotStart("\(error)") }
    }

    /// Revoke every grant, close the connection and unpublish the listener
    /// until the person enables it again.
    static func detach(_ reason: String) {
        current?.retire(reason)
        listener?.close()
        listener = nil
    }

    /// A connection ended: its indicator, approval and grants go with it. Main thread.
    static func ended(_ connection: AttachConnection) {
        connection.reads = false
        connection.input = false
        connection.cancelCalls()
        connection.surface?.end()
        if current === connection { current = nil }
    }

    /// A new client of `origin`, after the peer check. One driver at a time. Main thread.
    static func accepted(_ connection: AttachConnection, from origin: AttachListener) {
        guard listener === origin else {
            connection.refuseAndClose(["error": "attach: Agent Attach was turned off in the app", "refused": "off"])
            return
        }
        if let current, !current.isRetired {
            connection.refuseAndClose(["error": "busy: another agent is attached to this app (Detach Agent, in the app, ends it)", "refused": "busy"])
            return
        }
        current = connection
        connection.start()
    }

    /// Whether a driver is attached, its reads allowed: Allow Agent Input has a connection to grant.
    static var attached: Bool { current.map { !$0.isRetired && $0.reads } ?? false }
    /// Whether the attached driver may tap, type and call.
    static var inputAllowed: Bool { attached && current?.input == true }
    /// Allow Agent Input, a checked command: grants the attached connection input, or revokes it and leaves its reads.
    static func toggleInput() {
        guard let current, attached else { return }
        current.allowInput(!current.input)
    }

    /// Enable Agent Attach, a checked command: on, or off with any connection ended.
    static func toggle() {
        if listening { detach("Agent Attach was turned off in the app") } else { enable() }
    }

    /// Unlink the socket and marker as the process ends.
    static func unpublish() { listener?.unlinkPaths() }
}

/// The socket and the marker that names it, for one launch.
final class AttachListener {
    let fd: Int32
    let socketPath: String, markerPath: String
    let nonce: String
    private let source: DispatchSourceRead
    /// Accepted connections not yet seen by the main thread, at most `AgentAttach.pendingLimit`.
    private let lock = NSLock()
    private var pending = 0

    enum Failure: Error, CustomStringConvertible {
        case sandboxed, directory(String, String), path(String), system(String, Int32)
        var description: String {
            switch self {
            case .sandboxed: return "A sandboxed app cannot be attached to yet (LLP 1012.000 piece 9)."
            case let .directory(path, why): return "\(path) \(why), so the socket was not opened there."
            case let .path(path): return "\(path) is longer than a Unix socket path can be (103 bytes)."
            case let .system(call, code): return "\(call): \(String(cString: strerror(code)))"
            }
        }
    }

    static func open() throws -> AttachListener {
        let temp = userTemp(), pid = getpid(), uid = getuid()
        #if os(macOS)
        if sandboxed() { throw Failure.sandboxed }
        var socketPath = (try privateDirectory((temp as NSString).appendingPathComponent("exact-attach"), uid: uid) as NSString).appendingPathComponent("\(pid)")
        // LLP 1111.000 D4's fallback for a long per-user path.
        if socketPath.utf8.count > 103 { socketPath = try privateDirectory("/tmp/exact-\(uid)", uid: uid) + "/a\(pid)" }
        #else
        // A simulator app's container `tmp` is past the 103 bytes a socket path can be (LLP 1012.000 piece 1); the
        // simulator's processes share the Mac's `/tmp`.
        let socketPath = try privateDirectory("/tmp/exact-\(uid)", uid: uid) + "/a\(pid)"
        #endif
        if socketPath.utf8.count > 103 { throw Failure.path(socketPath) }
        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { throw Failure.system("socket", errno) }
        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        withUnsafeMutableBytes(of: &addr.sun_path) { raw in
            for (i, b) in socketPath.utf8.enumerated() { raw[i] = b }
            raw[socketPath.utf8.count] = 0
        }
        unlink(socketPath)
        let bound = withUnsafePointer(to: &addr) { $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { bind(fd, $0, socklen_t(MemoryLayout<sockaddr_un>.size)) } }
        guard bound == 0, chmod(socketPath, 0o600) == 0, listen(fd, 4) == 0 else {
            let code = errno
            Darwin.close(fd); unlink(socketPath)
            throw Failure.system("bind \(socketPath)", code)
        }
        var bytes = [UInt8](repeating: 0, count: 16)
        arc4random_buf(&bytes, bytes.count)
        let nonce = bytes.map { String(format: "%02x", $0) }.joined()
        let markerPath = (temp as NSString).appendingPathComponent("exact-attach-\(pid).json")
        let info = ExactEnv.appMetadata
        let marker: [String: Any] = [
            "v": AgentAttach.version, "pid": Int(pid), "started": processStart(), "bundle": Bundle.main.bundleIdentifier ?? "",
            "executable": Bundle.main.executablePath ?? "", "socket": socketPath, "nonce": nonce,
            "commit": info["ExactCommit"] ?? NSNull(), "dirty": info["ExactCommitDirty"] ?? NSNull(),
        ]
        do { try publish(marker, at: markerPath) }
        catch { Darwin.close(fd); unlink(socketPath); throw error }
        let listener = AttachListener(fd: fd, socketPath: socketPath, markerPath: markerPath, nonce: nonce)
        installExitCleanup()
        return listener
    }

    private init(fd: Int32, socketPath: String, markerPath: String, nonce: String) {
        self.fd = fd; self.socketPath = socketPath; self.markerPath = markerPath; self.nonce = nonce
        source = DispatchSource.makeReadSource(fileDescriptor: fd, queue: DispatchQueue(label: "exact.attach.accept"))
        source.setEventHandler { [weak self, fd, nonce] in
            let client = accept(fd, nil, nil)
            guard client >= 0 else { return }
            var uid: uid_t = 0, gid: gid_t = 0
            // Only a process running as this user; the nonce then names this listener.
            guard let self, getpeereid(client, &uid, &gid) == 0, uid == getuid(),
                  lock.withLock({ () -> Bool in
                      guard self.pending < AgentAttach.pendingLimit else { return false }
                      self.pending += 1
                      return true
                  }) else { Darwin.close(client); return }
            // The slot is held until the connection starts, or, refused, until its descriptor closes.
            let connection = AttachConnection(fd: client, nonce: nonce) { [weak self] in self?.lock.withLock { self?.pending -= 1 } }
            DispatchQueue.main.async { AgentAttach.accepted(connection, from: self) }
        }
        source.setCancelHandler { [fd] in Darwin.close(fd) }
        source.resume()
    }

    func close() {
        source.cancel()
        unlinkPaths()
    }

    func unlinkPaths() {
        unlink(markerPath)
        unlink(socketPath)
    }

    /// Where the marker goes: the Mac's per-user temporary directory, beside `exact-trace-<pid>.ready`, or the
    /// simulator app's container `tmp`, which `simctl get_app_container` names to the driver.
    static func userTemp() -> String {
        #if os(macOS)
        var buffer = [CChar](repeating: 0, count: Int(PATH_MAX))
        let n = confstr(_CS_DARWIN_USER_TEMP_DIR, &buffer, buffer.count)
        return n > 0 ? String(cString: buffer) : NSTemporaryDirectory()
        #else
        return NSTemporaryDirectory()
        #endif
    }

    /// `path` as a `0700` directory this user owns and no link leads to, made if missing.
    static func privateDirectory(_ path: String, uid: uid_t) throws -> String {
        if mkdir(path, 0o700) != 0, errno != EEXIST { throw Failure.system("mkdir \(path)", errno) }
        var st = stat()
        guard lstat(path, &st) == 0 else { throw Failure.system("lstat \(path)", errno) }
        if (st.st_mode & S_IFMT) != S_IFDIR { throw Failure.directory(path, "is not a directory") }
        if st.st_uid != uid { throw Failure.directory(path, "is owned by another user") }
        if st.st_mode & 0o077 != 0 { throw Failure.directory(path, "is open to other users") }
        return path
    }

    /// The marker, written whole beside its final name and renamed into place.
    static func publish(_ marker: [String: Any], at path: String) throws {
        let data = try JSONSerialization.data(withJSONObject: marker, options: [.sortedKeys])
        let staging = path + ".part"
        unlink(staging)
        let fd = Darwin.open(staging, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW, 0o600)
        guard fd >= 0 else { throw Failure.system("open \(staging)", errno) }
        let written = data.withUnsafeBytes { write(fd, $0.baseAddress, $0.count) }
        Darwin.close(fd)
        guard written == data.count, rename(staging, path) == 0 else { unlink(staging); throw Failure.system("write \(path)", errno) }
    }

    #if os(macOS)
    /// Whether this process is signed into the App Sandbox, by its own entitlement; a
    /// process that cannot read its entitlements is taken to be.
    static func sandboxed() -> Bool {
        guard let task = SecTaskCreateFromSelf(nil) else { return true }
        let value = SecTaskCopyValueForEntitlement(task, "com.apple.security.app-sandbox" as CFString, nil)
        return (value as? Bool) ?? (value != nil)
    }
    #endif

    /// The process's start, seconds since 1970, which the driver checks against `ps`.
    static func processStart() -> Double {
        var info = kinfo_proc(), size = MemoryLayout<kinfo_proc>.stride
        var mib: [Int32] = [CTL_KERN, KERN_PROC, KERN_PROC_PID, getpid()]
        guard sysctl(&mib, 4, &info, &size, nil, 0) == 0 else { return 0 }
        let start = info.kp_proc.p_starttime
        return Double(start.tv_sec) + Double(start.tv_usec) / 1_000_000
    }

    nonisolated(unsafe) private static var cleanupInstalled = false
    private static func installExitCleanup() {
        guard !cleanupInstalled else { return }
        cleanupInstalled = true
        atexit { AgentAttach.unpublish() }
    }
}

/// One driver's connection: its grants, its pinned window and its replies.
/// The grants and target are the main thread's; the reader and writer run
/// beside it, so a stalled request never hides a hangup.
final class AttachConnection {
    let fd: Int32
    private let nonce: String
    private let lock = NSLock()
    private var retired = false, queued = 0
    /// Times are `systemUptime` seconds, which no clock change moves.
    private var outstanding: (id: Int, since: TimeInterval)?
    private var handshaken = false, approved = false, started = false, flushing = false
    /// Gives back the listener's slot, once: at start, or at the close of one never started.
    private var release: (() -> Void)?
    private let accepted = ProcessInfo.processInfo.systemUptime
    /// Writes, the shutdowns and the close, in order. `open` is its own.
    private let writer = DispatchQueue(label: "exact.attach.writer")
    private var open = true
    private var watchdog: DispatchSourceTimer?

    // Main thread.
    /// The pinned window, its session, the person's approval and the indicator.
    var surface: AttachSurface?
    var reads = false
    /// The second grant (LLP 1012.000 D1): tap, type and a declared tool's call. Main thread.
    var input = false
    /// The tool calls this connection made: one still queued when input ends is cancelled, never run. Main thread.
    private var calls: [Int] = []

    init(fd: Int32, nonce: String, release: @escaping () -> Void) {
        self.fd = fd
        self.nonce = nonce
        self.release = release
        var on: Int32 = 1
        setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &on, socklen_t(MemoryLayout<Int32>.size))
        // A blocked write wakes each second to check its reply's deadline (`enqueue`).
        var deadline = timeval(tv_sec: 1, tv_usec: 0)
        setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &deadline, socklen_t(MemoryLayout<timeval>.size))
    }

    var isRetired: Bool { lock.withLock { retired } }

    func start() {
        lock.withLock { started = true }
        releaseSlot()
        let timer = DispatchSource.makeTimerSource(queue: .global())
        timer.schedule(deadline: .now() + 1, repeating: 1)
        timer.setEventHandler { [weak self] in self?.check() }
        timer.resume()
        watchdog = timer
        Thread { [self] in readLoop() }.start()
    }

    /// A handshake or approval that never came, or a request past its bound.
    private func check() {
        let now = ProcessInfo.processInfo.systemUptime
        let (shook, ok, request) = lock.withLock { (handshaken, approved, outstanding) }
        // The driver is told why: it waits longer than these, so an unexplained close would read as Detach.
        if !shook, now - accepted > AgentAttach.handshakeTimeout {
            refuseAndClose(["error": "handshake: none arrived within \(Int(AgentAttach.handshakeTimeout)) s", "refused": "handshake"])
        } else if !ok, now - accepted > AgentAttach.approvalTimeout {
            refuseAndClose(["error": "attach: not allowed in the app within \(Int(AgentAttach.approvalTimeout)) s", "refused": "declined"])
        }
        else if let request, now - request.since > AgentAttach.requestTimeout {
            retire("request \(request.id) was not answered within \(Int(AgentAttach.requestTimeout)) s")
        }
    }

    private func readLoop() {
        var pending = Data(), buffer = [UInt8](repeating: 0, count: 65536)
        reading: while true {
            let n = read(fd, &buffer, buffer.count)
            if n < 0, errno == EINTR { continue }
            if n <= 0 { retire(n == 0 ? "the agent hung up" : "read: \(String(cString: strerror(errno)))"); break }
            pending.append(buffer, count: n)
            while let end = pending.firstIndex(of: 0x0a) {
                let line = pending.subdata(in: pending.startIndex..<end)
                pending.removeSubrange(pending.startIndex...end)
                if line.count > AgentAttach.requestLimit { retire("a request over 1 MiB"); break reading }
                if !receive(line) { break reading }
            }
            if pending.count > AgentAttach.requestLimit { retire("a request over 1 MiB"); break }
        }
        // After a refusal the driver is given the grace `retire` gives it before the close.
        writer.asyncAfter(deadline: .now() + (lock.withLock { flushing } ? 2 : 0)) { [self] in closeDescriptor() }
    }

    /// Writer queue.
    private func stopReading() {
        guard open else { return }
        shutdown(fd, SHUT_RDWR)
        if !lock.withLock({ started }) { closeDescriptor() }
    }

    /// Writer queue.
    private func closeDescriptor() {
        guard open else { return }
        open = false
        Darwin.close(fd)
        releaseSlot()
    }

    private func releaseSlot() {
        let give = lock.withLock { () -> (() -> Void)? in defer { release = nil }; return release }
        give?()
    }

    /// One line from the driver. False once the connection is spent.
    private func receive(_ line: Data) -> Bool {
        if isRetired { return false }
        guard lock.withLock({ handshaken }) else { return handshake(line) }
        guard lock.withLock({ approved }) else { retire("a request before the person approved reads"); return false }
        switch AttachRequest.admit(line) {
        case let .failure(refusal):
            send(refusal.reply)
            return true
        case let .success((id, request)):
            let busy = lock.withLock { () -> Bool in
                if outstanding != nil { return true }
                outstanding = (id, ProcessInfo.processInfo.systemUptime)
                return false
            }
            if busy { send(["request": id, "error": "busy: one request at a time", "refused": "busy"]); return true }
            DispatchQueue.main.async { [self] in
                let reply = dispatch(id, request)
                // A reply past its deadline is never sent, though the watchdog has not yet looked. The request stays
                // outstanding, under the watchdog, until its reply is written.
                let late = lock.withLock { outstanding.map { ProcessInfo.processInfo.systemUptime - $0.since > AgentAttach.requestTimeout } ?? true }
                if late { retire("request \(id) was not answered within \(Int(AgentAttach.requestTimeout)) s") } else { enqueue(Data(reply.utf8), answering: id) }
            }
            return true
        }
    }

    private func handshake(_ line: Data) -> Bool {
        guard let object = try? JSONSerialization.jsonObject(with: line) as? [String: Any],
              Set(object.keys).isSubset(of: ["attach", "nonce", "session"]), AttachRequest.distinctKeys(line),
              AttachRequest.count(object["attach"]) == AgentAttach.version, let offered = object["nonce"] as? String else {
            refuseAndClose(["error": "handshake: expected {\"attach\":\(AgentAttach.version),\"nonce\":…,\"session\"?:…} as the first line", "refused": "handshake"])
            return false
        }
        guard offered.utf8.count == nonce.utf8.count, zip(offered.utf8, nonce.utf8).reduce(0, { $0 | ($1.0 ^ $1.1) }) == 0 else {
            refuseAndClose(["error": "handshake: the nonce names another listener", "refused": "handshake"])
            return false
        }
        if let wanted = object["session"], !(wanted is String) {
            refuseAndClose(["error": "handshake: session is a window's session label", "refused": "handshake"])
            return false
        }
        lock.withLock { handshaken = true }
        let wanted = object["session"] as? String
        DispatchQueue.main.async { [self] in pin(wanted) }
        return true
    }

    /// Pin the one window, then ask its person. Main thread.
    private func pin(_ wanted: String?) {
        guard !isRetired else { return }
        let surface: AttachSurface
        switch AttachSurface.pin(wanted) {
        case let .failure(refusal): refuseAndClose(refusal.reply); return
        case let .success(pinned): surface = pinned
        }
        self.surface = surface
        surface.watch { [weak self] why in self?.retire(why) }
        send(["attach": AgentAttach.version, "waiting": "approval", "pid": Int(getpid())])
        surface.ask { [weak self] allowed in
            guard let self else { return }
            guard allowed == true, !self.isRetired else {
                if allowed == false { self.refuseAndClose(["error": "attach: the person did not allow it", "refused": "declined"]) }
                return
            }
            self.approve()
        }
    }

    /// Reads are granted: say so in the window, then to the driver. Main thread.
    private func approve() {
        guard let surface, let session = surface.session, surface.isOpen, !isRetired else { retire("the attached window closed"); return }
        // The grant and its deadline are one decision, which the watchdog's expiry cannot fall between.
        let inTime = lock.withLock { () -> Bool in
            guard !retired, ProcessInfo.processInfo.systemUptime - accepted <= AgentAttach.approvalTimeout else { return false }
            approved = true
            return true
        }
        guard inTime else { retire("not approved in the app within \(Int(AgentAttach.approvalTimeout)) s"); return }
        reads = true
        surface.showAttached()
        let info = ExactEnv.appMetadata
        var reply: [String: Any] = ["attached": true, "pid": Int(getpid()), "session": session.label, "window": surface.title,
              "grants": ["read": true, "input": false],
              "build": ["commit": info["ExactCommit"] ?? NSNull(), "dirty": info["ExactCommitDirty"] ?? NSNull(),
                        "appCommit": info["ExactAppCommit"] ?? NSNull(), "builtAt": info["ExactBuildTime"] ?? NSNull()],
              "planDigest": session.runtime.planDigest() ?? NSNull()]
        if let number = surface.number { reply["windowNumber"] = number }
        send(reply)
    }

    /// Grant or revoke input; reads stay. Main thread.
    func allowInput(_ on: Bool) {
        guard reads, !isRetired, let surface else { return }
        input = on
        if !on { cancelCalls() }
        surface.showAttached(input: on)
        FileHandle.standardError.write(Data("exact: agent input \(on ? "allowed" : "revoked") in the app\n".utf8))
    }

    /// Cancel the calls this connection made: one still waiting for another to end is dropped; one running runs to
    /// its end, since what it started is the app's (runner `calls.rs`). Main thread.
    func cancelCalls() {
        guard let session = surface?.session else { calls = []; return }
        for id in calls { _ = session.agent("{\"op\":\"cancel\",\"id\":\(id)}") }
        calls = []
    }

    /// Whether input may still take effect now: the connection live, the grant held, its request within its time,
    /// and the app in front with no person's gesture down. Checked again as the effect is made, after the work
    /// before it, in which the main loop may have run. Main thread.
    func inputStill() -> Bool {
        let live = lock.withLock { !retired && (outstanding.map { ProcessInfo.processInfo.systemUptime - $0.since <= AgentAttach.requestTimeout } ?? false) }
        return live && input && surface?.inputRefusal() == nil
    }

    /// Answer one admitted request against the pinned target. Main thread.
    private func dispatch(_ id: Int, _ request: AttachRequest) -> String {
        guard !isRetired, reads else { return AttachRequest.raw(id, ["error": "not attached"]) }
        guard let surface, let session = surface.session, surface.isOpen, session.state != .destroyed else {
            retire("the attached window closed")
            return AttachRequest.raw(id, ["error": "the attached window closed"])
        }
        switch request {
        case .grants: return AttachRequest.raw(id, ["read": reads, "input": input])
        case let .input(form):
            // Checked here, on the main thread, as the effect is made: a revocation or a hangup before now leaves none.
            guard input else { return AttachRequest.raw(id, ["error": "input-not-granted: \(AttachRequest.ungranted)", "refused": "input-not-granted"]) }
            // The dispatch log and a call's record are reads: the runner's own touch may still be down in the log.
            switch form {
            case .log, .called: break
            default: if let refusal = surface.inputRefusal() { return AttachRequest.raw(id, refusal) }
            }
            if case let .call(tool, args) = form {
                // A declared tool through the agent's own door, as the runner's `call` takes it; the host chooses the door.
                let request = (try? JSONSerialization.data(withJSONObject: ["op": "call", "tool": tool, "args": args, "door": "agent"])).map { String(decoding: $0, as: UTF8.self) } ?? "{}"
                let reply = session.agent(request)
                if let data = reply.data(using: .utf8), let call = AttachRequest.count((try? JSONSerialization.jsonObject(with: data) as? [String: Any])?["call"]) { calls.append(call) }
                return AttachRequest.splice(id, reply)
            }
            if case let .called(call) = form {
                guard calls.contains(call) else { return AttachRequest.raw(id, ["error": "target: call \(call) is not one this connection made", "refused": "target"]) }
                return AttachRequest.splice(id, session.agent("{\"op\":\"called\",\"id\":\(call)}"))
            }
            Agent.inputGate = { [weak self] in self?.inputStill() ?? false }
            defer { Agent.inputGate = nil }
            let reply = surface.perform(form, session: session)
            // An aim is the go-ahead for a touch the driver's runner plays after it: given only if the grant and the
            // front still hold once the aim, which may wait out the keyboard, is made.
            if case .aim = form, !inputStill() {
                return AttachRequest.raw(id, surface.inputRefusal() ?? ["error": "input-not-granted: \(AttachRequest.ungranted)", "refused": "input-not-granted"])
            }
            return AttachRequest.raw(id, reply)
        default: return AttachRead.answer(id, request, session: session, surface: surface)
        }
    }

    // Replies. Any thread.

    func send(_ object: [String: Any]) {
        guard let data = try? JSONSerialization.data(withJSONObject: object) else { return }
        enqueue(data)
    }

    /// Queue one reply line. Once the connection is retired only a `final`
    /// one, the refusal that ends it, is still written.
    private func enqueue(_ body: Data, final: Bool = false, answering request: Int? = nil) {
        var line = body
        if line.count > AgentAttach.replyLimit {
            let id = (try? JSONSerialization.jsonObject(with: body) as? [String: Any])?["request"] ?? NSNull()
            line = (try? JSONSerialization.data(withJSONObject: ["request": id, "error": "the reply is over 32 MiB", "refused": "reply-size"])) ?? Data()
        }
        line.append(0x0a)
        let over = lock.withLock { () -> Bool in
            if retired { return false }
            if queued + line.count > AgentAttach.queueLimit { return true }
            queued += line.count
            writer.async { [self] in
                defer { lock.withLock { queued -= line.count } }
                guard open, final || !isRetired else { return }
                let deadline = ProcessInfo.processInfo.systemUptime + Double(AgentAttach.writeTimeout)
                let written = line.withUnsafeBytes { raw -> Bool in
                    var offset = 0
                    while offset < raw.count {
                        let n = write(fd, raw.baseAddress! + offset, raw.count - offset)
                        if n > 0 { offset += n } else if n == 0 || ![EINTR, EAGAIN].contains(errno) { return false }
                        if offset < raw.count, ProcessInfo.processInfo.systemUptime >= deadline { return false }
                    }
                    return true
                }
                if !written { retire("the agent did not read its replies within \(AgentAttach.writeTimeout) s") }
                if let request { lock.withLock { if outstanding?.id == request { outstanding = nil } } }
            }
            return false
        }
        if over { retire("the agent's unread replies passed 64 MiB") }
    }

    /// One refusal, then the end of the connection: the refusal is written
    /// before the close, and nothing queued before it is.
    func refuseAndClose(_ object: [String: Any]) {
        if let data = try? JSONSerialization.data(withJSONObject: object) { enqueue(data, final: true) }
        retire(object["error"] as? String ?? "refused", flush: true)
    }

    /// End the connection and revoke its grants: a write in progress fails and
    /// queued replies are dropped. What the kernel already took of a reply is
    /// still the driver's to read. A
    /// `flush` lets the final refusal out first, then half-closes: a socket
    /// closed at once can reach a client still connecting as a reset, with
    /// the refusal unread. Either way reading stops, which ends the reader;
    /// a connection that never started one is closed. Any thread.
    func retire(_ reason: String, flush: Bool = false) {
        let first = lock.withLock { () -> Bool in
            if retired { return false }
            retired = true
            // The descriptor is still open: only the reader's end, or the close
            // below, both after this, close it.
            flushing = flush
            if flush {
                writer.async { [self] in if open { shutdown(fd, SHUT_WR) } }
                writer.asyncAfter(deadline: .now() + 2) { [self] in stopReading() }
            } else {
                shutdown(fd, SHUT_RDWR)
                writer.async { [self] in stopReading() }
            }
            return true
        }
        guard first else { return }
        watchdog?.cancel()
        FileHandle.standardError.write(Data("exact: agent attach ended: \(reason)\n".utf8))
        DispatchQueue.main.async { AgentAttach.ended(self) }
    }
}
/// A request the attached carrier admits, built from checked fields: the host
/// forwards none of the driver's own JSON (LLP 1012.000 D3).
enum AttachRequest {
    case runner([String: Any])
    case state([String: Any])
    case layout(id: Int?, plan: Bool)
    case frames(late: Int)
    case screenshot
    case grants
    case input(Input)

    /// What input grants: a plain tap and a whole text entry at a target (a view id or a test ID), the touch
    /// runner's aim and dispatch log in the iOS simulator, and a declared tool's call.
    enum Input {
        case tap(target: Any)
        case aim(target: Any)
        case log(after: Int)
        case type(target: Any, text: String)
        /// The simulator's text entry, before its tap and before its keystrokes: the target is a shown, editable
        /// text input, and with `focused`, the one with the keyboard.
        case textTarget(target: Any, focused: Bool)
        case call(tool: String, args: String)
        /// A call's record, one this connection made: whether it has left its queue, and its answer once it has one.
        case called(id: Int)
    }
    static let ungranted = "this connection may read; input needs Allow Agent Input, which the person chooses in the app"

    struct Refusal: Error { let reply: [String: Any] }

    enum Field { case flag, count, text, target }
    /// Each read and the fields it takes.
    static let forms: [String: [String: Field]] = [
        "tree": ["target": .target, "shallow": .flag, "plan": .flag],
        "state": ["id": .count, "under": .text],
        "logs": ["since": .count, "all": .flag],
        "layout": ["id": .count, "plan": .flag],
        "perf": ["target": .count, "frames": .flag, "late": .count],
        "screenshot": [:],
        "tags": [:],
        "grants": [:],
    ]
    /// Input an attached session never sends: held gestures, keys, pickers, window and preference changes.
    static let withheld: Set<String> = ["prefer", "post", "pick", "share", "midi", "back", "close", "resize", "background"]

    static func admit(_ line: Data) -> Result<(Int, AttachRequest), Refusal> {
        func refuse(_ id: Int?, _ code: String, _ why: String) -> Result<(Int, AttachRequest), Refusal> {
            .failure(Refusal(reply: ["request": id.map { $0 as Any } ?? NSNull(), "error": "\(code): \(why)", "refused": code]))
        }
        guard let object = try? JSONSerialization.jsonObject(with: line) as? [String: Any] else { return refuse(nil, "malformed", "a request is one JSON object") }
        guard distinctKeys(line) else { return refuse(count(object["request"]), "malformed", "a field is named twice") }
        guard let id = count(object["request"]), id > 0 else { return refuse(nil, "malformed", "a request needs a positive integer request number") }
        guard let op = object["op"] as? String else { return refuse(id, "malformed", "a request names its op") }
        if object["session"] != nil { return refuse(id, "pinned", "this connection reads the window the person approved; a request names no session") }
        if op == "clock" { return refuse(id, "live-clock", "an attached app keeps its own wall clock: it is never taken, seeked, settled or landed") }
        if op == "call", object["door"] != nil || object["cold"] != nil { return refuse(id, "door", "the host chooses a call's door; a request names none") }
        if op == "prefer", let substitute = ["faults", "extension", "permission", "midi"].first(where: { object[$0] != nil }) {
            return refuse(id, "unsupported", "prefer \(substitute) substitutes for the app's world, which an attached session never does")
        }
        if withheld.contains(op) { return refuse(id, "unsupported", "\(op) is not offered to an attached session: its input is a plain tap, a text entry and a declared tool's call") }
        if ["tap", "type", "call", "called"].contains(op) { return admitInput(id, op, object).map { (id, .input($0)) } }
        guard let fields = forms[op] else { return refuse(id, "unsupported", "\(op) is not offered to an attached session") }
        if op == "state", object["reveal"] != nil { return refuse(id, "masked", "an attached session reads state masked") }
        if op == "perf", let control = ["capture", "hold", "track", "report", "health", "live", "source"].first(where: { object[$0] != nil }) {
            return refuse(id, "unsupported", "perf \(control) is not offered to an attached session")
        }
        var request: [String: Any] = ["op": op]
        for (key, value) in object where key != "request" && key != "op" {
            guard let field = fields[key] else { return refuse(id, "malformed", "\(op) takes no field \(key)") }
            switch field {
            case .flag: guard let b = flag(value) else { return refuse(id, "malformed", "\(key) is true or false") }; request[key] = b
            case .count: guard let n = count(value) else { return refuse(id, "malformed", "\(key) is a whole number up to \(UInt32.max)") }; request[key] = n
            case .text: guard let t = value as? String, t.utf8.count <= 256 else { return refuse(id, "malformed", "\(key) is a short string") }; request[key] = t
            case .target:
                if let n = count(value) { request[key] = n }
                else if let t = value as? String, !t.isEmpty, t.utf8.count <= 256 { request[key] = t }
                else { return refuse(id, "malformed", "\(key) is a view id or a short string") }
            }
        }
        switch op {
        case "state": return .success((id, .state(request)))
        case "layout": return .success((id, .layout(id: request["id"] as? Int, plan: request["plan"] as? Bool == true)))
        case "screenshot": return .success((id, .screenshot))
        case "grants": return .success((id, .grants))
        case "perf" where request["frames"] != nil:
            guard request["frames"] as? Bool == true, request["target"] == nil else { return refuse(id, "malformed", "perf frames takes late alone") }
            return .success((id, .frames(late: min(request["late"] as? Int ?? 20, 200))))
        case "perf" where request["late"] != nil: return refuse(id, "malformed", "late is perf frames'")
        default: return .success((id, .runner(request)))
        }
    }

    /// `tap`, `type` and `call`, each field checked; any other field, a key, a point, a phase or a door, refused.
    static func admitInput(_ id: Int, _ op: String, _ object: [String: Any]) -> Result<Input, Refusal> {
        func refuse(_ why: String) -> Result<Input, Refusal> {
            .failure(Refusal(reply: ["request": id, "error": "malformed: \(why)", "refused": "malformed"]))
        }
        let fields = Set(object.keys).subtracting(["request", "op"])
        let target: Any? = count(object["target"]) ?? (object["target"] as? String).flatMap { $0.isEmpty || $0.utf8.count > 256 ? nil : $0 }
        switch op {
        case "tap":
            if fields == ["log"] { return count(object["log"]).map { .success(.log(after: $0)) } ?? refuse("log is a whole number") }
            guard let target else { return refuse("tap names its target, a view id or a test ID") }
            if fields == ["target"] { return .success(.tap(target: target)) }
            // The runner's aim, `true` or a press's `{"press":true}`: a finger's landing at the target's own press.
            let press = (object["aim"] as? [String: Any]).map { $0.count == 1 && flag($0["press"]) == true } ?? (flag(object["aim"]) == true)
            guard fields == ["target", "aim"], press else { return refuse("an attached tap is `{target}`, or in the simulator `{target, aim}` or `{log}`: no point, phase, key, wheel or gesture") }
            return .success(.aim(target: target))
        case "type":
            if let target, fields == ["target", "editable"] || fields == ["target", "focused"], flag(object["editable"] ?? object["focused"]) == true {
                return .success(.textTarget(target: target, focused: fields.contains("focused")))
            }
            guard fields == ["target", "text"], let target else { return refuse("type is `{target, text}`: a whole text entry, no key or clipboard") }
            guard let text = object["text"] as? String, text.utf8.count <= 8192 else { return refuse("text is a string up to 8 KiB") }
            return .success(.type(target: target, text: text))
        case "called":
            guard fields == ["id"], let call = count(object["id"]) else { return refuse("called is `{id}`, a call's id") }
            return .success(.called(id: call))
        default:
            guard fields.isSubset(of: ["tool", "args"]), let tool = object["tool"] as? String, !tool.isEmpty, tool.utf8.count <= 256 else { return refuse("call is `{tool, args}`") }
            let args: String
            switch object["args"] {
            case nil: args = "{}"
            case let text as String: args = text
            case let value? where value is [String: Any] || value is [Any]:
                args = (try? JSONSerialization.data(withJSONObject: value)).map { String(decoding: $0, as: UTF8.self) } ?? "{}"
            default: return refuse("args is a JSON object or its text")
            }
            guard args.utf8.count <= 65536 else { return refuse("args is at most 64 KiB") }
            return .success(.call(tool: tool, args: args))
        }
    }

    /// The one view `target` names in `session` and shows in `window`: its id, or the one such view with that test
    /// ID. A view hidden itself or under a hidden ancestor is not shown. Main thread.
    static func view(_ target: Any, in session: ExactSession, window: AnyObject?) -> Result<NodeView, Refusal> {
        func refuse(_ why: String) -> Result<NodeView, Refusal> { .failure(Refusal(reply: ["error": why, "refused": "target"])) }
        func shown(_ v: NodeView) -> Bool {
            guard let window, v.window === window else { return false }
            var at: PlatformView? = v
            while let u = at { if u.isHidden { return false }; at = u.superview }
            return true
        }
        if let id = target as? Int {
            guard let v = session.presenter.views[UInt32(id)], shown(v) else { return refuse("no view #\(id) is shown in the attached window") }
            return .success(v)
        }
        let name = target as? String ?? ""
        let found = session.presenter.views.values.filter { $0.props["testId"] == name && shown($0) }
        guard found.count == 1 else {
            return refuse(found.isEmpty ? "no shown view has test ID \(name)" : "\(found.count) shown views have test ID \(name): name one by its id")
        }
        return .success(found[0])
    }

    /// A whole number, not a Boolean, which JSONSerialization also makes an NSNumber, within a view id's range: a
    /// host converts an id to `UInt32`, and a larger one would end the app there.
    static func count(_ value: Any?) -> Int? {
        guard let n = value as? NSNumber, CFGetTypeID(n) != CFBooleanGetTypeID(), CFNumberIsFloatType(n) == false,
              n.int64Value >= 0, n.int64Value <= Int64(UInt32.max), n.stringValue == String(n.int64Value) else { return nil }
        return n.intValue
    }
    static func flag(_ value: Any?) -> Bool? {
        guard let n = value as? NSNumber, CFGetTypeID(n) == CFBooleanGetTypeID() else { return nil }
        return n.boolValue
    }

    /// Whether the object's top-level keys are distinct, compared as decoded (`"a"` and `"\u0061"` are one key):
    /// JSONSerialization keeps one of a repeated key silently. `line` is already known to be one JSON object.
    static func distinctKeys(_ line: Data) -> Bool {
        let bytes = [UInt8](line)
        var keys = Set<String>(), depth = 0, keyNext = false, i = 0
        while i < bytes.count {
            switch bytes[i] {
            case 0x22: // a string, read to its closing quote; at depth 1 after `{` or `,`, a key
                var j = i + 1
                while j < bytes.count, bytes[j] != 0x22 { j += bytes[j] == 0x5c ? 2 : 1 }
                if depth == 1, keyNext {
                    let quoted = Data(bytes[i..<min(j + 1, bytes.count)])
                    guard let key = try? JSONSerialization.jsonObject(with: quoted, options: .fragmentsAllowed) as? String,
                          keys.insert(key).inserted else { return false }
                }
                keyNext = false
                i = j
            case 0x7b, 0x5b: depth += 1; keyNext = depth == 1
            case 0x7d, 0x5d: depth -= 1
            case 0x2c where depth == 1: keyNext = true
            default: break
            }
            i += 1
        }
        return true
    }

    static func raw(_ id: Int, _ object: [String: Any]) -> String {
        var with = object
        with["request"] = id
        return (try? JSONSerialization.data(withJSONObject: with)).map { String(decoding: $0, as: UTF8.self) } ?? "{\"request\":\(id),\"error\":\"unwritable reply\"}"
    }

    /// The runner's own reply with the request's number first.
    static func splice(_ id: Int, _ reply: String) -> String {
        guard reply.hasPrefix("{"), reply.hasSuffix("}") else { return raw(id, ["error": "an unreadable reply"]) }
        let rest = reply.dropFirst()
        return "{\"request\":\(id)" + (rest.hasPrefix("}") ? "" : ",") + rest
    }
}

/// The passive reads (LLP 1012.000 D3): the runner's memory and the host's
/// committed views and pixels, with no draining, settling, guest script or
/// change to the window. Main thread.
enum AttachRead {
    static func answer(_ id: Int, _ request: AttachRequest, session: ExactSession, surface: AttachSurface) -> String {
        func ask(_ object: [String: Any]) -> String {
            let json = (try? JSONSerialization.data(withJSONObject: object)).map { String(decoding: $0, as: UTF8.self) } ?? "{}"
            return session.agent(json)
        }
        switch request {
        case let .runner(object): return AttachRequest.splice(id, ask(object))
        case let .state(object):
            // The runner masks every value a live password field holds (`reveal` is never sent).
            var reply = ask(object)
            let observation = session.executionObservation().merging(["attached": true, "wall": ExactEnv.wall()]) { _, mine in mine }
            if reply.hasSuffix("}"), !reply.hasPrefix("{\"error\""), let tail = try? JSONSerialization.data(withJSONObject: ["observation": observation]) {
                reply.removeLast()
                reply += "," + String(decoding: tail, as: UTF8.self).dropFirst()
            }
            return AttachRequest.splice(id, reply)
        case let .layout(node, plan):
            // Not `session.agentInstance`, which keeps its session alive past the window's close.
            let agent = Agent(session: session)
            return AttachRequest.raw(id, node.map { agent.layout(["op": "layout", "id": $0, "plan": plan]) } ?? agent.layout())
        case let .frames(late):
            var frames = session.sampler?.reply(late: late, all: false) ?? ["unavailable": true]
            frames["health"] = session.frameHealth.reply(virtual: false)
            return AttachRequest.raw(id, frames)
        case .screenshot: return AttachRequest.raw(id, surface.picture())
        // The connection's own: its dispatch answers these before a read.
        case .grants, .input: return AttachRequest.raw(id, ["error": "not a read"])
        }
    }
}
#endif
