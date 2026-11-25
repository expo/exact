// @exact/clipboard on Apple (LLP 1133.006): the Async Clipboard API's
// `write` with a `ClipboardItem` of PNG, plain text and sanitized HTML.
//
// - The image is sanitized, as the specification asks and Chrome does: the
//   PNG is decoded and a new PNG of its pixels written, so nothing else in
//   the file is carried (D1). An animated PNG writes frame 0 of its
//   animation, which this file cuts out itself: ImageIO's frame 0 of an APNG
//   whose default image is not part of the animation is a later frame
//   (macOS 27), where Chromium's is the first `fcTL`'s.
// - macOS writes what a hand-built AppKit app writes when it copies an
//   image, `writeObjects([NSImage])`, and `public.png` beside it (D5); iOS
//   one item of `public.png` and `public.utf8-plain-text` (D5).
// - Under the agent it is its own substitute (D8 item 4, §7): it writes a
//   private pasteboard named for the session, by the rule and name the
//   host's `copyText` uses (AgentPasteboard.swift), unless `--bind
//   clipboard` or `EXACT_DEVICE=real` asks for the person's. Where the
//   private pasteboard cannot be made the write refuses.
// - A later write supersedes one still decoding, which fails `AbortError` (D2).
import CryptoKit
import ExactPackage
import Foundation
import ImageIO
import libxml2
import UniformTypeIdentifiers
#if os(macOS)
import AppKit
#else
import UIKit
#endif

public final class ClipboardModule: ExactPackage {
    /// D2's bounds: an image file by path, as bytes, its decoded pixels, and the text.
    static let mostFile = 64 * 1024 * 1024, mostBytes = 2 * 1024 * 1024, mostPixels = 64_000_000, mostText = 1024 * 1024
    /// The types this module writes (D1).
    static let types: Set<String> = ["text/plain", "text/html", "image/png"]

    let context: ExactPackageContext
    /// The number of the latest write: one still decoding whose number is older lands nothing.
    private var latest = 0
    /// The write in flight, by its number, and the runner's number for its call.
    private var inFlight: (number: Int, request: Int?, reply: ([String: Any]) -> Void)?
    private var writes = 0
    private let queue = DispatchQueue(label: "exact.clipboard", qos: .userInitiated)

    public init(context: ExactPackageContext) { self.context = context }

    func say(_ line: String) { context.log("clipboard: \(line)") }

    /// The pasteboard written: the person's, or under the agent the session's own (§7).
    var substituting: Bool { context.agent && ProcessInfo.processInfo.environment["EXACT_DEVICE"] != "real" }

    public func later(_ name: String, _ args: [String: Any], preferred: Any?, reply: @escaping ([String: Any]) -> Void) {
        switch name {
        case "read", "readText":
            read(textOnly: name == "readText", reply: reply)
        case "write":
            guard let items = args["data"] as? [Any] else { return reply(refused("TypeError", "write takes a list of ClipboardItem")) }
            write(items, apple: args["apple"], reply: reply)
        case "writeText":
            guard let text = args["data"] as? String else { return reply(refused("TypeError", "writeText takes a string")) }
            write([["text/plain": text]], reply: reply)
        default:
            reply(["error": "NotSupportedError"])
        }
    }

    public func cancel(_ request: Int) {
        guard let w = inFlight, w.request == request else { return }
        latest += 1
        inFlight = nil
        w.reply(refused("AbortError", "the call was let go"))
    }

    public func destroy(_ done: @escaping () -> Void) {
        if let w = inFlight { inFlight = nil; latest += 1; w.reply(refused("AbortError", "the session ended")) }
        done()
    }

    /// A refusal as the binding rejects it, journaled.
    func refused(_ code: String, _ message: String) -> [String: Any] {
        say("write refused: \(code) (\(message))")
        return ["error": code, "message": message]
    }

    private struct WriteOptions {
        #if !os(macOS)
        var values: [UIPasteboard.OptionsKey: Any] = [:]
        #endif

