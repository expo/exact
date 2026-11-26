// Zoom uses the intersection of touch-action from the hit to the document.
// Save it at touchdown, before a Contract pointerdown can change the tree.
import { inertAncestor } from './navigation.js';
import { Portals } from './mount.js';
import { onEnd } from './rt.js';

let observers = 0, listening, held, pair, frame = 0;
const fingers = new Map();
let syntheticPacket;

// A scripted sender dispatches the contacts of one sample synchronously.
// Its event timestamps measure JS construction time, not the hardware sample.
function scriptedPacket() {
  if (!syntheticPacket) {
    syntheticPacket = { time: performance.now() };
    queueMicrotask(() => { syntheticPacket = null; });
  }
  return syntheticPacket;
}

function down(event) {
  if (event.pointerType !== 'touch' || pair || inertAncestor(event.target)) return;
  const path = event.composedPath();
  const owners = [];
  for (const node of Portals?.path(event) ?? path) {
    if (node.$pinch && !node.matches(':disabled,[disabled]')) owners.push(node);
    if (node.matches?.('input,textarea,select,button,[contenteditable="true"]')) break;
  }
  if (!owners.length || !path.some(node => {
    if (node.nodeType !== 1) return false;
    const style = getComputedStyle(node);
    if (style.display === 'inline' && !node.matches('img,video,canvas,iframe,embed,object,input,select,textarea,button,svg')) return false;
    if (/^table-(row|row-group|header-group|footer-group|column|column-group)$/.test(style.display)) return false;
    const value = style.touchAction;
    return value !== 'auto' && value !== 'manipulation' && !value.split(' ').includes('pinch-zoom');
  })) return;
  if (!fingers.size) {
    held = new AbortController();
    const options = { capture: true, passive: true, signal: held.signal };
    window.addEventListener('pointermove', move, options);
    window.addEventListener('pointerup', finish, options);
    window.addEventListener('pointercancel', finish, options);
    window.addEventListener('blur', blur, { signal: held.signal });
  }
  const finger = { owners, x: event.clientX, y: event.clientY };
  for (const other of fingers.values()) {
    const node = owners.find(owner => owner.$pinch && other.owners.includes(owner));
    const span = Math.hypot(other.x - finger.x, other.y - finger.y);
    if (node && span > 0 && Number.isFinite(span)) {
      pair = { node, a: other, b: finger, span, moves: [], sample: null, sent: null };
      break;
    }
  }
  fingers.set(event.pointerId, finger);
}

function move(event) {
  const finger = fingers.get(event.pointerId);
  if (!finger) return;
  if (finger !== pair?.a && finger !== pair?.b) {
    finger.x = event.clientX;
    finger.y = event.clientY;
    return;
  }
  const history = event.getCoalescedEvents?.();
  const packet = !event.isTrusted && !history?.length ? scriptedPacket() : null;
  for (const point of history?.length ? history : [event]) {
    pair.moves.push({ finger, x: point.clientX, y: point.clientY,
      time: packet?.time ?? point.timeStamp, packet: packet ?? point.timeStamp });
  }
  if (!frame) frame = requestAnimationFrame(flush);
}

function sample() {
  const { a, b } = pair;
  const distance = Math.hypot(a.x - b.x, a.y - b.y);
  if (!pair.sample && Math.abs(distance - pair.span) < 4) return;
  const scale = distance / pair.span;
  const factor = scale > 0 && Number.isFinite(scale) ? scale : pair.sample?.[0] ?? 1;
  pair.sample = [factor, (a.x + b.x) / 2, (a.y + b.y) / 2];
}

function flush() {
  if (!frame) return;
  cancelAnimationFrame(frame);
  frame = 0;
  if (!pair?.node.isConnected) return;
  // Coalesced moves arrive per pointer. Rejoin the contacts by hardware
  // timestamp so translation keeps its span and a short excursion counts.
  let previous;
  for (const move of pair.moves.sort((a, b) => a.time - b.time)) {
    if (previous && (previous.packet !== move.packet || previous.finger === move.finger)) sample();
    move.finger.x = move.x;
    move.finger.y = move.y;
    previous = move;
  }
  pair.moves.length = 0;
  sample();
  const next = pair.sample;
  if (!next || pair.sent?.every((v, i) => v === next[i])) return;
  pair.sent = next;
  pair.node.$pinch?.move?.(...next);
}

function finish(event) {
  const finger = fingers.get(event.pointerId);
  if (!finger) return;
  const last = pair;
  const enrolled = finger === last?.a || finger === last?.b;
  const cancelled = event.type !== 'pointerup';
  if (enrolled) {
    if (!cancelled) move(event);
    flush();
    pair = null;
  }
  fingers.delete(event.pointerId);
  if (!fingers.size) held.abort();
  if (enrolled && last.node.isConnected && last.sample) {
    const [scale, x, y] = last.sample;
    last.node.$pinch?.release?.(cancelled ? 1 : scale, x, y);
  }
}

function blur() {
  for (const pointerId of fingers.keys()) finish({pointerId, type: 'pointercancel'});
}

function observe(element) {
  if (!observers++) {
    listening = new AbortController();
    window.addEventListener('pointerdown', down, { capture: true, passive: true, signal: listening.signal });
  }
  onEnd(() => {
    delete element.$pinch;
    if (pair?.node === element) pair = null;
    if (!--observers) {
      listening.abort(); held?.abort();
      cancelAnimationFrame(frame); frame = 0; fingers.clear();
    }
  });
  return {};
}

export function onPinch(element, action) {
  if (typeof window !== 'undefined') (element.$pinch ??= observe(element)).move = action;
}

export function onPinchRelease(element, action) {
  if (typeof window !== 'undefined') (element.$pinch ??= observe(element)).release = action;
}
