// What a Mac drag carries (LLP 1094.000 D5, D14): the node's admitted
// `dataTransferFiles` with their `dataTransferNames`, `dataTransferUrl` and
// `dataTransferText`, read when the drag begins. Each file is a promise
// (`NSFilePromiseProvider`), written where a destination asks under the name
// it is offered under, `url` and `text` further types of the first file's
// item after its promise, and `NodeItem.type` last on each file's item;
// with no file one pasteboard item carries both. Each item's image is the
// node as drawn when AppKit asks for it.
#if os(macOS)
import AppKit
import ExactCore
import ExactKit
import UniformTypeIdentifiers

enum DragItemsMac {
    /// Each `app:/` file is opened when a destination asks, through `open`.
    static func items(of node: NodeView, in session: ExactSession?, open: @escaping DragFiles.Opener = DragFiles.open) -> [NSDraggingItem] {
        let files = DragFiles.files(of: node)
        let url = node.props["dataTransferUrl"].flatMap { $0.isEmpty ? nil : $0 }
        let text = node.props["dataTransferText"].flatMap { $0.isEmpty ? nil : $0 }
        let journal = DragFiles.Journal(session: session)
        var writers: [NSPasteboardWriting] = files.map { entry, name in
            let type = UTType(filenameExtension: (entry as NSString).pathExtension) ?? .data
            let writer = PromiseWriter(entry: entry, name: name, resolver: session?.app.resolver, journal: journal)
            writer.open = open
            let promise = FormsPromiseProvider(fileType: type.identifier, delegate: writer)
            // The provider's delegate is weak: its `userInfo` holds the writer
            // as long as AppKit holds the provider.
            promise.userInfo = writer
            return promise
        }
        let words = [(url, NSPasteboard.PasteboardType.URL), (text, .string)].compactMap { word, type in word.map { ($0, type) } }
        if let first = writers.first as? FormsPromiseProvider {
            first.forms = words.map { ($0.1, $0.0) }
        }
        writers.forEach { ($0 as? FormsPromiseProvider)?.forms.append((NSPasteboard.PasteboardType(NodeItem.type), Data())) }
        if writers.isEmpty, !words.isEmpty {
            let item = NSPasteboardItem()
            words.forEach { item.setString($0.0, forType: $0.1) }
            writers = [item]
        }
        return writers.map { writer in
            let item = NSDraggingItem(pasteboardWriter: writer)
            item.draggingFrame = node.bounds
            // Draw each item's image only when AppKit asks for it, which its header
            // promises only "if necessary": making the items draws nothing
            item.imageComponentsProvider = { [weak node] in
                guard let node, let image = picture(of: node) else { return [] }
                let component = NSDraggingImageComponent(key: .icon)
                component.contents = image
                component.frame = NSRect(origin: .zero, size: node.bounds.size)
                return [component]
            }
            return item
        }
    }

    /// The node as drawn, the drag's image, as `share`'s picture of a node
    /// draws it.
    static func picture(of view: NSView) -> NSImage? {
        guard view.bounds.width > 0, view.bounds.height > 0, let rep = view.bitmapImageRepForCachingDisplay(in: view.bounds) else { return nil }
        view.cacheDisplay(in: view.bounds, to: rep)
        let image = NSImage(size: view.bounds.size)
        image.addRepresentation(rep)
        return image
    }

    /// Writes one promised file where a destination asks (`DragFiles.place`),
    /// on the drags' copy queue (`DragFiles.copies`) rather than the main
    /// queue, the provider's default, since a file has no size bound: a
    /// shipped one from the resolver found when the drag began (a delivered
    /// update's removes its files when released), an `app:/` one opened when
    /// the destination asks for it (`open`).
    final class PromiseWriter: NSObject, NSFilePromiseProviderDelegate {
        let entry: String, name: String
        let resolver: AssetResolver?
        let journal: DragFiles.Journal
        var open: DragFiles.Opener = DragFiles.open
        /// Each write's progress once it is published; a test's stand-in.
        var published: (Progress) -> Void = { _ in }

        init(entry: String, name: String, resolver: AssetResolver?, journal: DragFiles.Journal) {
            self.entry = entry; self.name = name; self.resolver = resolver; self.journal = journal
        }

        func filePromiseProvider(_ filePromiseProvider: NSFilePromiseProvider, fileNameForType fileType: String) -> String { name }

        func operationQueue(for filePromiseProvider: NSFilePromiseProvider) -> OperationQueue { DragFiles.copies }

        func filePromiseProvider(_ filePromiseProvider: NSFilePromiseProvider, writePromiseTo url: URL, completionHandler: @escaping (Error?) -> Void) {
            do {
                try write(to: url)
                completionHandler(nil)
            } catch where DragFiles.isCancel(error) {
                completionHandler(DragFiles.cancelled(entry, journal))
            } catch {
                completionHandler(journal.failed(entry, error.localizedDescription))
            }
        }

        /// The file at `url`, its source descriptor closed before this returns,
        /// its copy published as a file progress for `url` (Foundation's
        /// `publish()`), from before the source is opened until the write
        /// ends; a subscriber's cancel stops it (LLP 1094.000 D17).
        func write(to url: URL) throws {
            let progress = Progress(totalUnitCount: -1)
            progress.kind = .file
            progress.fileOperationKind = .copying
            progress.fileURL = url
            progress.publish()
            defer { progress.unpublish() }
            published(progress)
            if progress.isCancelled { throw POSIXError(.ECANCELED) }
            let fd: Int32
            if entry.hasPrefix("app:/") {
                fd = try open(entry, journal).get()
            } else {
                guard let file = DragFiles.shipped(entry, in: resolver) else { throw DragFiles.failure("the app does not ship it") }
                fd = Darwin.open(file.path, O_RDONLY | O_CLOEXEC)
                guard fd >= 0 else { throw DragFiles.posix() }
            }
            defer { Darwin.close(fd) }
            try DragFiles.place(fd, at: url, progress: progress)
        }
    }
}

/// A file's promise that also writes further forms of its item, the node's
/// link and text and `NodeItem.type`, after the promise's own types, as
/// `PromiseFileProvider.swift` in Apple's table view file promise sample adds
/// its types (LLP 1094.000 D5).
final class FormsPromiseProvider: NSFilePromiseProvider {
    var forms: [(NSPasteboard.PasteboardType, Any)] = []

    override func writableTypes(for pasteboard: NSPasteboard) -> [NSPasteboard.PasteboardType] {
        super.writableTypes(for: pasteboard) + forms.map(\.0)
    }

    override func pasteboardPropertyList(forType type: NSPasteboard.PasteboardType) -> Any? {
        forms.first { $0.0 == type }?.1 ?? super.pasteboardPropertyList(forType: type)
    }
}
#endif
