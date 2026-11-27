import { log } from "./routes.js";
import { textSource, textPosition } from "./selection.js";
// LLP 1069.001 D4: restore controlled choices after options change or an edit is refused.
// After the input action (`acted`), keep the person's choice.
const VALUED = new Set([
  "range",
  "date",
  "time",
  "datetime-local"
]);
export const valuedControl = (el) => el instanceof HTMLSelectElement || el instanceof HTMLInputElement && VALUED.has(el.type);
export function settleValue(el, acted = false) {
  const c = el instanceof HTMLOptionElement ? el.parentElement : el;
  if (acted) {
    return;
  }
  if (c && valuedControl(c) && c.exactValue !== undefined && c.value !== c.exactValue) {
    c.value = c.exactValue;
  }
}
// What `type <id> <value>` sets rather than types into (D9): the valued controls, a checkbox (or `switch`), which a
// value turns on or off, and a radio, which `true` checks (x2apps survey #2).
export const typedControl = (el) => valuedControl(el) || el instanceof HTMLInputElement && (el.type === "checkbox" || el.type === "radio");
// A text field (x2apps codeedit #2): an input that is no control, or a textarea that is not the Markdown editor; its `input` and `change` carry its selection, and its `select` is HTML's. A `sourceField` adds the Markdown editor, whose `input`, `selectionchange` and `beforeinput` carry its source selection (LLP 1045 D6) and whose `select` is its own.
export const textField = (el) => el instanceof HTMLTextAreaElement ? el.getAttribute("markup") !== "markdown" : el instanceof HTMLInputElement && !typedControl(el) && el.type !== "file", sourceField = (el) => !!el?.exactMarkup || el instanceof HTMLTextAreaElement || textField(el);
export const editMenu = (target) => {
  const f = target.closest("input,textarea,[contenteditable]");
  return !!f && (!(f instanceof HTMLInputElement) || textField(f));
};
// a `contextmenu` from a text field, a textarea or an editable region opens the browser's edit menu and runs no handler (review b5-b 3); a control's input (a checkbox, switch, radio, range, date, time or file) has no such menu, so its node's handler runs and its popover opens, as Chrome sends the event and macOS runs it (#487)
// Host kinds 40–42 carry UTF-16 start,end,direction before the field’s text.
export const fieldSelection = (el) => {
  const end = el.value.length;
  return `${el.selectionStart ?? end},${el.selectionEnd ?? end},${el.selectionDirection ?? "none"},`;
};
// Restore unchecked peers first so the committed radio wins; an absent bound value is uncontrolled.
export function settleRadios(el, bound) {
  const group = el.name ? [...document.querySelectorAll("input[type=\"radio\"]")].filter((r) => r.name === el.name) : [el];
  for (const r of group) {
    if (bound(r) === false && r.checked) {
      r.checked = false;
    }
  }
  for (const r of group) {
    if (bound(r) === true && !r.checked) {
      r.checked = true;
    }
  }
}
// DOM selection offsets count the line endings the widget actually displays.
export function fieldText(el, text) {
  if (el.localName === "textarea" || el.exactMarkup) return text.replace(/\r\n?/g, "\n");
  if (el.localName === "input" && /^(text|search|password|url|tel|email)$/.test(el.type)) {
    text = text.replace(/[\r\n]/g, "");
    if (el.type === "url" || el.type === "email") return text.replace(/^[\t\f ]+|[\t\f ]+$/g, "");
  }
  return text;
}
// Both web targets use the field's post-edit selection, in UTF-16 offsets (LLP 1102 §3.16).
export function replaceFieldText(el, value, filtered = true) {
  const markup = el.exactMarkup ?? el.$markup;
  if (markup) return markup.setValue(value, filtered);
  const old = el.value;
  if (el.ownerDocument?.activeElement !== el || !filtered && old === "" ||
      el.localName !== "textarea" && !(el.localName === "input" && /^(text|search|password|tel|url)$/.test(el.type))) {
    el.value = value;
    return;
  }
  const start = el.selectionStart, end = el.selectionEnd;
  let prefix = 0, suffix = 0;
  const limit = Math.min(old.length, value.length);
  while (prefix < limit && old[prefix] === value[prefix]) prefix++;
  while (suffix < limit - prefix && old[old.length - suffix - 1] === value[value.length - suffix - 1]) suffix++;
  const oldEnd = old.length - suffix, newEnd = value.length - suffix;
  const carry = p => p <= prefix ? p : p < oldEnd ? (filtered ? Math.min(p, newEnd) : newEnd) : newEnd + p - oldEnd;
  const direction = el.selectionDirection;
  el.setRangeText(value.slice(prefix, newEnd), prefix, oldEnd, "preserve");
  el.setSelectionRange(carry(start), carry(end), direction);
}
export function setFieldSelection(el, args, log) {
  const [id, start, end, direction] = args ?? [];
  if (!el) {
    return log(`setSelectionRange "${id}" refused: no live node with that id`);
  }
  if (el.matches("[data-exact-text], [data-exact-text] *")) {
    const size = textSource(el).length, clamp = (n) => Math.min(size, Number(n) >>> 0), b = clamp(end), a = Math.min(clamp(start), b);
    const [an, ao] = textPosition(el, a), [bn, bo] = textPosition(el, b);
    getSelection().setBaseAndExtent(...direction === "backward" ? [
      bn,
      bo,
      an,
      ao
    ] : [
      an,
      ao,
      bn,
      bo
    ]);
    return;
  }
  if (typeof el.setSelectionRange !== "function") {
    return log(`setSelectionRange "${id}" refused: not a text field or text`);
  }
  try {
    el.setSelectionRange(start, end, direction);
  } catch (error) {
    log(`setSelectionRange "${id}" refused: ${el.exactMarkup ? error.message : `an input type=${el.type} has no text selection`}`);
  }
}
// @ref LLP 1069.001, 2026-10-12 — HTML's range edit, plus one input to synchronize the app's value; no undo step (the Markdown editor's makes one and its own input, LLP 1045 D1).
export function setFieldRangeText(el, args, log) {
  const [id, text, ...range] = args;
  if (!el) {
    return log(`setRangeText "${id}" refused: no live node with that id`);
  }
  if (typeof el.setRangeText !== "function") {
    return log(`setRangeText "${id}" refused: not a text field`);
  }
  // a Markdown textarea before its editor loads is a plain source editor
  try {
    el.setRangeText(text, ...range);
  } catch (error) {
    return log(`setRangeText "${id}" refused: ${el.exactMarkup ? error.message : error.name}`);
  }
  if (!el.exactMarkup) {
    el.dispatchEvent(new InputEvent("input", {
      inputType: "insertReplacementText",
      data: text,
      bubbles: true
    }));
  }
}
// Edit data is length-prefixed in UTF-8; commas and newlines are valid text.
function inputPayload(el, e) {
  return `${e.inputType ?? ""},${+e.cancelable},${+!!e.isComposing},${e.data == null ? -1 : new TextEncoder().encode(e.data).length},${e.data ?? ""}${fieldSelection(el)}${el.value}`;
}
export function controlEvent(el, kind, on, dispatch) {
  if (kind === "select" || kind === "selectionchange") {
    return void ((kind === "select" ? textField : sourceField)(el) && on(kind, () => dispatch(kind === "select" ? 42 : 52, fieldSelection(el) + el.value)));
  }
  if (kind === "beforeinput") {
    return on(kind, (e) => dispatch(53, inputPayload(el, e), e));
  }
  if (el.type === "checkbox") {
    return on(kind, () => {
      dispatch(kind === "change" ? 24 : 25, String(el.checked));
      if (el.exactChecked !== undefined && el.checked !== el.exactChecked) {
        el.checked = el.exactChecked;
      }
    });
  }
  if (el.type === "radio") {
    return on(kind, () => {
      dispatch(kind === "change" ? 1 : 23, el.value);
      settleRadios(el, (r) => r.exactChecked);
    });
  }
  if (kind === "change") {
    return on("change", () => {
      dispatch(textField(el) ? 41 : 1, (textField(el) ? fieldSelection(el) : "") + el.value);
      settleValue(el, true);
    });
  }
  on("input", (e) => {
    const value = el.value;
    if (el.getAttribute("emojiPicker") === "true") {
      if (e.isComposing) {
        return;
      }
      el.value = "";
      const clusters = [...new Intl.Segmenter(undefined, { granularity: "grapheme" }).segment(value)];
      if (clusters.length !== 1 || !(/\p{Emoji_Presentation}/u.test(value) || /[\uFE0F\u20E3]/u.test(value) && /\p{Emoji}/u.test(value))) {
        return;
      }
      return void dispatch(23, value);
    }
    dispatch(sourceField(el) ? 54 : 23, sourceField(el) ? inputPayload(el, e) : value);
    settleValue(el, true);
  });
}

// Capture may move to another element before the physical up. Suppress that
// contact's click wherever it lands. A new down with the same id supersedes it.
let cancelledClick = null;
export function suppressPointerClick(id) {
  if (cancelledClick === null) {
    const clear = () => {
      cancelledClick = null;
      window.removeEventListener("click", click, true);
      window.removeEventListener("pointerdown", down, true);
    };
    const down = e => { if (e.pointerId === cancelledClick) clear(); };
    const click = e => {
      if (!e.isTrusted || !(e.pointerId === cancelledClick || e.pointerId == null && e.detail > 0)) return;
      clear(); e.preventDefault(); e.stopImmediatePropagation();
    };
    window.addEventListener("pointerdown", down, true);
    window.addEventListener("click", click, true);
  }
  cancelledClick = id;
}
