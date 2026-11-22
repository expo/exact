// What a lift carries (LLP 1094.000 D3, D4): the node's admitted
// `dataTransferFiles`, `dataTransferUrl` and `dataTransferText`, read when
// the lift begins. No file is read or opened then: each file's load handler
// reads its own when a destination asks.
#if os(iOS)
import ExactCore
import ExactKit
import UIKit
import UniformTypeIdentifiers

enum DragItems {
    /// The node an item was lifted from, which a cancelled drag returns to
    /// while the view is still that node's: the same id and incarnation, since
    /// the node pool lends a view to the next node.
    final class Lifted {
        let id: UInt32, incarnation: UInt64
        weak var view: NodeView?
        init(_ view: NodeView) { id = view.id; incarnation = view.incarnation; self.view = view }
    }

    /// What `node` carries, named as a person would know it: its text, else
    /// its first file's name, else its URL; nil when it carries nothing, as
    /// `items` reads it, which UIKit lifts nothing without.
    static func name(of node: NodeView) -> String? {
        if let text = node.props["dataTransferText"], !text.isEmpty { return text }
        if let first = (node.props["dataTransferFiles"] ?? "").split(separator: "\n").first { return (String(first) as NSString).lastPathComponent }
        return url(of: node)?.absoluteString
    }

    /// One item per file, its type read from its extension and its name the
    /// file's own; `url`, then `text`, on the first item after its file, since
    /// a provider lists its higher-fidelity types first (Document:
    /// `NSItemProvider.h`), or on one item of their own when there is no file.
    /// An empty `url` or `text` is none, as `tree` reads it, and so is a
    /// `url` Foundation cannot read (`URL(string:)`), which the sources
    /// journal as it arrives.
    /// Each `app:/` file is opened when a destination asks, through `open`.
    static func items(of node: NodeView, in session: ExactSession?, open: @escaping DragFiles.Opener = DragFiles.open) -> [UIDragItem] {
        let files = (node.props["dataTransferFiles"] ?? "").split(separator: "\n").map(String.init)
        let url = url(of: node)
        let text = node.props["dataTransferText"].flatMap { $0.isEmpty ? nil : $0 }
        let journal = DragFiles.Journal(session: session)
        let scratch = Scratch()
        var providers = files.map { entry in
            let provider = NSItemProvider()
            let name = (entry as NSString).lastPathComponent
            provider.suggestedName = name
            let type = UTType(filenameExtension: (name as NSString).pathExtension) ?? .data
            provider.registerFileRepresentation(forTypeIdentifier: type.identifier, fileOptions: [], visibility: .all,
                                                loadHandler: entry.hasPrefix("app:/") ? appFile(entry, scratch, journal, open: open) : shipped(entry, session?.app.resolver, journal))
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

    private static func url(of node: NodeView) -> URL? { node.props["dataTransferUrl"].flatMap { $0.isEmpty ? nil : URL(string: $0) } }

    typealias Load = (@escaping (URL?, Bool, Error?) -> Void) -> Progress?

    /// A shipped file, through the generation's resolver as found at the
    /// lift, which the handler holds: a delivered update's resolver removes
    /// its files when it is released (`AssetResolver.deinit`). The system
    /// copies the file before the handler's completion returns (Document:
    /// `NSItemProvider.h`), so its URL needs no copy of Exact's.
    static func shipped(_ entry: String, _ resolver: AssetResolver?, _ journal: DragFiles.Journal) -> Load {
        { completion in
            if let url = DragFiles.shipped(entry, in: resolver) {
                completion(url, false, nil)
            } else {
                completion(nil, false, journal.failed(entry, "the app does not ship it"))
            }
            return nil
        }
    }

    /// An `app:/` file, opened when a destination asks (`open`) and placed
    /// on the drags' copy queue (`DragFiles.copies`) in a folder of its own
    /// in `scratch` (`DragFiles.place`), the descriptor closed before the
    /// file is answered. A copy that fails takes its folder with it.
    static func appFile(_ entry: String, _ scratch: Scratch, _ journal: DragFiles.Journal, open: @escaping DragFiles.Opener = DragFiles.open) -> Load {
        { completion in
            DragFiles.copies.addOperation {
                let folder = scratch.directory.appendingPathComponent(UUID().uuidString, isDirectory: true)
                let url = folder.appendingPathComponent((entry as NSString).lastPathComponent)
                do {
                    let fd = try open(entry, journal).get()
                    let placed = Result {
                        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
                        try DragFiles.place(fd, at: url)
                    }
                    Darwin.close(fd)
                    try placed.get()
                    completion(url, false, nil)
                } catch {
                    try? FileManager.default.removeItem(at: folder)
                    completion(nil, false, journal.failed(entry, error.localizedDescription))
                }
            }
            return nil
        }
    }

    /// The directory a lift's `app:/` files are placed in for the system to
    /// copy, removed once every provider that may load into it is released.
    final class Scratch {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent("exact-drag-\(UUID().uuidString)", isDirectory: true)
        deinit { try? FileManager.default.removeItem(at: directory) }
    }
}
#endif
