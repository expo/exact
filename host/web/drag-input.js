export const DRAG_FIELDS = 'input,select,textarea,[contenteditable]:not([contenteditable="false"]),.exact-markdown-editor';

// The primary presses a drag may begin from (LLP 1094.000 D8), which the wasm target's wait (views.js) and the drag-out
// listener (drag-out.js) both follow.

/** Whether `event` is at the point of `down`, within the pixel Chromium rounds a down's to. */
const atPoint = (down, event) => Math.abs(down?.x - event.clientX) <= 1 && Math.abs(down?.y - event.clientY) <= 1;
/** The element an event's `target` is, or the one holding it when it is text. */
export const elementOf = (target) => target?.nodeType === 1 ? target : target?.parentElement;
/** What `down` went down on, when `event` is at its point and `target`, where the browser began the drag, holds it. */
export const pressedAt = (down, event, target) => atPoint(down, event) && target.contains(down.target) ? down.target : null;

/** The most recent press still held where a drag at `target` began. */
const heldAt = (input, event, target = elementOf(event.target)) =>
  target && [...input.held.values()].findLast((press) => pressedAt(press, event, target));

/** The press a drag at `target` began from: the most recent one still held there, falling back to the last primary
 * down. */
export function downForDrag(input, event, target = elementOf(event.target)) {
  return heldAt(input, event, target) || input.last;
}

/** How long after a pen's or a finger's down or release its compatibility mouse event may come; a tap's come together
 * after it lifts, a long press's after its release. */
const COMPAT_MS = 500;
const expired = (press, now) => now - (press.release?.timeStamp ?? press.down.timeStamp) > COMPAT_MS;
const matchesMouseEvent = (point, event, target) => point && ((target != null && point.target === target) || atPoint(point, event));

/** Follows the primary presses in `root` until `signal` aborts, into `input`: each pointer type's held press
 * ({ type, down, target, x, y, mouse, field }), the last primary down, the field it went down in, and the press a drag
 * began from, which its `dragend` ends. `changed` hears each down and
 * each release of a field no longer the last down's. The wasm wait hands its `input` to the listener when it loads. */
export function observeDragInput(root, signal, input = { held: new Map() }, changed = () => {}) {
  const { held } = input, compat = input.compat ??= new Set();
  // A pen's or a finger's compatibility mouse events pair with that press by target or point. Its `mouseup` follows
  // its release, and preventing its `pointerdown` suppresses both mouse events (Pointer Events 3 §11).
  const compatFor = (event, target) => {
    const up = event.type === 'mouseup';
    return [...compat].findLast((press) =>
      press.sawMouseDown === up && (!up || press.release) && !press.down.defaultPrevented
      && !expired(press, event.timeStamp)
      && (matchesMouseEvent(press, event, target) || matchesMouseEvent(press.release, event, target)));
  };
  const down = (event) => {
    if (event.isTrusted === false || event.isPrimary === false || event.button > 0) return;
    const target = elementOf(event.target);
    if (!target) return;
    // WebKit can send a standalone `mousedown` after a drag (Measured under Playwright).
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
    // Keep the most recent down last, including when a new press replaces the same pointer type.
    held.delete(type);
    held.set(type, press);
    if (!press.mouse) {
      // A held press can still get a fresh window at release. Earlier taps can still have mouse events on the way.
      for (const pending of compat) {
        if (held.get(pending.type) !== pending && expired(pending, event.timeStamp)) compat.delete(pending);
      }
      compat.add(press);
    }
    input.last = press;
    input.field = press.field;
    changed(input);
  };
  const end = (press) => {
    if (!press) return;
    if (input.dragPress === press) input.dragPress = null;
    if (held.get(press.type) !== press) return;
    held.delete(press.type);
    if (press.mouse && input.last === press) input.last = null;
    if (press.field && press.field !== input.field) changed(input);
  };
  const up = (event) => {
    // A chord's last `pointerup` names whichever button was released last, and ends the press with no button left down.
    const last = event.type === 'pointercancel' || (event.type === 'pointerup' && event.buttons === 0);
    if (event.isTrusted === false || event.isPrimary === false || (event.button > 0 && !last)) return;
    if (event.type === 'mouseup') {
      const press = compatFor(event, elementOf(event.target));
      if (press) compat.delete(press);
      else {
        const mouse = held.get('mouse');
        if (mouse?.down.type === 'mousedown') end(mouse);
      }
      return;
    }
    const type = event.pointerType || 'mouse';
    const press = held.get(type);
    if (press?.down.type !== 'pointerdown' || press.down.pointerId !== event.pointerId) return;
    if (event.type === 'pointercancel') compat.delete(press);
    else if (compat.has(press)) {
      press.release = { target: elementOf(event.target), x: event.clientX, y: event.clientY, timeStamp: event.timeStamp };
    }
    end(press);
  };
  const capture = { capture: true, signal };
  for (const type of ['pointerdown', 'mousedown']) root.addEventListener(type, down, capture);
  for (const type of ['pointerup', 'pointercancel', 'mouseup']) root.addEventListener(type, up, capture);
  // Releasing the primary button while another is held sends a move (Pointer Events 3, "Chorded button interactions").
  // The page can miss a mouse release outside the window. The last down's field still keeps its protection.
  root.addEventListener('pointermove', (event) => {
    if (event.isTrusted !== false && event.pointerType === 'mouse' && (event.buttons & 1) === 0) end(held.get('mouse'));
  }, capture);
  // A window that loses focus may miss a held press's release, so it ends them all; the last down's field keeps its
  // protection. A script's `blur` is no release.
  root.defaultView?.addEventListener('blur', (event) => {
    if (event.isTrusted !== false) for (const press of [...held.values()]) end(press);
  }, { signal });
  // A drag's end ends only a press held where it began, and that press's pending compatibility events. Its start
  // leaves them: a later listener can cancel `dragstart`, and a drag that begins sends the pointer `pointercancel`
  // (Pointer Events 3, "Suppressing a pointer event stream"). WebKit can send `dragend` only to a source removed during
  // the drag, so the source hears it too.
  let leaveSource = null;
  const dragEnded = (press) => {
    compat.delete(press);
    end(press);
    leaveSource?.();
    leaveSource = null;
  };
  root.addEventListener('dragstart', (event) => {
    if (event.isTrusted === false) return;
    leaveSource?.();
    const press = input.dragPress = heldAt(input, event) || null, source = event.target;
    const onEnd = (ended) => ended.isTrusted !== false && dragEnded(press);
    source.addEventListener('dragend', onEnd);
    leaveSource = () => source.removeEventListener('dragend', onEnd);
  }, capture);
  root.addEventListener('dragend', (event) => {
    if (event.isTrusted !== false) dragEnded(input.dragPress);
  }, capture);
  return input;
}
