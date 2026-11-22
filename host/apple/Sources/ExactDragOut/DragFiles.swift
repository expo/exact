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
    /// can be another's by the time it is removed.
    static func place(_ fd: Int32, at url: URL) throws {
        // Off the main thread: a file has no size bound
        dispatchPrecondition(condition: .notOnQueue(.main))
        let out = Darwin.open(url.path, O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC, 0o666)
        guard out >= 0 else { throw posix() }
        guard fcopyfile(fd, out, nil, copyfile_flags_t(COPYFILE_DATA)) == 0 else {
            let error = posix()
            Darwin.close(out)
            throw error
        }
        // A write the system had not yet committed fails at the close (`close(2)`)
        guard Darwin.close(out) == 0 else { throw posix() }
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
}
#endif
