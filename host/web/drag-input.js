export const DRAG_FIELDS = 'input,select,textarea,[contenteditable]:not([contenteditable="false"]),.exact-markdown-editor';

// The primary presses a drag may begin from (LLP 1094.000 D8), which the wasm target's wait (views.js) and the drag-out
// listener (drag-out.js) both follow.

/** Whether `event` is at the point of `down`, within the pixel Chromium rounds a down's to. */
const atPoint = (down, event) => Math.abs(down?.x - event.clientX) <= 1 && Math.abs(down?.y - event.clientY) <= 1;
/** The element an event's `target` is, or the one holding it when it is text. */
export const elementOf = (target) => target?.nodeType === 1 ? target : target?.parentElement;
/** What `down` went down on, when `event` is at its point and `target`, where the browser began the drag, holds it. */
export const pressedAt = (down, event, target) => atPoint(down, event) && target.contains(down.target) ? down.target : null;

/** The most recent press still held where a drag at `target` began, falling back to the last primary down. */
export function downForDrag(input, event, target = elementOf(event.target)) {
  const held = target && [...input.held.values()].findLast((press) => pressedAt(press, event, target));
  return held || input.last;
}

/** How long after a pen's or a finger's down or release its compatibility mouse event may come; a tap's come together
 * after it lifts, a long press's after its release. */
const COMPAT_MS = 500;
const matchesMouseEvent = (point, event, target) => point && (point.target === target || atPoint(point, event));

/** Follows the primary presses in `root` until `signal` aborts, into `input`: each pointer type's held press
 * ({ type, down, target, x, y, mouse, field }), the last primary down, the field it went down in, and the press a drag
 * began from, which its `dragend` ends. `changed` hears each down and
 * each release of a field no longer the last down's. The wasm wait hands its `input` to the listener when it loads. */
export function observeDragInput(root, signal, input = { held: new Map() }, changed = () => {}) {
  const { held } = input, compat = input.compat ??= new Map();
  // A pen's or a finger's compatibility `mousedown` and `mouseup` are its own, matched by target or point, and never
  // a mouse's press or release (Pointer Events 3 §11). A down whose default was prevented sends neither.
  const compatFor = (event, target) => [...compat.values()].findLast((press) =>
    press.sawMouseDown === (event.type === 'mouseup') && !press.down.defaultPrevented
    && !(event.timeStamp - (press.release?.timeStamp ?? press.down.timeStamp) > COMPAT_MS)
    && (matchesMouseEvent(press, event, target) || matchesMouseEvent(press.release, event, target)));
  const forgetCompat = (press) => {
    if (press && compat.get(press.type) === press) compat.delete(press.type);
  };
  const down = (event) => {
    if (event.isTrusted === false || event.isPrimary === false || event.button > 0) return;
    const target = elementOf(event.target);
    if (!target) return;
    // A `mousedown` alone is a press too: WebKit sends a press after a drag no `pointerdown` (Measured under Playwright).
    if (event.type === 'mousedown') {
      const mouse = held.get('mouse');
      if (mouse?.down.type === 'pointerdown' && !mouse.sawMouseDown && !mouse.down.defaultPrevented
          && matchesMouseEvent(mouse, event, target)) {
        mouse.sawMouseDown = true;
        return;
      }
      const press = compatFor(event, target);
      if (press) press.sawMouseDown = true;
      if (press || mouse) return;
    }
    const type = event.type === 'mousedown' ? 'mouse' : event.pointerType || 'mouse';
    const press = { type, down: event, target, x: event.clientX, y: event.clientY, mouse: type === 'mouse',
      field: target.closest(DRAG_FIELDS), sawMouseDown: event.type === 'mousedown' };
    if (!press.mouse) {
      compat.delete(type);
      compat.set(type, press);
    }
    // Keep the most recent down last, including when a new press replaces the same pointer type.
    held.delete(type);
    held.set(type, press);
    input.last = press;
    input.field = press.field;
    changed(input);
  };
  const end = (press) => {
    if (!press || held.get(press.type) !== press) return;
    held.delete(press.type);
    if (press.mouse && input.last === press) input.last = null;
    if (press.field && press.field !== input.field) changed(input);
  };
  const up = (event) => {
    if (event.isTrusted === false || event.isPrimary === false || (event.button > 0 && event.type !== 'pointercancel')) return;
    if (event.type === 'mouseup') {
      const press = compatFor(event, elementOf(event.target));
      if (press) forgetCompat(press);
      else {
        const mouse = held.get('mouse');
        if (mouse?.down.type === 'mousedown') end(mouse);
      }
      return;
    }
    const type = event.pointerType || 'mouse';
    const press = held.get(type);
    if (press?.down.type !== 'pointerdown' || press.down.pointerId !== event.pointerId) return;
    if (event.type === 'pointercancel') forgetCompat(press);
    else if (compat.get(type) === press) {
      press.release = { target: elementOf(event.target), x: event.clientX, y: event.clientY, timeStamp: event.timeStamp };
    }
    end(press);
  };
  const capture = { capture: true, signal };
  for (const type of ['pointerdown', 'mousedown']) root.addEventListener(type, down, capture);
  for (const type of ['pointerup', 'pointercancel', 'mouseup']) root.addEventListener(type, up, capture);
  // A mouse released outside the window can miss its up. The last down's field still keeps its protection.
  root.addEventListener('pointermove', (event) => {
    if (event.isTrusted !== false && event.pointerType === 'mouse' && event.buttons === 0) end(held.get('mouse'));
  }, capture);
  root.defaultView?.addEventListener('blur', () => end(held.get('mouse')), { signal });
  root.addEventListener('dragstart', (event) => {
    if (event.isTrusted === false) return;
    input.dragPress = downForDrag(input, event);
    forgetCompat(input.dragPress);
  }, capture);
  root.addEventListener('dragend', (event) => {
    if (event.isTrusted === false) return;
    end(input.dragPress);
    input.dragPress = null;
  }, capture);
  return input;
}
