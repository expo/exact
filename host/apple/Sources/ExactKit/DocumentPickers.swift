// @ref LLP 1069.010 D1, D2 — documents in place. What the person chooses
// becomes a `doc:` handle the library mints for this session; the app reads
// and writes it under `fs.read doc:/` / `fs.write doc:/`. The File System
// Access API's three pickers are commands: the runner rules first (refused,
// or under the agent held as `open-file`, `open-directory`, `save-file` for
// `type @t <path>` / `tap @t cancel`), then macOS shows an NSOpenPanel or
// NSSavePanel sheet and iOS a document picker opening in place. The
// handle(s) arrive as `change` on the element the command names, one per
// line; a dismissed picker is HTML's `cancel` there. Each chosen URL stays
// under `startAccessingSecurityScopedResource` while the session lives: a
// no-op for today's unsandboxed builds, the call a sandboxed one needs, and
// what a security-scoped bookmark would be made from (ruled: the Mac App
// Store stays possible). A drive that shows UIKit's own presentations
// (`--touch platform`) keeps the hold and presents the real picker too: what
// the person chooses there answers the hold, and the agent's answer, or
// `back`, closes it.
import CExact
import Foundation
import UniformTypeIdentifiers
#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif

extension Picker {
    /// `showOpenFilePicker(id[, multiple])`, `showDirectoryPicker(id)`,
    /// `showSaveFilePicker(id, suggestedName)`, from any action, each with a
    /// trailing `startIn` when the call names one (LLP 1173 D2).
    func document(_ name: String, _ args: [Any]) {
        let ruling = session.runtime.command(PickerRequests.document(name, args, agent: ExactEnv.agentMode))
        let view = (ruling["view"] as? NSNumber)?.uint32Value
        if ruling["refused"] != nil { if let view { documentCancelled(view, name) }; return }
        guard ruling["present"] as? Bool == true, let view else { return }
        if let ticket = (ruling["ticket"] as? NSNumber)?.uint64Value { documentTickets[view] = ticket }
        presentDocument(name, view: view, multiple: ruling["multiple"] as? Bool ?? false,
                        suggestedName: ruling["suggestedName"] as? String ?? "",
                        startIn: startFolder(ruling["startIn"] as? String, name))
    }

    /// Where a picker or save panel opens for `startIn` (LLP 1173 D2): a
    /// `doc:` handle's folder, or on a Mac the well-known folder of that
    /// name; iOS has none of those, and opens where it would. One that does
    /// not resolve is refused into the journal, and the panel opens where
    /// the platform would.
    func startFolder(_ start: String?, _ name: String) -> URL? {
        guard let start else { return nil }
        if start.hasPrefix("doc:/") {
            if let folder = session.documentCall(["op": "documentFolder", "path": start])["folder"] as? String {
                return URL(fileURLWithPath: folder, isDirectory: true)
            }
            session.log("\(name): startIn: refused: \(start) is not a document this session holds")
            return nil
        }
        #if os(macOS)
        if let folder = Picker.knownFolder(start) { return folder }
        session.log("\(name): startIn: refused: \(start) is not a folder here")
        #endif
        return nil
    }

    #if os(macOS)
    /// The well-known folder `name` names (LLP 1173 D4's table), asked at
    /// each use: the home's Desktop, Documents, Downloads, Music, Pictures
    /// and Movies, and `screenshots` where the Screenshot app saves
    /// (`com.apple.screencapture`'s `location-screenshot`, which macOS 27's
    /// `screencapture` reads, else `location`, else the Desktop). Nil for a
    /// destination that is not a folder (the Clipboard, Mail, an app).
    static func knownFolder(_ name: String) -> URL? {
        let directory: FileManager.SearchPathDirectory
        switch name {
        case "desktop": directory = .desktopDirectory
        case "documents": directory = .documentDirectory
        case "downloads": directory = .downloadsDirectory
        case "music": directory = .musicDirectory
        case "pictures": directory = .picturesDirectory
        case "videos": directory = .moviesDirectory
        case "screenshots":
            let screenshots = UserDefaults(suiteName: "com.apple.screencapture")
            if let set = screenshots?.string(forKey: "location-screenshot") ?? screenshots?.string(forKey: "location") {
                return folder(URL(fileURLWithPath: (set as NSString).expandingTildeInPath, isDirectory: true))
            }
            directory = .desktopDirectory
        default: return nil
        }
        return FileManager.default.urls(for: directory, in: .userDomainMask).first.flatMap(folder)
    }

