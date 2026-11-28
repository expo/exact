// Plain field InputEvent metadata and the synchronous beforeinput decision.
#if os(macOS)
import AppKit
#else
import UIKit
#endif
import ExactCore

struct FieldInput {
    var inputType: String
    var data: String?
    var cancelable = true
    var isComposing = false

    func payload(_ value: String, _ selection: FieldSelection) -> String {
        FieldWire.input(type: inputType, data: data, cancelable: cancelable, composing: isComposing,
                        value: value, start: selection.start, end: selection.end, direction: selection.direction)
    }
}

#if os(iOS)
extension NodeView: UITextPasteDelegate {
    // UIKit resolves item providers asynchronously. Intercept the final string,
    // when its native paste range is known, rather than the earlier paste: call.
    package func textPasteConfigurationSupporting(_ supporting: UITextPasteConfigurationSupporting, performPasteOf text: NSAttributedString, to range: UITextRange) -> UITextRange {
        // Only from the editor that stands for this field now: a paste UIKit resolves after the field moved to another
        // (a bar's search retired or set up again) is not this field's, nor one whose `beforeinput` moved it
        guard let editor = supporting as? UITextInput, let fields = presenter?.fieldSelections else { return range }
        let stands = { [weak self] in editor === self?.selectionField || editor === self?.textArea }
        guard stands() else { return range }
        let replaced = NSRange(location: editor.offset(from: editor.beginningOfDocument, to: range.start), length: editor.offset(from: range.start, to: range.end))
        guard !disabled, !inert, props["editable"] != "false" else { return range }
        let replacement = props["type"]?.lowercased() == "number" && editor.markedTextRange == nil
            ? NumberStepping.insertion(text.string, in: fieldText, range: replaced) : text.string
        guard !replacement.isEmpty, TextInputLimit.allows(fieldText, range: replaced, replacement: replacement, props: props) else { return range }
        fields.withIntent(self, "insertFromPaste") {
            fields.nativeEdit(self, replacement: replacement, marked: editor.markedTextRange != nil, stands: stands) {
                editor.replace(range, withText: replacement)
            }
        }
        return editor.selectedTextRange ?? range
    }
}
#endif

extension FieldSelections {
    func withIntent<T>(_ node: NodeView?, _ type: String, _ perform: () -> T) -> T {
        guard let node else { return perform() }
        let old = editIntent[node.id]
        editIntent[node.id] = type
        defer { editIntent[node.id] = old }
        return perform()
    }

    func withComposition<T>(_ node: NodeView?, _ perform: () -> T) -> T {
        guard let node else { return perform() }
        let was = composing.contains(node.id)
        composing.insert(node.id)
        defer { if !was { composing.remove(node.id) } }
        return perform()
    }

    func history(_ node: NodeView?, redo: Bool, _ perform: () -> Void) {
        guard let node else { return perform() }
        let type = redo ? "historyRedo" : "historyUndo"
        inputDetails[node.id] = nil
        defer { inputDetails[node.id] = nil }
        withIntent(node, type) {
            guard shouldEdit(node, range: current(node).range, replacement: "", marked: false) else { return }
            perform()
            node.typedText(node.fieldText)
        }
    }

    /// `stands`: whether the native editor the edit came from still stands for the field, asked again after a
    /// `beforeinput` handler ran.
    func shouldEdit(_ node: NodeView, range: NSRange, replacement: String, marked: Bool, stands: () -> Bool = { true }) -> Bool {
        guard Self.tracks(node), !editing.contains(node.id), !replacing.contains(node.id), !Self.applying(node) else { return true }
        let value = node.fieldText, selection = current(node)
        let ime = marked || composing.contains(node.id)
        let type = ime ? "insertCompositionText" : editIntent[node.id] ??
            (replacement.isEmpty ? "deleteContentBackward" : replacement == "\n" ? "insertLineBreak" : "insertText")
        if type.hasPrefix("history"), inputDetails[node.id]?.inputType == type { return true }
        let data = type.hasPrefix("delete") || type.hasPrefix("history") || type == "insertLineBreak" ? nil : replacement
        let edit = FieldInput(inputType: type, data: data, cancelable: !ime, isComposing: ime)
        let outer = presenter.defaultPrevented
        presenter.defaultPrevented = false
        let heard = node.handlers.contains("beforeinput")
        if heard { onBeforeInput?(node.id, value, selection, edit) }
        let prevented = presenter.defaultPrevented && edit.cancelable
        presenter.defaultPrevented = outer
        guard !prevented else { return false }
        // The handler's batch may have removed the field, written its value or
        // moved it to another editor; the edit then no longer applies. Without
        // a handler nothing ran.
        if heard, presenter.views[node.id] !== node || !ime && node.fieldText != value || !stands() { return false }
        inputDetails[node.id] = edit
        return true
    }

    func input(_ id: UInt32) -> FieldInput? {
        guard var edit = inputDetails.removeValue(forKey: id) else { return nil }
        edit.cancelable = false
        return edit
    }

    // UIKeyInput and marked-text entry points need not ask UIKit's delegate.
    // A delegate called inside the same operation must not ask a second time.
    func nativeEdit(_ node: NodeView?, replacement: String, marked: Bool, stands: () -> Bool = { true }, _ perform: () -> Void) {
        guard let node, Self.tracks(node), !editing.contains(node.id), !Self.applying(node) else { return perform() }
        guard shouldEdit(node, range: current(node).range, replacement: replacement, marked: marked, stands: stands) else { return }
        editing.insert(node.id)
        defer { editing.remove(node.id) }
        perform()
        node.typedText(node.fieldText)
    }
}
