// The agent's `tap` and `type` into an iframe guest. They are script events,
// untrusted on purpose (@ref LLP 1020 D4; exact1
// 20260806-webview-frame-guest-click-delivery), so each also does what the
// browser does after a person's input that no listener cancelled: a press
// focuses and places the caret at its point, a key's modifiers are the
// event's fields and a printable key inserts its text, and text goes in at
// the caret. The partitioned frame's automation does the same
// (`webarm/FrameAutomation.swift`).
import Foundation

enum GuestInput {
    /// A press at (`x`, `y`) in the guest's viewport, or on the element
    /// `selector` names (its middle unless a point is given).
    static func tap(selector: String?, x: Double?, y: Double?) -> String {
        var arguments: [String: Any] = [:]
        if let selector { arguments["selector"] = selector }
        if let x { arguments["x"] = x }
        if let y { arguments["y"] = y }
        return call(tapScript, arguments)
    }

    /// `text` into the editable `selector` names, else the focused one, else
    /// the first shown; or the key chord `key` (`Meta+s`, `Shift+Tab`, `Plus`).
    static func type(selector: String?, key: String?, text: String) -> String {
        var arguments: [String: Any] = ["text": text]
        if let selector { arguments["selector"] = selector }
        if let key { arguments["key"] = key }
        return call(typeScript, arguments)
    }

    private static func call(_ function: String, _ arguments: [String: Any]) -> String {
        let json = (try? JSONSerialization.data(withJSONObject: arguments)).flatMap { String(data: $0, encoding: .utf8) } ?? "{}"
        return "(\(function))(\(json))"
    }

    private static let tapScript = """
    (a) => {
      const named = a.selector ? document.querySelector(a.selector) : null;
      const box = named?.getBoundingClientRect();
      const x = a.x ?? (box ? box.left + box.width / 2 : innerWidth / 2);
      const y = a.y ?? (box ? box.top + box.height / 2 : innerHeight / 2);
      const target = named || document.elementFromPoint(x, y) || document.body;
      if (!target) return JSON.stringify({ok:false, error:'guest tap found no target'});
      const base = {bubbles:true, cancelable:true, composed:true, clientX:x, clientY:y, button:0, view:window};
      const pointer = {pointerId:1, pointerType:'mouse', isPrimary:true};
      target.dispatchEvent(new PointerEvent('pointerdown', {...base, ...pointer, buttons:1}));
      if (target.dispatchEvent(new MouseEvent('mousedown', {...base, buttons:1}))) {
        target.closest?.('input,textarea,select,button,a[href],[tabindex],[contenteditable]:not([contenteditable="false"])')?.focus();
        const caret = document.caretRangeFromPoint?.(x, y);
        if (caret && !target.matches?.('input,textarea')) {
          const selection = getSelection();
          selection.removeAllRanges();
          selection.addRange(caret);
        }
      }
      target.dispatchEvent(new PointerEvent('pointerup', {...base, ...pointer, buttons:0}));
      target.dispatchEvent(new MouseEvent('mouseup', {...base, buttons:0}));
      target.dispatchEvent(new MouseEvent('click', {...base, buttons:0}));
      return JSON.stringify({ok:true});
    }
    """

    private static let typeScript = """
    (a) => {
      const editables = 'input,textarea,[contenteditable]:not([contenteditable="false"])';
      const shown = (e) => !e.disabled && e.getClientRects().length > 0 && getComputedStyle(e).visibility !== 'hidden';
      const active = document.activeElement;
      const focused = active && active !== document.body && (active.matches?.(editables) || active.isContentEditable) ? active : null;
      const target = (a.selector ? document.querySelector(a.selector) : null) || focused || [...document.querySelectorAll(editables)].find(shown);
      if (!target) return JSON.stringify({ok:false, error:'guest type found no target'});
      const selection = getSelection();
      // Read before focus, which puts a caret at an editable's start.
      const caret = selection.rangeCount > 0 && target.contains(selection.anchorNode);
      if (target !== document.activeElement) target.focus();
      const field = 'value' in target && !target.isContentEditable;
      if (a.key !== undefined) {
        const parts = (a.key === '+' ? 'Plus' : a.key.endsWith('++') ? a.key.slice(0, -1) + 'Plus' : a.key).split('+');
        const last = parts.pop(), key = last === 'Plus' ? '+' : last;
        const held = {Shift:'shiftKey', Control:'ctrlKey', Alt:'altKey', Meta:'metaKey'};
        const init = {key, bubbles:true, cancelable:true, composed:true, view:window};
        for (const m of parts) {
          if (!held[m]) return JSON.stringify({ok:false, error:'unknown key modifier ' + m});
          init[held[m]] = true;
        }
        if (target.dispatchEvent(new KeyboardEvent('keydown', init)) && !init.ctrlKey && !init.metaKey) {
          if (key.length === 1) { if (target.dispatchEvent(new KeyboardEvent('keypress', init))) document.execCommand('insertText', false, key); }
          else if (key === 'Backspace') document.execCommand('delete');
          else if (key === 'Delete') document.execCommand('forwardDelete');
          else if (key === 'Enter' && field && target.form && target.localName === 'input') target.form.requestSubmit();
          else if (key === 'Enter') document.execCommand('insertParagraph');
        }
        target.dispatchEvent(new KeyboardEvent('keyup', init));
      } else if (field) {
        target.value = a.text;
        target.dispatchEvent(new InputEvent('input', {data:a.text, inputType:'insertText', bubbles:true, composed:true}));
        target.dispatchEvent(new Event('change', {bubbles:true, composed:true}));
      } else {
        // At the caret; with none in the editable, after what it holds.
        if (!caret) {
          const end = document.createRange();
          end.selectNodeContents(target);
          end.collapse(false);
          selection.removeAllRanges();
          selection.addRange(end);
        }
        document.execCommand('insertText', false, a.text);
      }
      // A password's value is never agent output (#134): a fixed mark.
      return JSON.stringify({ok:true, value:field ? (target.type === 'password' && target.value ? '•••' : target.value) : target.textContent});
    }
    """
}
