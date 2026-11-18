// @ref LLP 1047.001 D10 — the session's pickers: `showPicker` (LLP 1069.002,
// roster `picker`), and `saveFile` and the File System Access pickers (LLP
// 1069.010, roster `documents`). They are one Swift family: one object
// presents all three kinds of picker and is the one delegate of UIKit's
// document pickers, so its target is linked when the composition names
// either capability. The core keeps the table of `doc:` handles the pickers
// mint (SessionDocuments.swift), since drops, pastes and a document opened
// from outside the app mint there too, and answers `requestPermission` from
// it. The core reaches the pickers only through what this file declares.
// Without them the runner still rules on each request, and one it would
// present is refused as tvOS refuses it, having no picker: logged, and the
// element hears `cancel`. The slot's default is today's picker.
import CExact
import Foundation
#if os(iOS)
import UIKit
#endif

/// What the session's command dispatch asks of its pickers.
package protocol FilePicking: AnyObject {
    /// `showPicker(id)`.
    func show(_ args: [Any])
    /// `saveFile(id, from, suggestedName)` and its `text=` form.
    func save(_ args: [Any])
    /// `showOpenFilePicker`, `showDirectoryPicker` or `showSaveFilePicker`.
    func document(_ name: String, _ args: [Any])
    #if EXACT_AGENT
    // What the agent asks: development only, so production strips them.
    /// The agent answered a held request (LLP 1069.007 D4); each answers
    /// only the hold of its own kind.
    func answered(_ reply: [String: Any], request: [String: Any])
    func saveAnswered(_ reply: [String: Any], request: [String: Any])
    func documentAnswered(_ reply: [String: Any], request: [String: Any])
    #if os(iOS)
    /// Whether a picker shown for real has yet to finish arriving.
    var documentArriving: Bool { get }
    /// `back` with a real picker up: the newest closes, cancelled; false with none.
    func agentBackDocument() -> Bool
    #endif
    #endif
}

/// The runner's requests for `saveFile` and the File System Access pickers,
/// from a command's arguments.
package enum PickerRequests {
    /// `saveFile(id, from, suggestedName)`, or `(id, none, suggestedName,
    /// text)` for `text=`; `startIn=` makes either five (`none` in the
    /// fourth for a copy; LLP 1173 D2).
    package static func save(_ args: [Any], agent: Bool) -> [String: Any] {
        var request: [String: Any] = ["command": "saveFile", "agent": agent]
        let strings = args.map { $0 as? String }
        let copy = args.count == 3 || (args.count == 5 && strings[3] == nil)
        if copy, let id = strings[0], let from = strings[1], let name = strings[2] {
            request["id"] = id; request["from"] = from; request["suggestedName"] = name
        } else if args.count >= 4, strings[1] == nil, let id = strings[0], let name = strings[2], let text = strings[3] {
            request["id"] = id; request["text"] = text; request["suggestedName"] = name
        }
        if args.count == 5, let startIn = strings[4] { request["startIn"] = startIn }
        return request
    }

    /// `showOpenFilePicker(id[, multiple])`, `showDirectoryPicker(id)`,
    /// `showSaveFilePicker(id, suggestedName)`, each with a trailing
    /// `startIn` when the call names one (LLP 1173 D2). Under the agent, a
    /// drive that shows UIKit's own presentations has the picker presented
    /// beside its hold.
    package static func document(_ name: String, _ args: [Any], agent: Bool) -> [String: Any] {
        var request: [String: Any] = ["command": name, "agent": agent]
        if agent && !ExactEnv.agentPaints { request["present"] = true }
        if let id = args.first as? String { request["id"] = id }
        let directory = name == "showDirectoryPicker"
        if args.count > 1, !directory {
            if let multiple = args[1] as? Bool { request["multiple"] = multiple }
            if let suggested = args[1] as? String { request["suggestedName"] = suggested }
        }
        let start = directory ? 1 : 2
        if args.count > start, let startIn = args[start] as? String { request["startIn"] = startIn }
        return request
    }
}

/// No pickers. Each request is ruled as the family rules it, never held for
/// the agent, since nothing could answer the hold; one the family would
/// present is refused at its element.
package final class NoPicker: FilePicking {
    unowned let session: ExactSession
    package init(session: ExactSession) { self.session = session }

    package func show(_ args: [Any]) {
        guard let id = args.first as? String else { session.log("picker: refused: showPicker names an element id"); return }
        let ask = (try? JSONSerialization.data(withJSONObject: ["id": id])).map { String(decoding: $0, as: UTF8.self) } ?? "{}"
        let reply = (try? JSONSerialization.jsonObject(with: Data(session.runtime.ask(exact_picker, ask).utf8))) as? [String: Any]
        guard let view = (reply?["view"] as? NSNumber)?.uint32Value else { session.log("picker: refused: no file input with id \"\(id)\""); return }
        session.log("picker: refused: this build links no file picker")
        session.apply(session.runtime.pickerCancel(view, now: session.now()))
    }

    package func save(_ args: [Any]) { rule("saveFile", PickerRequests.save(args, agent: false)) }
    package func document(_ name: String, _ args: [Any]) { rule(name, PickerRequests.document(name, args, agent: false)) }

    private func rule(_ name: String, _ request: [String: Any]) {
        let ruling = session.runtime.command(request)
        guard let view = (ruling["view"] as? NSNumber)?.uint32Value, ruling["refused"] != nil || ruling["present"] as? Bool == true else { return }
        if ruling["refused"] == nil { session.log("\(name): refused: this build links no document picker") }
        session.log("\(name): cancelled")
        session.apply(session.runtime.pickerCancel(view, now: session.now()))
    }

    #if EXACT_AGENT
    package func answered(_ reply: [String: Any], request: [String: Any]) {}
    package func saveAnswered(_ reply: [String: Any], request: [String: Any]) {}
    package func documentAnswered(_ reply: [String: Any], request: [String: Any]) {}
    #if os(iOS)
    package var documentArriving: Bool { false }
    package func agentBackDocument() -> Bool { false }
    #endif
    #endif
}

/// The slot is set before any session is made, and read on the main thread.
package enum PickerLink {
    /// A session's pickers.
    package static var make: ((ExactSession) -> any FilePicking)? = { Picker(session: $0) }
}

#if os(iOS)
/// What the pickers ask of the photo picker's module, which links PhotosUI
/// (LLP 1047.001.000 E).
package protocol MediaPickerCapability: AnyObject {
    /// Present the photo picker over `controller` for a media-only
    /// `accept`. `done` runs once, on the main queue, with each chosen file
    /// staged in the temporary directory and the name to give it (none
    /// when cancelled), and why a chosen file could not be staged, if one
    /// could not; the caller removes the staged files.
    func present(over controller: UIViewController, accept: [String], multiple: Bool,
                 done: @escaping ([(url: URL, name: String)], String?) -> Void)
}

/// The photo picker's module, installed by the composition.
package enum MediaPickerLink {
    package static var installed: MediaPickerCapability?
}
#endif