        init(_ apple: Any?) throws {
            #if !os(macOS)
            guard let apple else { return }
            guard let words = apple as? [String: Any] else { throw FileRefusal(code: "TypeError", message: "apple options must be an object") }
            if let local = words["localOnly"] {
                guard let flag = local as? NSNumber, CFGetTypeID(flag) == CFBooleanGetTypeID() else {
                    throw FileRefusal(code: "TypeError", message: "apple.localOnly must be a boolean")
                }
                values[.localOnly] = flag.boolValue
            }
            if let value = words["expirationDate"] {
                guard let number = value as? NSNumber, CFGetTypeID(number) != CFBooleanGetTypeID(), number.doubleValue.isFinite else {
                    throw FileRefusal(code: "TypeError", message: "apple.expirationDate must be finite epoch milliseconds")
                }
                values[.expirationDate] = Date(timeIntervalSince1970: number.doubleValue / 1000)
            }
            #endif
        }
    }

    private func write(_ items: [Any], apple: Any? = nil, reply: @escaping ([String: Any]) -> Void) {
        // Chrome resolves an empty write and leaves the clipboard as it was.
        guard let item = items.first else { return reply([:]) }
        guard items.count == 1 else { return reply(refused("NotAllowedError", "Support for multiple ClipboardItems is not implemented.")) }
        guard let record = item as? [String: Any], !record.isEmpty else { return reply(refused("TypeError", "a ClipboardItem has at least one type")) }
        if let other = record.keys.sorted().first(where: { !Self.types.contains($0) }) {
            return reply(refused("NotAllowedError", "Type \(other) not supported on write."))
        }
        var text: String?
        if let t = record["text/plain"] {
            guard let s = t as? String else { return reply(refused("TypeError", "text/plain is a string")) }
            guard s.utf8.count <= Self.mostText else { return reply(refused("DataError", "text/plain is over 1 MiB")) }
            text = s
        }
        var html: String?
        if let value = record["text/html"] {
            guard let string = value as? String else { return reply(refused("TypeError", "text/html is a string")) }
            guard string.utf8.count <= Self.mostText else { return reply(refused("DataError", "text/html is over 1 MiB")) }
            html = string
        }
        var source: Data?
        switch record["image/png"] {
        case nil: break
        case let path as String:
            guard let url = context.file(path) else { return reply(refused("NotFoundError", "no file at \(path)")) }
            switch Self.contents(url, path) {
            case .success(let data): source = data
            case .failure(let why): return reply(refused(why.code, why.message))
            }
        case let wire as [String: Any]:
            guard let b64 = wire["base64"] as? String, let data = Data(base64Encoded: b64) else { return reply(refused("TypeError", "image/png is an app:/ path or bytes")) }
            guard data.count <= Self.mostBytes else { return reply(refused("DataError", "image/png is over 2 MiB as bytes: pass an app:/ path")) }
            source = data
        default:
            return reply(refused("TypeError", "image/png is an app:/ path or bytes"))
        }
        let options: WriteOptions
        do { options = try WriteOptions(apple) }
        catch let why as FileRefusal { return reply(refused(why.code, why.message)) }
        catch { return reply(refused("TypeError", error.localizedDescription)) }
        // This write supersedes any still decoding (D2).
        latest += 1
        let number = latest
        if let w = inFlight { inFlight = nil; w.reply(refused("AbortError", "a later write superseded it")) }
        inFlight = (number, context.request, reply)
        let finish = { [weak self] (result: Result<(String?, Sanitized?), FileRefusal>) in
            guard let self, self.latest == number, let w = self.inFlight, w.number == number else { return }
            self.inFlight = nil
            switch result {
            case .success(let (html, image)): w.reply(self.land(text: text, html: html, image: image, options: options))
            case .failure(let why): w.reply(self.refused(why.code, why.message))
            }
        }
        guard html != nil || source != nil else { return finish(.success((nil, nil))) }
        queue.async { [html, source] in
            let result = Result { () throws -> (String?, Sanitized?) in
                (try html.map(Self.sanitizedHTML), try source.map { try Self.sanitized($0).get() })
            }.mapError { $0 as? FileRefusal ?? FileRefusal(code: "DataError", message: String(describing: $0)) }
            DispatchQueue.main.async { finish(result) }
        }
    }

