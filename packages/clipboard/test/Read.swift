import AppKit
import ExactPackage
import ImageIO
import UniformTypeIdentifiers

@main
struct ReadTests {
    static func main() {
        let session = UUID().uuidString
        let context = ExactPackageContext(agent: true, session: session, hostSession: session,
                                          changed: { _ in }, log: { _ in }, now: { 0 }, file: { _ in nil })
        let module = ClipboardModule(context: context)
        let board = NSPasteboard(name: .init("exact.agent.clipboard." + session))
        defer { board.releaseGlobally() }

        func call(_ name: String) -> [String: Any] {
            var answer: [String: Any] = [:]
            module.later(name, [:], preferred: nil) { answer = $0 }
            return answer
        }
        func check(_ items: [NSPasteboardItem], _ strings: [String]) {
            board.clearContents()
            if !items.isEmpty { precondition(board.writeObjects(items)) }
            let answer = call("read")["exact:answer"] as? [[String: String]]
            precondition(answer == strings.map { ["text/plain": $0] })
            precondition(call("readText")["exact:answer"] as? String == (strings.first ?? ""))
        }
        func item(_ text: String, _ type: NSPasteboard.PasteboardType) -> NSPasteboardItem {
            let item = NSPasteboardItem()
            precondition(item.setString(text, forType: type))
            return item
        }
        func writeItem(_ record: [String: Any]) -> [String: Any] {
            var answer: [String: Any]?
            module.later("write", ["data": [record]], preferred: nil) { answer = $0 }
            let end = Date(timeIntervalSinceNow: 10)
            while answer == nil && Date() < end { _ = RunLoop.main.run(mode: .default, before: end) }
            return answer!
        }
        let html = #"<b onclick="void 0">Résumé</b> <a href="javascript:void(0)">note</a><script>discard</script><a href="https://example.org/">safe</a>"#
        for fallback in [nil, "", "plain"] as [String?] {
            var record = ["text/html": html]
            record["text/plain"] = fallback
            precondition(writeItem(record).isEmpty)
            precondition(board.pasteboardItems?.count == 1)
            let item = board.pasteboardItems!.first!
            let data = item.data(forType: .html)!
            let stored = String(decoding: data, as: UTF8.self)
            precondition(stored.contains("<b>Résumé</b>") && stored.contains("<a>note</a>"))
            precondition(stored.contains("https://example.org/") && !stored.contains("script") && !stored.contains("discard"))
            precondition(item.string(forType: .string) == fallback)
        }
        let nearLimit = "<p>" + String(repeating: "a", count: 1_048_560 - 7) + "</p>"
        precondition(writeItem(["text/html": nearLimit]).isEmpty)
        precondition(board.data(forType: .html) == Data(nearLimit.utf8))
        let change = board.changeCount
        precondition(writeItem(["text/html": String(repeating: "&", count: 220_000)])["error"] as? String == "DataError")
        let nested = String(repeating: "<div>", count: 700) + "note" + String(repeating: "</div>", count: 700)
        precondition(writeItem(["text/html": nested])["error"] as? String == "NotAllowedError")
        precondition(board.changeCount == change)
        let plain = NSPasteboard.PasteboardType(UTType.plainText.identifier)
        check([], [])
        check([item("first", plain), item("second", .string)], ["first", "second"])
        check([item("", plain), item("next", .string)], ["", "next"])
        let both = item("generic", plain)
        precondition(both.setString("standard", forType: .string))
        check([both], ["standard"])
        for (type, encoding) in [(UTType.utf16PlainText, String.Encoding.utf16LittleEndian),
                                 (.utf16ExternalPlainText, .utf16BigEndian), (.utf16ExternalPlainText, .utf16)] {
            let item = NSPasteboardItem()
            precondition(item.setData("héllo".data(using: encoding)!, forType: .init(type.identifier)))
            check([item], ["héllo"])
        }
        let legacyType = "com.apple.traditional-mac-plain-text"
        let legacy = NSPasteboardItem()
        precondition(legacy.setData("café".data(using: .macOSRoman)!, forType: .init(legacyType)))
        check([legacy], ["café"])
        precondition((try? ClipboardModule.decodeText(Data([0xff, 0x80]), type: UTType.utf8PlainText.identifier)) == nil)
        precondition((try? ClipboardModule.decodeText(Data([0xff]), type: UTType.utf16PlainText.identifier)) == nil)
        let pixels = CGContext(data: nil, width: 8, height: 4, bitsPerComponent: 8, bytesPerRow: 32,
                               space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue)!
        let jpeg = NSMutableData()
        let dest = CGImageDestinationCreateWithData(jpeg, UTType.jpeg.identifier as CFString, 1, nil)!
        CGImageDestinationAddImage(dest, pixels.makeImage()!, [kCGImagePropertyOrientation: 6] as CFDictionary)
        precondition(CGImageDestinationFinalize(dest))
        guard case .success(let image) = ClipboardModule.sanitizedRaster(jpeg as Data, type: .jpeg) else {
            fatalError("could not convert oriented JPEG")
        }
        precondition(image.width == 4 && image.height == 8)
        precondition(writeItem(["image/png": ["base64": image.png.base64EncodedString()], "text/html": html, "text/plain": "note"]).isEmpty)
        let mixed = board.pasteboardItems!
        precondition(mixed.count == 1 && mixed[0].string(forType: .string) == "note")
        precondition(mixed[0].data(forType: .html) != nil && mixed[0].data(forType: .png) != nil)
        precondition(NSImage(data: mixed[0].data(forType: .tiff)!) != nil)
        let full = String(repeating: "é", count: 524288)
        check([item(full, .string)], [full])
        board.clearContents()
        precondition(board.writeObjects([item(full, .string), item("x", plain)]))
        precondition(call("read")["error"] as? String == "DataError")
        precondition(call("readText")["exact:answer"] as? String == full)
    }
}
