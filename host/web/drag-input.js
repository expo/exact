export const DRAG_FIELDS = 'input,select,textarea,[contenteditable]:not([contenteditable="false"]),.exact-markdown-editor';

// The primary presses a drag may begin from (LLP 1094.000 D8), which the wasm target's wait (views.js) and the drag-out
// listener (drag-out.js) both follow.

/** Whether `event` is at the point of `down`, within the pixel Chromium rounds a down's to. */
const atPoint = (down, event) => Math.abs(down?.x - event.clientX) <= 1 && Math.abs(down?.y - event.clientY) <= 1;
/** The element an event's `target` is, or the one holding it when it is text. */
export const elementOf = (target) => target?.nodeType === 1 ? target : target?.parentElement;
/** What `down` went down on, when `event` is at its point and `target`, where the browser began the drag, holds it. */
export const pressedAt = (down, event, target) => atPoint(down, event) && target.contains(down.target) ? down.target : null;

/** The down a drag at `target` began from: the last primary down, or a mouse's still held where the drag began, though
 * another input's primary down came since. */
export function downForDrag(input, event, target = elementOf(event.target)) {
  const { last } = input, mouse = input.held.get('mouse');
  if (target && !pressedAt(last, event, target) && pressedAt(mouse, event, target)) return mouse;
  return last;
}

/** Follows the primary presses in `root` until `signal` aborts, into `input`: each pointer type's held press
 * ({ target, x, y, mouse, field }), the last primary down, and the field it went down in. `changed` hears each down and
 * each release of a field no longer the last down's. The wasm wait hands its `input` to the listener when it loads. */
export function observeDragInput(root, signal, input = { held: new Map() }, changed = () => {}) {
  const { held } = input;
  const down = (event) => {
    if (event.isTrusted === false || event.isPrimary === false || event.button > 0) return;
    const target = elementOf(event.target);
    if (!target) return;
    // A `mousedown` alone is a press too: WebKit sends a press after a drag no `pointerdown` (Measured under Playwright).
    if (event.type === 'mousedown') {
      // A press's compatibility `mousedown` after its own `pointerdown` adds nothing, or a pen's or a finger's would read
      // as a mouse's; a tap sends it after the pointer has lifted.
      const { paired } = input;
      input.paired = null;
      const own = paired && (paired.target === target || atPoint(paired, event));
      input.compatUp = own && !paired.mouse;
      if (own) return;
    }
    const type = event.pointerType || 'mouse';
    const press = { target, x: event.clientX, y: event.clientY, mouse: type === 'mouse', field: target.closest(DRAG_FIELDS) };
    if (event.type === 'pointerdown') input.paired = press;
    held.set(type, press);
    input.last = press;
    input.field = press.field;
    changed(input);
  };
  const end = (type) => {
    const press = held.get(type);
    if (!held.delete(type)) return;
    if (press.mouse && input.last === press) input.last = null;
    if (input.dragMouse === press) input.dragMouse = null;
    if (press.field && press.field !== input.field) changed(input);
  };
  const up = (event) => {
    if (event.isTrusted === false || event.isPrimary === false || (event.button > 0 && event.type !== 'pointercancel')) return;
    const type = event.pointerType || 'mouse';
    if (event.type === 'mouseup' && input.compatUp) {
      input.compatUp = false;
      return;
    }
    if (input.paired === held.get(type) && (type === 'mouse' || event.type === 'pointercancel')) input.paired = null;
    end(type);
  };
  const capture = { capture: true, signal };
  for (const type of ['pointerdown', 'mousedown']) root.addEventListener(type, down, capture);
  for (const type of ['pointerup', 'pointercancel', 'mouseup']) root.addEventListener(type, up, capture);
  root.addEventListener('dragstart', (event) => {
    if (event.isTrusted === false) return;
    input.paired = null;
    const mouse = held.get('mouse');
    input.dragMouse = mouse && downForDrag(input, event) === mouse ? mouse : null;
  }, capture);
  root.addEventListener('dragend', (event) => {
    if (event.isTrusted === false) return;
    if (input.dragMouse === held.get('mouse')) end('mouse');
    input.dragMouse = null;
  }, capture);
  return input;
}
