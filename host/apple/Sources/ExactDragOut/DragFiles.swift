// How a drag's file is read when a destination asks (LLP 1094.000 D3), on iOS
// and the Mac: a shipped file through the resolver found at the lift, an
// `app:/` file through `share`'s confined copy. A read that fails is
// journaled, and the destination gets nothing for that file.
#if os(iOS) || os(macOS)
import CExact
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

    /// `entry`, an `app:/` file, copied to `url` through `share`'s confined
    /// copy (`exact_share_file`: the grants, no link followed, at most
    /// 64 MiB). On the main thread, where the runtime is asked.
    static func copy(_ entry: String, to url: URL, _ session: ExactSession) throws {
        let ask = try JSONSerialization.data(withJSONObject: ["op": "shareFile", "path": entry, "to": url.path])
        let reply = (try? JSONSerialization.jsonObject(with: Data(session.runtime.ask(exact_share_file, String(decoding: ask, as: UTF8.self)).utf8))) as? [String: Any]
        guard reply?["file"] is String else { throw failure(reply?["error"] as? String ?? "it could not be copied") }
    }

    /// The journal's `drag: <entry> could not be read: <reason>`, and the
    /// error the destination gets. On the main thread, where the session is
    /// held.
    static func failed(_ entry: String, _ reason: String, _ session: ExactSession?) -> Error {
        let line = "drag: \(entry) could not be read: \(reason)"
        session?.log(line)
        return failure(line)
    }

    private static func failure(_ description: String) -> Error {
        NSError(domain: NSCocoaErrorDomain, code: CocoaError.fileReadUnknown.rawValue, userInfo: [NSLocalizedDescriptionKey: description])
    }
}
#endif
