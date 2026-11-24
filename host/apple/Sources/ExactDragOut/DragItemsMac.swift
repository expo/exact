// What a Mac drag carries (LLP 1094.000 D5): the node's admitted
// `dataTransferFiles`, `dataTransferUrl` and `dataTransferText`, read when the
// drag begins. Each file is a promise (`NSFilePromiseProvider`), written
// where a destination asks, and `url` and `text` are each a pasteboard item
// of its own after them, since a destination usually reads one of an item's types;
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
        let files = (node.props["dataTransferFiles"] ?? "").split(separator: "\n").map(String.init)
        let url = node.props["dataTransferUrl"].flatMap { $0.isEmpty ? nil : $0 }
        let text = node.props["dataTransferText"].flatMap { $0.isEmpty ? nil : $0 }
        let journal = DragFiles.Journal(session: session)
        var writers: [NSPasteboardWriting] = files.map { entry in
            let name = (entry as NSString).lastPathComponent
            let type = UTType(filenameExtension: (name as NSString).pathExtension) ?? .data
            let writer = PromiseWriter(entry: entry, name: name, resolver: session?.app.resolver, journal: journal)
            writer.open = open
            let promise = NSFilePromiseProvider(fileType: type.identifier, delegate: writer)
            // The provider's delegate is weak: its `userInfo` holds the writer
            // as long as AppKit holds the provider.
            promise.userInfo = writer
            return promise
        }
        let words = [(url, NSPasteboard.PasteboardType.URL), (text, .string)].compactMap { word, type in word.map { ($0, type) } }
        if writers.isEmpty, !words.isEmpty {
            let item = NSPasteboardItem()
            words.forEach { item.setString($0.0, forType: $0.1) }
            writers = [item]
        } else {
            writers += words.map { word, type in
                let item = NSPasteboardItem()
                item.setString(word, forType: type)
                return item
            }
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

        init(entry: String, name: String, resolver: AssetResolver?, journal: DragFiles.Journal) {
            self.entry = entry; self.name = name; self.resolver = resolver; self.journal = journal
        }

        func filePromiseProvider(_ filePromiseProvider: NSFilePromiseProvider, fileNameForType fileType: String) -> String { name }

        func operationQueue(for filePromiseProvider: NSFilePromiseProvider) -> OperationQueue { DragFiles.copies }

        func filePromiseProvider(_ filePromiseProvider: NSFilePromiseProvider, writePromiseTo url: URL, completionHandler: @escaping (Error?) -> Void) {
            do {
                try write(to: url)
                completionHandler(nil)
            } catch {
                completionHandler(journal.failed(entry, error.localizedDescription))
            }
        }

        /// The file at `url`, its source descriptor closed before this returns.
        private func write(to url: URL) throws {
            let fd: Int32
            if entry.hasPrefix("app:/") {
                fd = try open(entry, journal).get()
            } else {
                guard let file = DragFiles.shipped(entry, in: resolver) else { throw DragFiles.failure("the app does not ship it") }
                fd = Darwin.open(file.path, O_RDONLY | O_CLOEXEC)
                guard fd >= 0 else { throw DragFiles.posix() }
            }
            defer { Darwin.close(fd) }
            try DragFiles.place(fd, at: url)
        }
    }
}
#endif