    /// The pasteboard written now, and what state discloses of the write.
    private func land(text: String?, html: String?, image: Sanitized?, options: WriteOptions) -> [String: Any] {
        guard let board = paste(text: text, html: html, image: image, options: options) else {
            return refused("UnknownError", "could not write the pasteboard item")
        }
        writes += 1
        var types: [String] = []
        if text != nil { types.append("text/plain") }
        if html != nil { types.append("text/html") }
        if image != nil { types.append("image/png") }
        var shown: [String: Any] = ["types": types, "pasteboard": board.name, "platformTypes": board.types]
        if let text { shown["text"] = String(text.prefix(200)) }
        if let image {
            shown["image"] = ["width": image.width, "height": image.height, "bytes": image.png.count,
                              "sha256": SHA256.hash(data: image.png).map { String(format: "%02x", $0) }.joined()]
            let bytes = NumberFormatter.localizedString(from: image.png.count as NSNumber, number: .decimal)
            say("wrote image/png \(image.width)×\(image.height) (\(bytes) bytes)\(types.filter { $0 != "image/png" }.map { " and " + $0 }.joined())")
        } else if html == nil, let text {
            say("wrote text/plain (\(text.utf8.count) bytes)")
        } else {
            say("wrote \(types.joined(separator: " and "))")
        }
        // The agent's view of the module (`state.extensions.clipboard.implementation`, §7 item 3).
        context.state(["writes": writes, "lastWrite": shown])
        return [:]
    }

    // MARK: the pasteboard (D5)

    /// The pasteboard's name under the agent: the session's, which the host's `copyText` writes too.
    var privateName: String { "exact.agent.clipboard.\(context.hostSession)" }

    #if os(macOS)
    private var board: NSPasteboard {
        substituting ? NSPasteboard(name: NSPasteboard.Name(privateName)) : .general
    }

    private typealias ReadItem = NSPasteboardItem

    private func readItems() throws -> [ReadItem] {
        guard let items = board.pasteboardItems else { throw readFailure() }
        return items
    }

    private func preferredImageType(_ item: ReadItem) -> String? { item.availableType(from: [.png, .tiff])?.rawValue }

