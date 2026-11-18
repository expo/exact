/** What the person's hands hold in this page (LLP 1029.000.002 D7, D9), for both web targets: `"composition"`,
 * `"pointer"`, `"gesture"` or null. It hears the window's capture phase, so no handler's `stopPropagation` hides an
 * event. `changed` runs after each event that may begin or end a hold, with that event, and with none after a
 * change to the document. It names nothing outside itself: the JS target's dev page is given its source
 * (host/web-js/dev.mjs). */
export function engagement(changed) {
  const composing = new Set(), pointers = new Set(), root = document.documentElement;
  let touches = 0, dragging = false, pinching = false;
  const prune = () => {
    for (const el of composing) if (!el.isConnected) composing.delete(el);
    if (!composing.size) removed.disconnect();
  };
  // A composing field that an update removes sends no `compositionend`: while one composes, a change to the
  // document asks again.
  const removed = new MutationObserver(() => { prune(); changed(); });
  // A row the keys lifted holds no pointer (LLP 1094 D9): the reorder's glue marks the document from the lift to
  // the drop or the cancel (group-glue.js).
  new MutationObserver(() => changed()).observe(root, { attributes: true, attributeFilter: ["data-exact-reorder-keys"] });
  const heard = { capture: true, passive: true }, hears = {};
  const on = (type, change) => addEventListener(type, hears[type] = (e) => { change(e); changed(e); }, heard);
  // The event that ends an interaction goes to the element it began on, and to no ancestor once that element has
  // left the document (a touch's, https://w3c.github.io/touch-events/#event-touchend; a drag's, at its source
  // node, https://html.spec.whatwg.org/multipage/dnd.html#dndevents). So each ending is heard on that element as
  // well as on the window; one that reaches both changes nothing the second time.
  const until = (e, ...types) => { for (const type of types) e.composedPath()[0].addEventListener(type, hears[type], heard); };
  on("compositionstart", (e) => { composing.add(e.target); removed.observe(document, { childList: true, subtree: true }); });
  on("compositionend", (e) => { composing.delete(e.target); prune(); });
  // No pointer goes down, lifts or moves unpressed during a native drag or a trackpad pinch: one that does says
  // the gesture is over.
  const over = () => { dragging = pinching = false; };
  on("pointerdown", (e) => { over(); pointers.add(e.pointerId); until(e, "pointerup", "pointercancel"); });
  on("pointerup", (e) => { over(); pointers.delete(e.pointerId); });
  // Touch scrolling cancels a finger's pointer while the finger stays down (Pointer Events,
  // https://www.w3.org/TR/pointerevents3/#the-pointercancel-event): the touch count holds it until it lifts.
  on("pointercancel", (e) => pointers.delete(e.pointerId));
  const count = (e) => touches = e.touches.length;
  on("touchstart", (e) => { count(e); until(e, "touchend", "touchcancel"); });
  for (const type of ["touchend", "touchcancel"]) on(type, count);
  // A release the page did not hear, outside the window or while it had no focus, ends the press at the next move,
  // whose buttons say none is down. The window's blur ends nothing: the button or the finger is still down.
  on("pointermove", (e) => { if (!e.buttons) { over(); pointers.delete(e.pointerId); } });
  // A native drag cancels its pointer and ends with `dragend` or `drop`. One a handler cancels never begins, and
  // the browser sends neither: the event says so once every handler has had it.
  on("dragstart", (e) => {
    dragging = true;
    until(e, "dragend");
    setTimeout(() => { if (e.defaultPrevented) { dragging = false; changed(); } });
  });
  for (const type of ["dragend", "drop"]) on(type, () => dragging = false);
  // Safari brackets a trackpad pinch or rotation, which presses no pointer, with events of its own
  // (https://developer.apple.com/documentation/webkitjs/gestureevent). Other browsers send none.
  on("gesturestart", (e) => { pinching = true; until(e, "gestureend"); });
  on("gestureend", () => pinching = false);
  // A page that leaves, into the back-forward cache too, hears no release.
  on("pagehide", () => { over(); composing.clear(); pointers.clear(); touches = 0; prune(); });
  return () => {
    prune();
    return composing.size ? "composition" : pointers.size || touches ? "pointer"
      : dragging || pinching || root.hasAttribute("data-exact-reorder-keys") ? "gesture" : null;
  };
}
