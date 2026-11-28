// How a drag's file is read when a destination asks (LLP 1094.000 D3), on iOS
// and the Mac: a shipped file through the resolver found at the lift, an
// `app:/` file through a descriptor the session's runtime opens for it then,
// through the app's pinned roots under its grants (`exact_share_file`'s
// `shareOpen`, LLP 1125 D12). Nothing is read at the lift. A read that fails
// is journaled while the session lives, and the destination is told it
// failed.
#if os(iOS) || os(macOS)
import CExact
import Darwin
import ExactCore
import ExactKit
import Foundation

enum DragFiles {
    /// Each of `node`'s admitted files with the name a destination is offered
    /// for it: the one `dataTransferNames` holds at its place, else the
    /// file's own (LLP 1094.000 D14).
    static func files(of node: NodeView) -> [(entry: String, name: String)] {
        let names = (node.props["dataTransferNames"] ?? "").split(separator: "\n", omittingEmptySubsequences: false)
        return (node.props["dataTransferFiles"] ?? "").split(separator: "\n").enumerated().map { at, entry in
            let name = at < names.count && !names[at].isEmpty ? String(names[at]) : (String(entry) as NSString).lastPathComponent
            return (String(entry), name)
        }
    }

    /// A shipped file's URL in `resolver`'s generation, or nil when the app
    /// does not ship it: an admitted entry is not read until the drop.
    static func shipped(_ entry: String, in resolver: AssetResolver?) -> URL? {
        guard let url = resolver?.url(entry), FileManager.default.isReadableFile(atPath: url.path) else { return nil }
        return url
    }

    /// The queue every drag's file copies run on, a few at once, each opening
    /// its file as it starts: a request for many files holds a few
    /// descriptors at a time, not one a file.
    static let copies: OperationQueue = {
        let queue = OperationQueue()
        queue.name = "exact.drag-out.copies"
        queue.qualityOfService = .userInitiated
        queue.maxConcurrentOperationCount = 4
        return queue
    }()

    /// How a copy opens an `app:/` file: `open`; a test's stand-in.
    typealias Opener = (String, Journal) -> Result<Int32, Error>

    /// `entry` as the session's runtime opens it for a destination: the
    /// descriptor, which the caller then owns and closes, or why the file
    /// could not be opened. Asked on the main thread, the only one that reads
    /// the session, which a copy waits on.
    static func open(_ entry: String, _ journal: Journal) -> Result<Int32, Error> {
        let ask = { () -> Result<Int32, Error> in
            guard let session = journal.live else { return .failure(failure("the app's session has ended")) }
            let request = (try? JSONSerialization.data(withJSONObject: ["op": "shareOpen", "path": entry])).map { String(decoding: $0, as: UTF8.self) } ?? "{}"
            let reply = (try? JSONSerialization.jsonObject(with: Data(session.runtime.ask(exact_share_file, request).utf8))) as? [String: Any]
            guard let fd = (reply?["fd"] as? NSNumber)?.int32Value else {
                // The runtime's error names the path, which the journal line names already
                let error = reply?["error"] as? String ?? "it could not be opened"
                return .failure(failure(error.hasPrefix("\(entry): ") ? String(error.dropFirst(entry.count + 2)) : error))
            }
            return .success(fd)
        }
        return Thread.isMainThread ? ask() : DispatchQueue.main.sync(execute: ask)
    }