    private func readFailure(text: Bool = false, loaded: Bool = false) -> FileRefusal {
        if #available(macOS 15.4, *), [.ask, .alwaysDeny].contains(board.accessBehavior) {
            return FileRefusal(code: "NotAllowedError", message: "pasteboard access was refused")
        }
        return text ? FileRefusal(code: "NotFoundError", message: "could not load plain text")
                    : FileRefusal(code: "NotReadableError", message: "could not read clipboard contents")
    }

    private func types(_ item: ReadItem) -> [String] { item.types.map(\.rawValue) }
    private func text(_ item: ReadItem, _ type: String) throws -> String? { item.string(forType: .init(type)) }
    private func data(_ item: ReadItem, _ type: String) throws -> Data? { item.data(forType: .init(type)) }
    #else
    private var board: UIPasteboard? {
        substituting ? UIPasteboard(name: .init(privateName), create: true) : .general
    }

    private struct ReadItem {
        let board: UIPasteboard
        let index: IndexSet
        let types: [String]
        let change: Int
    }

    private func readItems() throws -> [ReadItem] {
        guard let board else { throw FileRefusal(code: "NotReadableError", message: "private pasteboard unavailable") }
        let change = board.changeCount
        let offered = board.types(forItemSet: nil)
        guard board.changeCount == change else { throw FileRefusal(code: "NotReadableError", message: "clipboard changed during read") }
        guard let types = offered else {
            if board.numberOfItems == 0 { return [] }
            throw readFailure()
        }
        return types.enumerated().map { ReadItem(board: board, index: IndexSet(integer: $0.offset), types: $0.element, change: change) }
    }

    private func preferredImageType(_ item: ReadItem) -> String? {
        [UTType.png.identifier, UTType.tiff.identifier].first(where: item.types.contains)
    }

    private func readFailure(text: Bool = false, loaded: Bool = false) -> FileRefusal {
        // UIKit returns nil without a reason when the person denies the paste.
        if !substituting && !loaded { return FileRefusal(code: "NotAllowedError", message: "pasteboard contents unavailable") }
        return text ? FileRefusal(code: "NotFoundError", message: "could not load plain text")
                    : FileRefusal(code: "NotReadableError", message: "could not read clipboard contents")
    }

    private func types(_ item: ReadItem) -> [String] { item.types }
    private func text(_ item: ReadItem, _ type: String) throws -> String? {
        try checkCurrent(item)
        let value = item.board.values(forPasteboardType: type, inItemSet: item.index)?.first
        try checkCurrent(item)
        if let text = value as? String { return text }
        guard let value else { return nil }
        guard let bytes = value as? Data else { throw TextError.invalidEncoding }
        return try Self.decodeText(bytes, type: type)
    }
    private func data(_ item: ReadItem, _ type: String) throws -> Data? {
        try checkCurrent(item)
        let bytes = item.board.data(forPasteboardType: type, inItemSet: item.index)?.first
        try checkCurrent(item)
        return bytes
    }

    private func checkCurrent(_ item: ReadItem) throws {
        guard item.board.changeCount == item.change else { throw FileRefusal(code: "NotReadableError", message: "clipboard changed during read") }
    }
    #endif

    private static let imageTypes = Set(CGImageSourceCopyTypeIdentifiers() as! [String])

    private func read(textOnly: Bool, reply: @escaping ([String: Any]) -> Void) {
        do {
            let items = try readItems()
            if textOnly {
                for item in items {
                    if let text = try plainText(item) { return reply(["exact:answer": text]) }
                }
                return reply(["exact:answer": ""])
            }
            var records: [[String: String]] = []
            var files: [URL] = []
            var textBytes = 0, imageBytes = 0
            do {
                for item in items {
                    var record: [String: String] = [:]
                    if let text = try plainText(item, loaded: !records.isEmpty) {
                        textBytes += text.utf8.count
                        guard textBytes <= Self.mostText else { throw FileRefusal(code: "DataError", message: "text/plain is over 1 MiB") }
                        record["text/plain"] = text
                    }
                    let offered = types(item)
                    if let type = preferredImageType(item) ?? offered.first(where: {
                        Self.imageTypes.contains($0) && UTType($0)?.conforms(to: .image) == true
                    }), let format = UTType(type) {
                        guard let data = try data(item, type) else { throw readFailure(loaded: !record.isEmpty || !records.isEmpty) }
                        let (path, url) = try keepPNG(data, type: format, total: &imageBytes)
                        files.append(url)
                        record["image/png"] = path
                    }
                    if !record.isEmpty { records.append(record) }
                }
            } catch {
                for url in files { try? FileManager.default.removeItem(at: url) }
                throw error
            }
            reply(["exact:answer": records])
        } catch let why as FileRefusal {
            reply(["error": why.code, "message": why.message])
        } catch {
            reply(["error": "NotReadableError", "message": error.localizedDescription])
        }
    }

    private func plainText(_ item: ReadItem, loaded: Bool = false) throws -> String? {
        let plain = types(item).filter { UTType($0)?.conforms(to: .plainText) == true }
        if plain.isEmpty { return nil }
        let standard = UTType.utf8PlainText.identifier
        var invalid = false
        for type in (plain.contains(standard) ? [standard] : []) + plain.filter({ $0 != standard }) {
            do {
                let value: String?
                if type == UTType.utf16PlainText.identifier || type == UTType.utf16ExternalPlainText.identifier
                    || type == "com.apple.traditional-mac-plain-text" {
                    guard let bytes = try data(item, type) else { continue }
                    value = try Self.decodeText(bytes, type: type)
                } else {
                    value = try text(item, type)
                }
                if let value { return try boundedText(value) }
            } catch TextError.invalidEncoding {
                invalid = true
            }
        }
        if invalid { throw FileRefusal(code: "NotFoundError", message: "could not decode plain text") }
        throw readFailure(text: true, loaded: loaded)
    }

    private enum TextError: Error { case invalidEncoding }

    static func decodeText(_ bytes: Data, type: String) throws -> String {
        let wide = type == UTType.utf16PlainText.identifier || type == UTType.utf16ExternalPlainText.identifier
        guard bytes.count <= (wide ? 2 * mostText + 2 : mostText) else {
            throw FileRefusal(code: "DataError", message: "text/plain is over 1 MiB")
        }
        let encoding: String.Encoding
        if wide {
            guard bytes.count.isMultiple(of: 2) else { throw TextError.invalidEncoding }
            // External UTF-16 defaults to big endian. Native UTF-16 without a BOM uses host byte order.
            if type == UTType.utf16ExternalPlainText.identifier || bytes.starts(with: [0xff, 0xfe]) || bytes.starts(with: [0xfe, 0xff]) {
                encoding = .utf16
            } else {
                encoding = UInt16(littleEndian: 1) == 1 ? .utf16LittleEndian : .utf16BigEndian
            }
        } else {
            encoding = type == "com.apple.traditional-mac-plain-text" ? .macOSRoman : .utf8
        }
        guard let string = String(data: bytes, encoding: encoding) else { throw TextError.invalidEncoding }
        return string
    }

    private func boundedText(_ text: String) throws -> String {
        guard text.utf8.count <= Self.mostText else { throw FileRefusal(code: "DataError", message: "text/plain is over 1 MiB") }
        return text
    }

    private func keepPNG(_ data: Data, type: UTType, total: inout Int) throws -> (String, URL) {
        let size = FileRefusal(code: "DataError", message: "image/png is over 64 MiB")
        guard data.count <= Self.mostFile else { throw size }
        let image: Sanitized
        switch type == .png ? Self.sanitized(data) : Self.sanitizedRaster(data, type: type) {
        case .success(let clean): image = clean
        case .failure(let why): throw FileRefusal(code: "DataError", message: why.description)
        }
        total += image.png.count
        guard total <= Self.mostFile else { throw size }
        let path = "app:/tmp/clipboard-\(UUID().uuidString).png"
        guard let url = context.file(path) else { throw FileRefusal(code: "NotReadableError", message: "temporary storage unavailable") }
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try image.png.write(to: url, options: .withoutOverwriting)
        return (path, url)
    }

    #if os(macOS)
    private func paste(text: String?, html: String?, image: Sanitized?, options: WriteOptions) -> (name: String, types: [String])? {
        let board = self.board
        let item = NSPasteboardItem()
        if let image {
            let picture = NSImage(cgImage: image.cgImage, size: NSSize(width: image.width, height: image.height))
            for type in picture.writableTypes(for: board) where type != .png {
                guard let data = picture.pasteboardPropertyList(forType: type) as? Data, item.setData(data, forType: type) else { return nil }
            }
            guard item.setData(image.png, forType: .png) else { return nil }
        }
        if let html, !item.setData(Data(html.utf8), forType: .html) { return nil }
        if let text, !item.setString(text, forType: .string) { return nil }
        board.clearContents()
        guard board.writeObjects([item]) else { return nil }
        return (board.name.rawValue, (board.types ?? []).map(\.rawValue))
    }
    #else
    /// Nil when the session's private pasteboard could not be made: never the person's in its place.
    private func paste(text: String?, html: String?, image: Sanitized?, options: WriteOptions) -> (name: String, types: [String])? {
        guard let board else { return nil }
        var item: [String: Any] = [:]
        if let image { item[UTType.png.identifier] = image.png }
        if let html { item[UTType.html.identifier] = Data(html.utf8) }
        if let text { item[UTType.utf8PlainText.identifier] = text }
        board.setItems([item], options: options.values)
        return (board.name.rawValue, board.types)
    }
    #endif

    // MARK: sanitizing (D1, D2)

    private static let formatting = Set("a b blockquote br caption code dd del div dl dt em h1 h2 h3 h4 h5 h6 hr i ins kbd li mark ol p pre s small span strong sub sup table tbody td th thead tfoot tr u ul".split(separator: " ").map(String.init))
    private static let containers: Set<String> = ["html", "body", "main", "section", "article", "header", "footer"]

    /// Emit only known markup from the parsed tree. libxml2's HTML parser never renders the document.
    private static func sanitizedHTML(_ html: String) throws -> String {
        if html.isEmpty { return "" }
        let flags = HTML_PARSE_NONET.rawValue | HTML_PARSE_NOERROR.rawValue | HTML_PARSE_NOWARNING.rawValue
        let failure = FileRefusal(code: "NotAllowedError", message: "could not sanitize text/html")
        guard let parser = htmlNewParserCtxt() else { throw failure }
        defer { htmlFreeParserCtxt(parser) }
        guard let doc = htmlCtxtReadMemory(parser, html, Int32(html.utf8.count), nil, "UTF-8", Int32(flags)) else { throw failure }
        defer { xmlFreeDoc(doc) }
        // libxml2 can return a partial document when it reaches its nesting limit.
        guard xmlCtxtGetLastError(parser)?.pointee.level != XML_ERR_FATAL else { throw failure }
        var output = ""
        func escaped(_ value: String) -> String {
            value.replacingOccurrences(of: "&", with: "&amp;").replacingOccurrences(of: "<", with: "&lt;")
                .replacingOccurrences(of: ">", with: "&gt;").replacingOccurrences(of: "\"", with: "&quot;")
        }
        func copy(_ node: xmlNodePtr) {
            if node.pointee.type == XML_TEXT_NODE, let text = node.pointee.content {
                output += escaped(String(cString: text))
                return
            }
            guard node.pointee.type == XML_ELEMENT_NODE, xmlHasProp(node, "hidden") == nil else { return }
            let tag = String(cString: node.pointee.name)
            let keep = formatting.contains(tag)
            guard keep || containers.contains(tag) else { return }
            if keep {
                output += "<" + tag
                if tag == "a", let value = xmlGetProp(node, "href") {
                    defer { xmlFree(value) }
                    let href = String(cString: value)
                    if let url = URLComponents(string: href), let scheme = url.scheme?.lowercased(),
                       scheme == "mailto" || (["http", "https"].contains(scheme) && url.host?.isEmpty == false) {
                        output += " href=\"" + escaped(href) + "\""
                    }
                }
                output += ">"
                if tag == "br" || tag == "hr" { return }
            }
            var child = node.pointee.children
            while let current = child {
                copy(current)
                child = current.pointee.next
            }
            if keep { output += "</" + tag + ">" }
        }
        if let root = xmlDocGetRootElement(doc) { copy(root) }
        // AppKit's HTML importer defaults to a legacy encoding without a charset signature.
        if output.utf8.contains(where: { $0 >= 0x80 }) { output = "\u{feff}" + output }
        guard output.utf8.count <= mostText else { throw FileRefusal(code: "DataError", message: "sanitized text/html is over 1 MiB") }
        return output
    }

    struct Sanitized {
        let png: Data
        let cgImage: CGImage
        let width: Int, height: Int
    }

    struct Refusal: Error, CustomStringConvertible { let description: String }
    struct FileRefusal: Error { let code: String, message: String }

    /// The file at `url`, opened once, so what is checked is what is read: the
    /// file a link names, a regular one (a device's size says nothing of what
    /// it gives), at most 64 MiB by its size and by what is read of it.
    static func contents(_ url: URL, _ path: String) -> Result<Data, FileRefusal> {
        let fd = open(url.path, O_RDONLY)
        guard fd >= 0 else {
            return .failure(errno == ENOENT ? FileRefusal(code: "NotFoundError", message: "no file at \(path)")
                : FileRefusal(code: "NotReadableError", message: "\(path) could not be read"))
        }
        let handle = FileHandle(fileDescriptor: fd, closeOnDealloc: true)
        var info = stat()
        guard fstat(fd, &info) == 0, info.st_mode & S_IFMT == S_IFREG else {
            return .failure(FileRefusal(code: "NotReadableError", message: "\(path) is not a file"))
        }
        let tooLarge = FileRefusal(code: "DataError", message: "too large: \(path) is over 64 MiB")
        guard Int(info.st_size) <= mostFile else { return .failure(tooLarge) }
        guard let data = try? handle.read(upToCount: mostFile + 1) ?? Data() else {
            return .failure(FileRefusal(code: "NotReadableError", message: "\(path) could not be read"))
        }
        return data.count <= mostFile ? .success(data) : .failure(tooLarge)
    }

    /// A new PNG of `data`'s pixels, frame 0 of an animation, or why not.
    static func sanitized(_ data: Data) -> Result<Sanitized, Refusal> {
        guard let still = firstFrame(data) else { return .failure(Refusal(description: "not a PNG")) }
        // The header's size is held to the bound before anything is decoded (D2).
        let side = { (at: Int) in still[still.startIndex + at..<still.startIndex + at + 4].reduce(0) { $0 << 8 | Int($1) } }
        guard still.count >= 24, String(decoding: still[still.startIndex + 12..<still.startIndex + 16], as: UTF8.self) == "IHDR" else {
            return .failure(Refusal(description: "not a PNG"))
        }
        // A side past the PNG specification's 2³¹ − 1 is no PNG, and the product is taken without overflow.
        let (hw, hh) = (side(16), side(20)), (pixels, over) = hw.multipliedReportingOverflow(by: hh)
        guard hw <= Int(Int32.max), hh <= Int(Int32.max) else { return .failure(Refusal(description: "not a PNG")) }
        guard !over, pixels <= mostPixels else { return .failure(Refusal(description: "too large: \(hw)×\(hh) is over 64,000,000 pixels")) }
        return sanitizedRaster(still, type: .png)
    }

    static func sanitizedRaster(_ data: Data, type: UTType) -> Result<Sanitized, Refusal> {
        guard let source = CGImageSourceCreateWithData(data as CFData, [kCGImageSourceShouldCache: false] as CFDictionary),
              CGImageSourceGetType(source) as String? == type.identifier else {
            return .failure(Refusal(description: type == .png ? "not a PNG" : "could not read clipboard image"))
        }
        let header = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any]
        let w = header?[kCGImagePropertyPixelWidth] as? Int ?? 0, h = header?[kCGImagePropertyPixelHeight] as? Int ?? 0
        guard w > 0, h > 0 else { return .failure(Refusal(description: type == .png ? "not a PNG" : "could not read clipboard image dimensions")) }
        let (decoded, wide) = w.multipliedReportingOverflow(by: h)
        guard !wide, decoded <= mostPixels else { return .failure(Refusal(description: "too large: \(w)×\(h) is over 64,000,000 pixels")) }
        let image: CGImage?
        if type != .png, (header?[kCGImagePropertyOrientation] as? Int ?? 1) != 1 {
            image = CGImageSourceCreateThumbnailAtIndex(source, 0, [
                kCGImageSourceCreateThumbnailFromImageAlways: true, kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceThumbnailMaxPixelSize: max(w, h),
            ] as CFDictionary)
        } else {
            image = CGImageSourceCreateImageAtIndex(source, 0, [kCGImageSourceShouldCacheImmediately: true] as CFDictionary)
        }
        guard let image else {
            return .failure(Refusal(description: type == .png ? "not a PNG: it does not decode" : "could not decode clipboard image"))
        }
        let out = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(out, UTType.png.identifier as CFString, 1, nil) else {
            return .failure(Refusal(description: "the PNG encoder is unavailable"))
        }
        CGImageDestinationAddImage(destination, image, nil)
        guard CGImageDestinationFinalize(destination) else { return .failure(Refusal(description: "the PNG did not encode")) }
        return .success(Sanitized(png: out as Data, cgImage: image, width: image.width, height: image.height))
    }

    /// `data` as a still PNG of frame 0 of its animation (the APNG specification's first `fcTL`), or `data`
    /// itself when it is not animated; nil for what is not a PNG. The default image is frame 0 when an `fcTL`
    /// precedes its `IDAT`; otherwise frame 0's `fdAT`s become the still's `IDAT`s.
    static func firstFrame(_ data: Data) -> Data? {
        let signature = Data([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])
        guard data.count > 8, data.prefix(8) == signature else { return nil }
        var chunks: [(type: String, body: Data)] = []
        var at = data.startIndex + 8
        while at + 12 <= data.endIndex {
            let length = data[at..<at + 4].reduce(0) { $0 << 8 | Int($1) }
            guard length >= 0, at + 12 + length <= data.endIndex else { return nil }
            let type = String(decoding: data[at + 4..<at + 8], as: UTF8.self)
            chunks.append((type, Data(data[at + 8..<at + 8 + length])))
            at += 12 + length
            if type == "IEND" { break }
        }
        guard chunks.contains(where: { $0.type == "acTL" }) else { return data }
        guard let idat = chunks.firstIndex(where: { $0.type == "IDAT" }), let fctl = chunks.firstIndex(where: { $0.type == "fcTL" }) else { return nil }
        var kept: [(type: String, body: Data)] = []
        if fctl < idat {
            kept = chunks.filter { !["acTL", "fcTL", "fdAT"].contains($0.type) }
        } else {
            kept = Array(chunks[..<idat]).filter { !["acTL", "fcTL"].contains($0.type) }
            for c in chunks[(fctl + 1)...] {
                if c.type == "fcTL" || c.type == "IEND" { break }
                if c.type == "fdAT", c.body.count > 4 { kept.append(("IDAT", c.body.dropFirst(4))) }
            }
            kept.append(("IEND", Data()))
        }
        var out = signature
        for c in kept { out.append(chunk(c.type, c.body)) }
        return out
    }

    static func chunk(_ type: String, _ body: Data) -> Data {
        var out = Data()
        let n = UInt32(body.count)
        out.append(contentsOf: [UInt8(n >> 24), UInt8(n >> 16 & 255), UInt8(n >> 8 & 255), UInt8(n & 255)])
        let typed = Data(type.utf8) + body
        out.append(typed)
        let c = crc32(typed)
        out.append(contentsOf: [UInt8(c >> 24), UInt8(c >> 16 & 255), UInt8(c >> 8 & 255), UInt8(c & 255)])
        return out
    }

    static let crcTable: [UInt32] = (0..<256).map { n -> UInt32 in
        var c = UInt32(n)
        for _ in 0..<8 { c = c & 1 != 0 ? 0xEDB88320 ^ (c >> 1) : c >> 1 }
        return c
    }

    static func crc32(_ data: Data) -> UInt32 {
        var c: UInt32 = 0xFFFFFFFF
        for b in data { c = crcTable[Int((c ^ UInt32(b)) & 0xFF)] ^ (c >> 8) }
        return c ^ 0xFFFFFFFF
    }
}
