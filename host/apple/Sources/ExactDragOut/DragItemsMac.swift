// What a Mac drag carries (LLP 1094.000 D5): the node's admitted
// `dataTransferFiles`, `dataTransferUrl` and `dataTransferText`, read when the
// drag begins. Each file is a promise (`NSFilePromiseProvider`), written
// where a destination asks; the first also offers `url` and then `text`. With
// no file one pasteboard item carries them. Each item's image is the node as
// drawn.
#if os(macOS)
import AppKit
import ExactCore
import ExactKit
import UniformTypeIdentifiers

enum DragItemsMac {
    static func items(of node: NodeView, in session: ExactSession?) -> [NSDraggingItem] {
        let files = (node.props["dataTransferFiles"] ?? "").split(separator: "\n").map(String.init)
        let url = node.props["dataTransferUrl"].flatMap { $0.isEmpty ? nil : $0 }
        let text = node.props["dataTransferText"].flatMap { $0.isEmpty ? nil : $0 }
        var writers: [NSPasteboardWriting] = files.enumerated().map { index, entry in
            let name = (entry as NSString).lastPathComponent
            let type = UTType(filenameExtension: (name as NSString).pathExtension) ?? .data
            let writer = PromiseWriter(entry: entry, name: name, resolver: session?.app.resolver, session: session)
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
        let image = picture(of: node)
        return writers.map { writer in
            let item = NSDraggingItem(pasteboardWriter: writer)
            item.setDraggingFrame(node.bounds, contents: image)
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

    /// Writes one promised file where a destination asks, from the resolver
    /// found when the drag began (a delivered update's removes its files when
    /// released) or through `share`'s confined copy, on the main queue, the
    /// provider's default.
    final class PromiseWriter: NSObject, NSFilePromiseProviderDelegate {
        let entry: String, name: String
        let resolver: AssetResolver?
        weak var session: ExactSession?

        init(entry: String, name: String, resolver: AssetResolver?, session: ExactSession?) {
            self.entry = entry; self.name = name; self.resolver = resolver; self.session = session
        }

        func filePromiseProvider(_ filePromiseProvider: NSFilePromiseProvider, fileNameForType fileType: String) -> String { name }

        func filePromiseProvider(_ filePromiseProvider: NSFilePromiseProvider, writePromiseTo url: URL, completionHandler: @escaping (Error?) -> Void) {
            do {
                if entry.hasPrefix("app:/") {
                    guard let session else { throw CocoaError(.fileReadUnknown) }
                    try DragFiles.copy(entry, to: url, session)
                } else {
                    guard let file = DragFiles.shipped(entry, in: resolver) else { throw CocoaError(.fileReadNoSuchFile) }
                    try FileManager.default.copyItem(at: file, to: url)
                    // A delivered update's file is read-only; the person's copy is not.
                    try? FileManager.default.setAttributes([.posixPermissions: 0o644], ofItemAtPath: url.path)
                }
                completionHandler(nil)
            } catch {
                let reason = (error as? CocoaError)?.code == .fileReadNoSuchFile ? "the app does not ship it" : error.localizedDescription
                completionHandler(DragFiles.failed(entry, reason, session))
            }
        }
    }
}
#endif