    /// The bytes behind `fd` in a new file at `url`: made there exclusively,
    /// so a file or a link already there refuses it (`open(2)`'s `O_EXCL`),
    /// with the mode the umask gives a new file, and the data alone copied
    /// into it (`copyfile(3)`'s `COPYFILE_DATA`), none of the source's mode
    /// or extended attributes. A copy that fails leaves what it wrote, and
    /// the destination is told it failed: a name in a folder others write to
    /// can be another's by the time it is removed. `progress`, when given,
    /// takes the file's size as its total and counts the bytes written; its
    /// cancel, read before the copy, at each block and after the last, stops
    /// it with `ECANCELED` (LLP 1094.000 D17).
    static func place(_ fd: Int32, at url: URL, progress: Progress? = nil) throws {
        // Off the main thread: a file has no size bound
        dispatchPrecondition(condition: .notOnQueue(.main))
        if progress?.isCancelled == true { throw POSIXError(.ECANCELED) }
        let out = Darwin.open(url.path, O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC, 0o666)
        guard out >= 0 else { throw posix() }
        let state = copyfile_state_alloc()
        defer { copyfile_state_free(state) }
        let counting = progress.map(Counting.init)
        if let counting {
            var info = stat()
            if fstat(fd, &info) == 0 { counting.progress.totalUnitCount = Int64(info.st_size) }
            // Unretained: `withExtendedLifetime` below keeps the counter alive through the copy
            copyfile_state_set(state, UInt32(COPYFILE_STATE_STATUS_CTX), Unmanaged.passUnretained(counting).toOpaque())
            let counted: copyfile_callback_t = { what, stage, state, _, _, context in
                guard let context else { return COPYFILE_CONTINUE }
                let counting = Unmanaged<Counting>.fromOpaque(context).takeUnretainedValue()
                // A write's error ends the copy with it, which `copyfile` would otherwise retry
                if stage == COPYFILE_ERR { counting.failed = errno; return COPYFILE_QUIT }
                if what == COPYFILE_COPY_DATA, stage == COPYFILE_PROGRESS {
                    var copied: off_t = 0
                    if copyfile_state_get(state, UInt32(COPYFILE_STATE_COPIED), &copied) == 0 { counting.progress.completedUnitCount = Int64(copied) }
                }
                return counting.progress.isCancelled ? COPYFILE_QUIT : COPYFILE_CONTINUE
            }
            copyfile_state_set(state, UInt32(COPYFILE_STATE_STATUS_CB), unsafeBitCast(counted, to: UnsafeRawPointer.self))
        }
        let copied = withExtendedLifetime(counting) { fcopyfile(fd, out, state, copyfile_flags_t(COPYFILE_DATA)) }
        guard copied == 0 else {
            // `COPYFILE_QUIT` sets `ECANCELED`: a write's own error is the one the callback saw
            let error = counting.map { $0.failed } .flatMap { $0 == 0 ? nil : POSIXError(POSIXErrorCode(rawValue: $0) ?? .EIO) } ?? posix()
            Darwin.close(out)
            throw error
        }
        // The bytes written, the file's size now, which may have changed since it was measured (D3)
        var written = stat()
        let wrote = fstat(out, &written) == 0 ? Int64(written.st_size) : nil
        // A write the system had not yet committed fails at the close (`close(2)`)
        guard Darwin.close(out) == 0 else { throw posix() }
        if let progress {
            // An empty file's copy calls back for no block, and a cancel may come as the last one ends
            guard !progress.isCancelled else { throw POSIXError(.ECANCELED) }
            if let wrote { progress.totalUnitCount = wrote; progress.completedUnitCount = wrote }
        }
    }

    /// The session a drag opens its `app:/` files through and journals to,
    /// read on the main thread only: a thread that loaded it could make the
    /// last release, which ends the session off the main thread.
    struct Journal {
        weak var session: ExactSession?

        /// The session, until it is destroyed: one an embedder still holds
        /// after `destroy()` has no runtime to ask. On the main thread only.
        var live: ExactSession? {
            dispatchPrecondition(condition: .onQueue(.main))
            return session.flatMap { $0.state == .destroyed ? nil : $0 }
        }

        /// `drag: <entry> could not be read: <reason>`, journaled on the main
        /// thread while the session lives, and the error the destination
        /// gets.
        func failed(_ entry: String, _ reason: String) -> Error {
            let line = "drag: \(entry) could not be read: \(reason)"
            if Thread.isMainThread { live?.log(line) } else { DispatchQueue.main.async { [self] in live?.log(line) } }
            return failure(line)
        }
    }

    static func failure(_ description: String) -> Error {
        NSError(domain: NSCocoaErrorDomain, code: CocoaError.fileReadUnknown.rawValue, userInfo: [NSLocalizedDescriptionKey: description])
    }

    static func posix() -> Error { POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO) }

    /// A copy's progress, and the error of a write that ended it.
    private final class Counting {
        let progress: Progress
        var failed: Int32 = 0
        init(_ progress: Progress) { self.progress = progress }
    }

    /// What a destination that cancelled a file's copy is answered (LLP
    /// 1094.000 D17), journaled as its cancel.
    static func cancelled(_ entry: String, _ journal: Journal) -> Error {
        _ = journal.failed(entry, "the destination cancelled its copy")
        return CocoaError(.userCancelled)
    }

    /// Whether `error` is a copy's cancel.
    static func isCancel(_ error: Error) -> Bool { (error as? POSIXError)?.code == .ECANCELED }
}
#endif
