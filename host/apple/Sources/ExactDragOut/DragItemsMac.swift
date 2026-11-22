// What a Mac drag carries (LLP 1094.000 D5): the node's admitted
// `dataTransferFiles`, `dataTransferUrl` and `dataTransferText`, read when the
// drag begins. Each file is a promise (`NSFilePromiseProvider`), written
// where a destination asks; the first also offers `url` and then `text`. With
// no file one pasteboard item carries them. Each item's image is the node as
// drawn when AppKit asks for it.
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
        var writers: [NSPasteboardWriting] = files.enumerated().map { index, entry in
            let name = (entry as NSString).lastPathComponent
            let type = UTType(filenameExtension: (name as NSString).pathExtension) ?? .data
            let writer = PromiseWriter(entry: entry, name: name, resolver: session?.app.resolver, journal: journal)
            writer.open = open
            let promise = Promise(fileType: type.identifier, delegate: writer)
            // The provider's delegate is weak: its `userInfo` holds the writer
            // as long as AppKit holds the provider.
            promise.userInfo = writer
            if index == 0 { promise.alternatives = (url, text) }
            return promise
        }
        if writers.isEmpty, url != nil || text != nil {
            let item = NSPasteboardItem()
            if let url { item.setString(url, forType: .URL) }
            if let text { item.setString(text, forType: .string) }
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

    /// A file promise that also offers the drag's `url` and `text`, after the
    /// promise's own types.
    final class Promise: NSFilePromiseProvider {
        var alternatives: (url: String?, text: String?) = (nil, nil)

        override func writableTypes(for pasteboard: NSPasteboard) -> [NSPasteboard.PasteboardType] {
            super.writableTypes(for: pasteboard) + (alternatives.url != nil ? [.URL] : []) + (alternatives.text != nil ? [.string] : [])
        }

        override func pasteboardPropertyList(forType type: NSPasteboard.PasteboardType) -> Any? {
            switch type {
            case .URL where alternatives.url != nil: return alternatives.url
            case .string where alternatives.text != nil: return alternatives.text
            default: return super.pasteboardPropertyList(forType: type)
            }
        }
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
