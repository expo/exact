// What a lift carries (LLP 1094.000 D3, D4): the node's admitted
// `dataTransferFiles`, `dataTransferUrl` and `dataTransferText`, read when
// the lift begins. No bytes are read then: each file's load handler reads its
// own when a destination asks, through what it holds until the provider is
// released, however the drag ends.
#if os(iOS)
import ExactCore
import ExactKit
import UIKit
import UniformTypeIdentifiers

enum DragItems {
    /// The node an item was lifted from, which a cancelled drag returns to
    /// while the view is still that node's.
    final class Lifted {
        let id: UInt32
        weak var view: NodeView?
        init(_ view: NodeView) { id = view.id; self.view = view }
    }

    /// One item per file, its type read from its extension and its name the
    /// file's own; `url`, then `text`, on the first item after its file, since
    /// a provider lists its higher-fidelity types first (Document:
    /// `NSItemProvider.h`), or on one item of their own when there is no file.
    /// An empty `url` or `text` is none, as `tree` reads it.
    static func items(of node: NodeView, in session: ExactSession?) -> [UIDragItem] {
        let files = (node.props["dataTransferFiles"] ?? "").split(separator: "\n").map(String.init)
        let url = node.props["dataTransferUrl"].flatMap { $0.isEmpty ? nil : URL(string: $0) }
        let text = node.props["dataTransferText"].flatMap { $0.isEmpty ? nil : $0 }
        var providers = files.map { entry in
            let provider = NSItemProvider()
            let name = (entry as NSString).lastPathComponent
            provider.suggestedName = name
            let type = UTType(filenameExtension: (name as NSString).pathExtension) ?? .data
            provider.registerFileRepresentation(forTypeIdentifier: type.identifier, fileOptions: [], visibility: .all,
                                                loadHandler: entry.hasPrefix("app:/") ? appFile(entry, session) : shipped(entry, session?.app.resolver, session))
            return provider
        }
        if providers.isEmpty, url != nil || text != nil { providers = [NSItemProvider()] }
        if let first = providers.first {
            if let url { first.registerObject(url as NSURL, visibility: .all) }
            if let text { first.registerObject(text as NSString, visibility: .all) }
        }
        let lifted = Lifted(node)
        return providers.map { provider in
            let item = UIDragItem(itemProvider: provider)
            item.localObject = lifted
            return item
        }
    }

    typealias Load = (@escaping (URL?, Bool, Error?) -> Void) -> Progress?

    /// A shipped file, through the generation's resolver as found at the
    /// lift, which the handler holds: a delivered update's resolver removes
    /// its files when it is released (`AssetResolver.deinit`). The system
    /// copies the file before the handler's completion returns (Document:
    /// `NSItemProvider.h`), so its URL needs no copy of Exact's.
    static func shipped(_ entry: String, _ resolver: AssetResolver?, _ session: ExactSession?) -> Load {
        { [weak session] completion in
            if let url = DragFiles.shipped(entry, in: resolver) {
                completion(url, false, nil)
            } else {
                // The session is held on the main thread only.
                DispatchQueue.main.async { [weak session] in fail(entry, "the app does not ship it", session, completion) }
            }
            return nil
        }
    }

    /// An `app:/` file, through `share`'s confined copy (`exact_share_file`:
    /// the grants, no link followed, at most 64 MiB) into a directory the
    /// handler owns, made on the main thread, where the runtime is asked, at
    /// the first load and removed when the provider is released. The system's
    /// own copy of it is made off the main thread.
    static func appFile(_ entry: String, _ session: ExactSession?) -> Load {
        let copy = Copy(entry)
        return { [weak session] completion in
            DispatchQueue.main.async { [weak session] in
                guard let session else { return completion(nil, false, CocoaError(.fileReadUnknown)) }
                do {
                    let url = try copy.file(session)
                    // The copy's directory stays until the system has made its own.
                    DispatchQueue.global(qos: .userInitiated).async { withExtendedLifetime(copy) { completion(url, false, nil) } }
                } catch {
                    fail(entry, error.localizedDescription, session, completion)
                }
            }
            return nil
        }
    }

    /// The destination gets nothing for the item, and the journal says why.
    /// On the main thread.
    static func fail(_ entry: String, _ reason: String, _ session: ExactSession?, _ completion: (URL?, Bool, Error?) -> Void) {
        completion(nil, false, DragFiles.failed(entry, reason, session))
    }

    final class Copy {
        let entry: String
        private var directory: URL?
        private var made: URL?
        init(_ entry: String) { self.entry = entry }
        deinit { if let directory { try? FileManager.default.removeItem(at: directory) } }

        func file(_ session: ExactSession) throws -> URL {
            if let made { return made }
            let directory = directory ?? FileManager.default.temporaryDirectory.appendingPathComponent("exact-drag-\(UUID().uuidString)", isDirectory: true)
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            self.directory = directory
            let url = directory.appendingPathComponent((entry as NSString).lastPathComponent)
            try DragFiles.copy(entry, to: url, session)
            made = url
            return url
        }
    }
}
#endif