    private static func folder(_ url: URL) -> URL? {
        var isDirectory: ObjCBool = false
        return FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory) && isDirectory.boolValue ? url : nil
    }
    #endif

    /// The agent's answer to a held picker: each path minted as a chosen
    /// file would be.
    func documentAnswered(_ reply: [String: Any], request: [String: Any]) {
        guard let capability = reply["capability"] as? String,
              let name = ["open-file": "showOpenFilePicker", "open-directory": "showDirectoryPicker",
                          "save-file": "showSaveFilePicker"][capability],
              let view = (reply["node"] as? NSNumber)?.uint32Value else { return }
        // The real picker shown beside the hold closes, its outcome the agent's.
        if documentTickets.removeValue(forKey: view) != nil { closeDocument(view) }
        if reply["answered"] as? String == "cancel" { documentCancelled(view, name); return }
        let paths = (request["text"] as? String ?? "").split(whereSeparator: \.isNewline)
            .map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
        documentsChosen(view, paths.map { URL(fileURLWithPath: $0) }, name)
    }

    /// A shown picker's outcome answers its hold, as `type @t <paths>` or
    /// `tap @t cancel` would (journalled, never echoed); nil is a cancel.
    private func answerHold(_ view: UInt32, _ urls: [URL]?) {
        guard let ticket = documentTickets.removeValue(forKey: view) else { return }
        var ask: [String: Any] = ["op": urls == nil ? "tap" : "type", "ticket": ticket]
        if let urls { ask["text"] = urls.map(\.path).joined(separator: "\n") } else { ask["choice"] = "cancel" }
        if let data = try? JSONSerialization.data(withJSONObject: ask) { _ = session.runtime.ask(exact_answer_ticket, String(decoding: data, as: UTF8.self)) }
    }

    func documentsChosen(_ view: UInt32, _ urls: [URL], _ name: String) {
        answerHold(view, urls.isEmpty ? nil : urls)
        #if os(macOS)
        let docs = urls.compactMap { session.keptChoice($0) ?? session.mintDocument($0) }
        #else
        let docs = urls.compactMap { session.mintDocument($0) }
        #endif
        guard !docs.isEmpty else { session.log("\(name): refused: nothing to open"); documentCancelled(view, name); return }
        session.log("\(name): chosen")
        #if os(macOS)
        // A file chosen in the app is a document opened, as one Finder or
        // ⌘O brings is (studio diary R18): it joins Open Recent, and the
        // window that shows it is no longer an empty one.
        ExactDocuments.chosen(urls, in: session)
        #endif
        session.apply(session.runtime.change(view, docs.joined(separator: "\n"), now: session.now()))
    }

    func documentCancelled(_ view: UInt32, _ name: String) {
        answerHold(view, nil)
        session.log("\(name): cancelled")
        session.apply(session.runtime.pickerCancel(view, now: session.now()))
    }
}

