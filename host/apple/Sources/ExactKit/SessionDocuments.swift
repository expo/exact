// @ref LLP 1069.010 D1 — documents in place: the session's table of `doc:`
// handles. What a person chooses in a picker, drops, pastes or opens from
// outside the app (Files' "Open in", Launch Services) is minted here, and
// the app reads and writes it under `fs.read doc:/` / `fs.write doc:/`. Each
// minted URL stays under `startAccessingSecurityScopedResource` while the
// session lives, and `requestPermission` is answered from it. The pickers
// are a family (Links/PickerLink.swift); this table is the core's, since
// drops and pastes need it without them.
import CExact
import Foundation
import UniformTypeIdentifiers

extension ExactSession {
    /// This session's number in the library's document table.
    var documentOwner: UInt64 { UInt64(runtime.rt) }

    package func documentCall(_ request: [String: Any]) -> [String: Any] {
        var request = request
        request["owner"] = documentOwner
        let d = (try? JSONSerialization.data(withJSONObject: request)) ?? Data()
        return (try? JSONSerialization.jsonObject(with: Data(runtime.ask(exact_documents, String(decoding: d, as: UTF8.self)).utf8))) as? [String: Any] ?? [:]
    }

    /// A value arriving at the `open-file` field (LLP 1069.010 D1): a real
    /// path from any host route — Launch Services, the command line, ⌘O,
    /// Open Recent, a path typed there — becomes the document path, for an
    /// app granted `fs.read doc:/`.
    func documentValue(_ view: UInt32, _ value: String) -> String {
        guard value.hasPrefix("/"), presenter.views[view]?.props["testId"] == "open-file" else { return value }
        return documentCall(["op": "openDocument", "path": value])["path"] as? String ?? value
    }

    /// A URL the person chose, minted exactly: `doc:/<n>/<name>`.
    package func mintDocument(_ url: URL) -> String? {
        if url.startAccessingSecurityScopedResource() { documentScopes.append(url) }
        return documentCall(["op": "mintDocument", "path": url.standardizedFileURL.path])["doc"] as? String
    }

    /// `requestPermission(id, path, mode)` (LLP 1069.010.000 D7): the
    /// runner's rule, then `granted` at `id` for a document that resolves
    /// (a native host owns no permission a person could withhold), else `denied`.
    func requestPermission(_ args: [Any]) {
        var request: [String: Any] = ["command": "requestPermission"]
        for (key, value) in zip(["id", "path", "mode"], args) { request[key] = value }
        let ruling = runtime.command(request)
        guard let view = (ruling["view"] as? NSNumber)?.uint32Value, let path = ruling["path"] as? String else { return }
        let granted = documentCall(["op": "documentPermission", "path": path])["granted"] as? Bool == true
        apply(runtime.change(view, granted ? "granted" : "denied", now: now()))
    }

    /// The session ends: its handles go, and so does its access.
    func forgetDocuments() {
        _ = documentCall(["op": "forgetDocuments"])
        PastedImages.forget(self)
        for url in documentScopes { url.stopAccessingSecurityScopedResource() }
        documentScopes = []
    }
}

#if !os(macOS)
extension ExactSession {
    /// Nothing is kept across launches here (LLP 1069.010.000: iOS keeps
    /// nothing until an iOS document app asks).
    func restoreKeptDocuments() {}
}
#endif

#if canImport(UIKit)
extension ExactSession {
    /// A document from Files ("Open in", LLP 1069.010 slice 4): minted
    /// exactly, under its security scope, and delivered to the `open-file`
    /// node once the session has booted (a cold launch brings it first).
    public func openDocument(_ url: URL, tries: Int = 200) {
        guard state != .destroyed else { return }
        guard booted else {
            if tries > 0 { DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { [weak self] in self?.openDocument(url, tries: tries - 1) } }
            return
        }
        guard let doc = mintDocument(url) else { log("open-file: refused: \(url.lastPathComponent) is not a file"); return }
        if !change(testId: "open-file", value: doc) { log("open-file: refused: \(changeRefusal ?? "nothing took it")") }
    }
}
#endif

/// The document types the app declares.
package enum DocumentTypes {
    /// The manifest's `file_handlers`, as the bake wrote them into
    /// `CFBundleDocumentTypes`. The web-page type a Mac browser declares
    /// (`ExactWebPage`, LLP 1030 D2) is Launch Services' alone: an HTML file
    /// it brings is a link (`isWebPageFile`), never a document.
    static func declarations(in metadata: [String: Any] = ExactEnv.appMetadata) -> [[String: Any]] {
        (metadata["CFBundleDocumentTypes"] as? [[String: Any]] ?? []).filter { $0["ExactWebPage"] as? Bool != true }
    }

    /// The declared types: the only ones a picker offers (D2).
    package static var declaredTypes: [UTType] { documentTypes.filter { $0 != .folder } }

    /// Every declared type, and the type this device gives each declared
    /// extension: an app's own format whose extension another app's type
    /// already has (studio diary R13: Freeform has `.board`) is still offered,
    /// whichever type a file of it resolves to here.
    static var documentTypes: [UTType] { types(in: ExactEnv.appMetadata) }

    static func types(in metadata: [String: Any]) -> [UTType] {
        let declarations = declarations(in: metadata)
        let declared = declarations.flatMap { $0["LSItemContentTypes"] as? [String] ?? [] }.compactMap { UTType($0) }
        let byExtension = declarations.flatMap { $0["CFBundleTypeExtensions"] as? [String] ?? [] }.compactMap { UTType(filenameExtension: $0) }
        return (declared + byExtension).reduce(into: []) { types, t in if !types.contains(t) { types.append(t) } }
    }
}
