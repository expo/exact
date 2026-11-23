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
 * ({ type, down, target, x, y, mouse, field }), the last primary down, the field it went down in, and the drag in
 * progress from its `dragstart` to its `dragend` ({ press, start, cancelled, source }). `changed` hears each down and
 * each release of a field no longer the last down's. The wasm wait hands its `input` to the listener when it loads. */
export function observeDragInput(root, signal, input = { held: new Map() }, changed = () => {}) {
  const { held } = input, compat = input.compat ??= new Set();
  // A pen's or a finger's compatibility mouse events name no pointer, so pairing them with a press is a heuristic: by
  // point, or else by target, the earliest pending press first. A pair the browser delays or suppresses, or that
  // another script stops before the document, can shift the pairing of those after it. Its `mouseup` follows its release, and preventing its `pointerdown` suppresses both mouse events (Pointer
  // Events 3 §11).
  const compatFor = (event, target) => {
    const up = event.type === 'mouseup';
    const pending = [...compat].filter((press) => press.sawMouseDown === up && (!up || press.release)
      && !press.down.defaultPrevented && !expired(press, event.timeStamp));
    return pending.find((press) => atPoint(press, event) || atPoint(press.release, event))
      ?? pending.find((press) => target != null && (press.target === target || press.release?.target === target));
  };
  const down = (event) => {
    if (event.isTrusted === false) return;
    // A pen's or a finger's earlier press whose release the page missed has no mouse events left on the way: they went
    // with that release. Any button's press of the pointer retires it, so a barrel button's release cannot revive it.
    if (event.type === 'pointerdown' && event.pointerType && event.pointerType !== 'mouse') {
      for (const pending of compat) {
        const same = pending.type === event.pointerType && pending.down.pointerId === event.pointerId;
        if (same && !pending.release) compat.delete(pending);
      }
    }
    if (event.isPrimary === false || event.button > 0) return;
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
    if (!press || held.get(press.type) !== press) return;
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
    // A press a `blur` ended keeps its pending compatibility events until its pointer's release or cancel.
    const type = event.pointerType || 'mouse', heldPress = held.get(type);
    const ofPointer = (press) => press?.down.type === 'pointerdown' && press.down.pointerId === event.pointerId && !press.release;
    const press = ofPointer(heldPress) ? heldPress : [...compat].findLast((pending) => pending.type === type && ofPointer(pending));
    if (!press) return;
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
  // A drag's end ends only the press held where it began, which its release may already have ended, and drops that
  // press's pending compatibility events. Its start leaves them: a later listener can cancel `dragstart`, and a drag
  // that begins sends the pointer `pointercancel` (Pointer Events 3, "Suppressing a pointer event stream"). WebKit can
  // send `dragend` only to a source removed during the drag, so each observer of `input` also listens at the source
  // while the drag is on, one given `input` mid-drag too, as the listener is at the wasm wait's handoff.
  const hearSource = (drag) => {
    if (signal?.aborted) return;
    const onEnd = (event) => event.isTrusted !== false && dragEnded(drag);
    // The listener goes with the drag or with this observer, whichever ends first.
    const forget = () => {
      drag.source.removeEventListener('dragend', onEnd);
      signal?.removeEventListener('abort', forget);
      drag.leave.delete(forget);
    };
    drag.source.addEventListener('dragend', onEnd);
    signal?.addEventListener('abort', forget);
    drag.leave.add(forget);
  };
  const leave = (drag) => {
    for (const forget of [...drag.leave]) forget();
    if (input.drag === drag) input.drag = null;
  };
  const dragEnded = (drag) => {
    leave(drag);
    // A cancelled start began no drag, so a `dragend` WebKit still sends its removed source ends nothing. Whether it
    // was cancelled is read when this observer's zero-delay timer after the start runs, or live for an end WebKit sends
    // before then.
    if (drag.cancelled ?? drag.start.defaultPrevented) return;
    compat.delete(drag.press);
    end(drag.press);
  };
  if (input.drag) hearSource(input.drag);
  root.addEventListener('dragstart', (event) => {
    if (event.isTrusted === false) return;
    if (input.drag) leave(input.drag);
    // The browser's source, inside an open shadow root too, where the event's target is its host: WebKit sends a
    // removed source alone its `dragend`.
    const source = event.composedPath?.()[0] ?? event.target;
    const drag = input.drag = { press: heldAt(input, event) || null, start: event, source, leave: new Set() };
    hearSource(drag);
    // A cancelled start begins no drag, so its source goes once the event's dispatch is over. The flag is read when this
    // zero-delay timer runs, so a script that calls `preventDefault` after dispatch, on a timer that runs earlier, makes
    // a drag that began read as cancelled.
    setTimeout(() => {
      drag.cancelled = event.defaultPrevented;
      if (drag.cancelled) leave(drag);
    });
  }, capture);
  root.addEventListener('dragend', (event) => {
    if (event.isTrusted !== false && input.drag) dragEnded(input.drag);
  }, capture);
  return input;
}