#if os(tvOS)
extension Picker {
    func presentDocument(_ name: String, view: UInt32, multiple _: Bool, suggestedName _: String, startIn _: URL?) {
        // tvOS has no document picker.
        session.log("\(name): refused: no document picker"); documentCancelled(view, name)
    }
    func closeDocument(_: UInt32) {}
}
#elseif canImport(UIKit)
extension Picker {
    func presentDocument(_ name: String, view: UInt32, multiple: Bool, suggestedName: String, startIn: URL?) {
        guard let controller = ViewPresentation.presenter(for: session.presenter.views[view] ?? session.presenter.root) else {
            session.log("\(name): refused: no window"); documentCancelled(view, name); return
        }
        let picker: UIDocumentPickerViewController
        switch name {
        case "showDirectoryPicker":
            picker = UIDocumentPickerViewController(forOpeningContentTypes: [.folder], asCopy: false)
        case "showSaveFilePicker":
            // iOS saves by moving a file where the person says: an empty one
            // under the suggested name, whose new location is the handle.
            let dir = FileManager.default.temporaryDirectory.appendingPathComponent("exact-save-\(UUID().uuidString)", isDirectory: true)
            let staged = dir.appendingPathComponent(suggestedName)
            guard (try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)) != nil,
                  FileManager.default.createFile(atPath: staged.path, contents: Data()) else {
                session.log("\(name): refused: no scratch file"); documentCancelled(view, name); return
            }
            picker = UIDocumentPickerViewController(forExporting: [staged], asCopy: false)
        default:
            let types = DocumentTypes.declaredTypes
            picker = UIDocumentPickerViewController(forOpeningContentTypes: types.isEmpty ? [.item] : types, asCopy: false)
            picker.allowsMultipleSelection = multiple
        }
        picker.directoryURL = startIn
        picker.delegate = self
        documentRequests[ObjectIdentifier(picker)] = (view, name)
        if documentTickets[view] != nil { documentShown[view] = picker }
        controller.present(picker, animated: true)
    }

    /// Close the picker shown for real at `view`, its outcome said elsewhere.
    func closeDocument(_ view: UInt32) {
        guard let picker = documentShown.removeValue(forKey: view) else { return }
        documentRequests[ObjectIdentifier(picker)] = nil
        picker.dismiss(animated: !ExactEnv.agentFreezes)
    }

    /// `back` with a real picker up (`--touch platform`): the newest closes,
    /// cancelled, as its Cancel button closes it. False with none.
    func agentBackDocument() -> Bool {
        guard let (view, picker) = documentShown.max(by: { $0.key < $1.key }),
              let (_, name) = documentRequests[ObjectIdentifier(picker)] else { return false }
        closeDocument(view)
        documentCancelled(view, name)
        return true
    }

    /// Whether a picker shown for real has yet to finish arriving (ShareSheet.arriving).
    var documentArriving: Bool { documentShown.values.contains { $0.presentingViewController == nil || $0.isBeingPresented } }

    /// A document picker's outcome; `false` when `controller` is not one.
    func documentFinished(_ controller: UIDocumentPickerViewController, urls: [URL]?) -> Bool {
        guard let (view, name) = documentRequests.removeValue(forKey: ObjectIdentifier(controller)) else { return false }
        documentShown[view] = nil
        if let urls, !urls.isEmpty { documentsChosen(view, urls, name) } else { documentCancelled(view, name) }
        return true
    }
}
#else
extension Picker {
    /// AppKit's panels are never shown beside a hold: no drive shows them (`--touch platform` is iOS's).
    func closeDocument(_: UInt32) {}

    func presentDocument(_ name: String, view: UInt32, multiple: Bool, suggestedName: String, startIn: URL?) {
        let panel: NSSavePanel
        if name == "showSaveFilePicker" {
            panel = NSSavePanel()
            panel.nameFieldStringValue = suggestedName
            panel.canCreateDirectories = true
        } else {
            let open = NSOpenPanel()
            open.canChooseFiles = name == "showOpenFilePicker"
            open.canChooseDirectories = name == "showDirectoryPicker"
            open.allowsMultipleSelection = multiple
            open.treatsFilePackagesAsDirectories = false
            let types = DocumentTypes.declaredTypes
            if open.canChooseFiles && !types.isEmpty { open.allowedContentTypes = types }
            panel = open
        }
        if let startIn { panel.directoryURL = startIn }
        let finish: (NSApplication.ModalResponse) -> Void = { [weak self, weak panel] response in
            guard let self, let panel else { return }
            let urls = (panel as? NSOpenPanel)?.urls ?? panel.url.map { [$0] } ?? []
            if response == .OK, !urls.isEmpty { self.documentsChosen(view, urls, name) } else { self.documentCancelled(view, name) }
        }
        if let window = session.presenter.root.window { panel.beginSheetModal(for: window, completionHandler: finish) }
        else { panel.begin(completionHandler: finish) }
    }
}
#endif
