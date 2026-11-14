// The event binders (LLP 1005 §Events): `on`, and each family's binder, passed to it by the generated module
// only where its plan binds that family.
import { imageEvent } from './elements.js';
import { pointer, record } from './pointer.js';
import { press } from './focus.js';
import {
  settleRadios,
  sourceField,
  textField,
  onSelection,
  editMenu,
  inertAncestor,
  guestMessageAuthorized,
  scrollDrag,
  contextAt,
  liveById,
} from './navigation.js';
import { Owner, onEnd, Media, inflight, Sounds, Flushing, FocusQueue } from './rt.js';
import { clock, advance } from './clock.js';
import { Mutations } from './resources.js';
import { start } from './mount.js';
import { input } from './pieces.js';
// `Propagating`: the pointer's down or up a handler is running for, which only its `stopPropagation()` reaches (its
// default is the browser's); `KeyEvent`: the key, focus, wheel, beforeunload or clipboard event whose handler is
// running (`on`), whose `preventDefault()` and `stopPropagation()` commands run before its commit returns (rt.js
// `Hosts`)
export let KeyEvent = null,
  Propagating = null;
/** A loaded piece's own handling of an event (files.js's file input): true when handled. */
export const OnHooks = {};
export function on(e, kind, f, bind) {
  // The runner's dispatch_at fires what is due at the event's time first (a focus's `then` before the input); a refusal
  // still lets the event run.
  const ends = kind === 'close' || kind === 'scrollend',
    go = f,
    wall = !clock.agent,
    own = ends ? Owner : null,
    pointer = kind === 'pointerdown' || kind === 'pointerup',
    // another event's handler run inside a pointer handler (a focus it moved) is not that pointer event's: its
    // `stopPropagation()` does not stop it
    run = (...a) => {
      if (pointer) return go(...a);
      const outer = Propagating;
      Propagating = null;
      try {
        return go(...a);
      } finally {
        Propagating = outer;
      }
    };
  f = (...a) => {
    const to = wall && start ? Math.max(clock.now, performance.now() - start) : clock.now;
    // A change action's receipt boundary yields (true): what is due after it runs before the event too.
    if (clock.timers.some(t => t.due <= to && !(wall && t.frame)) || Mutations.some(m => m.due <= to || m.next <= to))
      while (advance(to, wall) === true);
    if (!ends || (e.isConnected && !own?.gone)) return run(...a);
  };
  // A queued dialog close (or a scroll's end a due task's removal outlived) can outlive its arm, even while an exit
  // animation keeps the element connected.
  const l = (t, g, capture = false) => {
    e.addEventListener(t, g, capture);
    if (ends) onEnd(() => e.removeEventListener(t, g, capture));
  };
  if (OnHooks.file && e.localName === 'input' && e.type === 'file' && OnHooks.file(e, kind, f)) return;
  // media.js: the glue's reports
  if (e.$media && Media.MEDIA_EVENTS.has(kind)) return Media.mediaOn(e, kind, f);
  // A module view hears its module's events, and the page's own input as any element does (glue.js `attach`): a click
  // is its press.
  if (e.exactNative) {
    l('exact-native', ev => {
      if (ev.detail.kind === kind) f(...(ev.detail.value == null ? [] : [ev.detail.value]));
    });
    if (kind === 'message') return;
  }
  if (e.localName === 'iframe' && kind === 'error') return l(kind, ev => f(ev.detail ?? 'iframe did not load'));
  // picture-in-picture.js: the failure's reason
  return bind ? bind(e, kind, f, l) : l(kind, kind === 'pictureinpictureerror' ? ev => f(ev.detail) : () => f());
}
// Each event family's binder, passed to `on` by the generated module only where its plan binds that family
// (emit.rs `binder`), so a plan carries only the families it hears; any other event is a plain listener.
// An image's `load` and `error` (elements.js `imageEvent`); another element's are plain listeners.
export const onImage = (e, kind, f, l) => (e.localName === 'img' ? imageEvent(e, kind, f, inflight) : l(kind, () => f()));
// A link with a press is the app's navigation: the browser's is prevented. A modified or other-button click, a `target`
// or `download`, is the browser's alone and the press does not run, with a router or without (`router`, input-glue.js).
// a press action taking one more parameter hears the MouseEvent's modifiers (gallery F20) and point (LLP 1021 D2)
export const onPress = (e, kind, f, l) => {
  // the input piece presses it by key (input-glue.js `pressesByKey`)
  if (!e.matches('button, a[href], input, select, textarea, summary')) input();
  return l('click', ev => {
    const a = ev.target.closest?.('a[href]');
    if (a && a !== e && e.contains(a)) return;
    // a disabled box on the way presses nothing, as on iOS (PointerIOS.swift `activationTarget`)
    for (let n = ev.target; n && n !== e.parentNode; n = n.parentNode)
      if (n.nodeType === 1 && n.hasAttribute('disabled')) {
        ev.stopPropagation();
        return;
      }
    if (
      e.localName === 'a' &&
      (ev.button ||
        ev.metaKey ||
        ev.ctrlKey ||
        ev.shiftKey ||
        ev.altKey ||
        (e.target && e.target !== '_self') ||
        e.hasAttribute('download'))
    )
      return;
    ev.stopPropagation();
    if (e.localName === 'a') ev.preventDefault();
    const done = ev.detail > 0 ? press(e) : null;
    try {
      f([ev.shiftKey, ev.ctrlKey, ev.altKey, ev.metaKey, ev.clientX, ev.clientY]);
    } finally {
      done?.();
    }
  });
};
// A checkbox's value is whether it is checked, a radio's its `value`; the platform moves the control at once, and an
// action that refuses snaps the box or the radio group back (glue.js, navigation.js). A host's change carries its own
// text (files.js: a picker's lines, which an input's value would flatten). A range's is a number (the events table). An
// action taking one more parameter hears the `InputEvent` (x2apps codeedit #2, survey #2).
export const onValue = (e, kind, f, l) => {
  return l(kind, ev => {
    if (ev instanceof CustomEvent) return f(ev.detail);
    const box = e.type === 'checkbox',
      radio = e.type === 'radio',
      v = box ? e.checked : e.type === 'range' ? Number(e.value) : e.value;
    f(v, inputRecord(e, box || radio ? e.value : String(v), box ? e.checked : radio, ev));
    if (box && e.$checked !== undefined && e.checked !== e.$checked) e.checked = e.$checked;
    if (radio) settleRadios(e, r => r.$checked);
  });
};
export const onBeforeInput = (e, kind, f, l) =>
  l(kind, ev => {
    const outer = KeyEvent;
    KeyEvent = ev;
    try {
      f(inputRecord(e, e.value, false, ev));
    } finally {
      KeyEvent = outer;
    }
  });
export const onHover = (e, kind, f, l) => {
  l('pointerenter', () => f(true));
  return l('pointerleave', () => f(false));
};
export const onFocusWithin = (e, kind, f, l) =>
  l(kind, ev => {
    if (ev.$stopped) return;
    const outer = KeyEvent;
    KeyEvent = ev;
    try {
      f([ev.target?.exactExitId ?? ev.target?.id ?? '', ev.relatedTarget?.exactExitId ?? ev.relatedTarget?.id ?? null]);
    } finally {
      KeyEvent = outer;
    }
  });
// `keycapture` hears keydown outermost first, then `key` bubbles; `keyup` bubbles too (#140). An action taking one more
// parameter hears the KeyboardEvent record (contract/types selection.rs's order: code, repeat and isComposing last; a
// keyCode-229 keydown, one an input method processed, is composing, as web code takes it: WebKit's Enter that commits a
// composition comes after `compositionend`, isComposing false). With nothing focused a key or a clipboard event targets
// `<body>`, outside the app, where the DOM's `window` listeners hear it: the first root's handlers hear any event whose
// target is `<body>` or `<html>` (`atBody`; never a copy a flow forwards to a detached node), on `<html>` as it
// bubbles, so after the shortcuts (input-glue's capture listener) and before the document's defaults (a sheet's Escape,
// a list's ⌘A), its `keycapture` before its `key`.
let BodyHeard;
const atBody = (e, kind, g) => {
  (e.$body ??= {})[kind] = g;
  if (!globalThis.document?.documentElement) return g;
  BodyHeard ??= ['keydown', 'keyup', 'copy', 'cut', 'paste'].map(t =>
    document.documentElement.addEventListener(t, ev => {
      const root = document.getElementById('exact-root'),
        h = root?.firstElementChild?.$body;
      if (!h || (ev.target !== document.body && ev.target !== document.documentElement)) return;
      if (t === 'keydown') {
        h.keycapture?.(ev);
        h.key?.(ev);
      } else h[t]?.(ev);
    }),
  );
  return g;
};
export const onKey = (e, kind, f, l) => {
  const type = kind === 'keyup' ? 'keyup' : 'keydown';
  if (!(e.exactFlowEvents ??= []).includes(type)) e.exactFlowEvents.push(type);
  return l(
    type,
    atBody(e, kind, ev => {
      if (ev.$stopped || (kind === 'keycapture' && ev.exactFlowCapture && !ev.exactFlowCapture.includes(e))) return;
      const outer = KeyEvent;
      KeyEvent = ev;
      try {
        f(ev.key, [
          ev.key,
          ev.shiftKey,
          ev.ctrlKey,
          ev.altKey,
          ev.metaKey,
          ev.code,
          ev.repeat,
          !!(ev.isComposing || ev.keyCode === 229 || ev.key === 'Process'),
        ]);
      } finally {
        KeyEvent = outer;
      }
    }),
    kind === 'keycapture',
  );
};
// a MIDI input's message (LLP 1125 D8), filtered by `midiinputs` in midi.js
export function onMIDI(e, kind, f) {
  const pair = [e, f],
    listeners = (Sounds.midi ??= new Set());
  listeners.add(pair);
  onEnd(() => listeners.delete(pair));
}
function onWindow(kind, listener) {
  addEventListener(kind, listener);
  onEnd(() => removeEventListener(kind, listener));
}
// The window's, heard by every connected element that declares it (studio diary R17).
export const onUnload = (e, kind, f, l) => {
  return onWindow('beforeunload', ev => {
    if (!e.isConnected) return;
    const outer = KeyEvent;
    KeyEvent = ev;
    try {
      f();
    } finally {
      KeyEvent = outer;
    }
  });
};
// Enter's default: after every `key` handler on the path (the window's listener is last), unless one prevented it, and
// after the browser's own default, HTML's `change` on Enter (gallery F26); the field's next key or edit (before it
// applies; an Enter from a textarea or an editor edits itself) runs it first, so the action reads the text Enter
// submitted (r27 t2: typing at once after Enter submitted the next text)
export const onSubmit = (e, kind, f, l) => {
  return l('keydown', ev => {
    if (ev.key === 'Enter' && !ev.isComposing && ev.keyCode !== 229 && !ev.$submit) {
      ev.$submit = true;
      addEventListener(
        'keydown',
        w => {
          if (w !== ev || ev.defaultPrevented) return;
          const run = () => {
            if (run.done) return;
            run.done = true;
            e.removeEventListener('keydown', run, true);
            e.removeEventListener('beforeinput', run, true);
            f();
          };
          e.addEventListener('keydown', run, true);
          if (ev.target.localName !== 'textarea' && !ev.target.isContentEditable)
            e.addEventListener('beforeinput', run, true);
          setTimeout(run);
        },
        { once: true },
      );
    }
  });
};
// A package view's declared event (LLP 1133.021 §9 item 2): a message `{"exact:event": name, "args": […]}` runs `f`
// with the arguments as the runtime holds values of `types` (facts.rs `surface_type_json`): a record by position, a
// field left out none.
const fromJSON = (j, t) =>
  typeof t === 'string'
    ? j
    : Array.isArray(t)
      ? t[0] === '?'
        ? j == null
          ? null
          : fromJSON(j, t[1])
        : t[0] === 'e'
          ? j
          : (Array.isArray(j) ? j : []).map(x => fromJSON(x, t[1]))
      : Object.entries(t).map(([f, u]) => fromJSON(j?.[f] ?? null, u));
export const viewEvent = (name, types, f) => text => {
  let m;
  try {
    m = JSON.parse(text);
  } catch {
    return;
  }
  if (m?.['exact:event'] === name) f(...types.map((t, i) => fromJSON(m.args?.[i] ?? null, t)));
};
// Only from the origin of the src the app committed (glue.js `guestMessageAuthorized`, LLP 1020 D2): a guest that
// navigated away is not heard; an opaque sandbox's origin is "null".
export const onMessage = (e, kind, f, l) => {
  // a GPU canvas's surface (x.message)
  if (e.localName !== 'iframe')
    return l('exact-native', ev => {
      if (ev.detail?.kind === 'message') f(ev.detail.value);
    });
  return onWindow('message', ev => {
    if (ev.source === e.contentWindow && guestMessageAuthorized(e, ev.origin))
      f(typeof ev.data === 'string' ? ev.data : JSON.stringify(ev.data));
  });
};
// The port's offsets, as the web host sends them (`glue.js` `attach`); an action taking one more parameter hears the
// `ScrollEvent` record. The boot's restored offset ends no reader's scroll, unless a smooth restore went on to
// deliver scrolls.
export const onScroll = (e, kind, f, l) => {
  return l(kind, () => {
    if (e.$bootScroll) {
      e.$bootScroll = false;
      return;
    }
    e.$bootScrollEnd = false;
    f(e.scrollLeft, e.scrollTop, [
      e.scrollLeft,
      e.scrollTop,
      e.scrollWidth,
      e.scrollHeight,
      e.clientWidth,
      e.clientHeight,
      ...(e.$list?.scrollAnchor() ?? [null, 0]),
    ]);
  });
};
export const onScrollend = (e, kind, f, l) => {
  return l(kind, () => {
    if (e.$bootScrollEnd) {
      e.$bootScrollEnd = false;
      return;
    }
    f();
  });
};
// A scroll drag's start and release, one tracker for both (navigation.js `scrollDrag`; Charlie, 2026-11-10): the
// release's `ScrollDragEvent` is its velocity and no target, which a browser never says.
export const onScrollDrag = (e, kind, f, l) => {
  if (!e.$scrollDrag) {
    e.$scrollDrag = {};
    scrollDrag(
      e,
      l,
      () => e.$scrollDrag.scrolldragstart?.(),
      (vx, vy) => e.$scrollDrag.scrolldragend?.([vx, vy, null, null]),
    );
  }
  e.$scrollDrag[kind] = f;
};
// Pull to refresh is a native port's; the web has none (`glue.js` attaches nothing).
export const onRefresh = () => {};
export const onDblclick = (e, kind, f, l) => {
  return l(kind, ev => {
    ev.preventDefault();
    f();
  });
};
// pointer.js (LLP 1005 §Events, 1056 §3)
export const onPointer = (e, kind, f, l) => {
  return pointer(
    e,
    kind,
    kind === 'pointermove'
      ? f
      : (r, ev) => {
          const outer = Propagating;
          Propagating = ev;
          try {
            f(r);
          } finally {
            Propagating = outer;
          }
        },
  );
};
// UI Events' `contextmenu` is a PointerEvent: where the secondary click was (studio diary R22).
// A text field's own edit menu stays the browser's (navigation.js `editMenu`); a control's input has none, so its
// node's handler runs (#487). The nearest handler alone hears it, as on the wasm host and macOS (review b5-b 3).
export const onContextmenu = (e, kind, f, l) => {
  return l(kind, ev => {
    if (editMenu(ev.target)) return;
    ev.preventDefault();
    ev.stopPropagation();
    f(record(e, ev));
  });
};
// DOM's own, bubbling to every ancestor's handler; one that calls `preventDefault()` keeps the scroll (a pinch is a
// Control-held wheel) from happening (studio diary R3).
export const onWheel = (e, kind, f, l) => {
  return e.addEventListener(
    'wheel',
    ev => {
      if (ev.$stopped || e.matches(':disabled') || inertAncestor(e)) return;
      const outer = KeyEvent;
      KeyEvent = ev;
      try {
        f([
          ...record(e, ev).slice(0, 2),
          ev.deltaX,
          ev.deltaY,
          ev.deltaMode,
          ev.shiftKey,
          ev.ctrlKey,
          ev.altKey,
          ev.metaKey,
        ]);
      } finally {
        KeyEvent = outer;
      }
    },
    { passive: false },
  );
};
// Files dropped from outside, each a `doc:` handle, and a file drag's enter and leave (files.js, documents-glue.js;
// studio diary R19).
export const onFileDrop = (e, kind, f, l) => {
  return OnHooks.drop?.(e, f);
};
export const onFileDrag = (e, kind, f, l) => {
  return OnHooks.drag?.(e, kind, f);
};
// Normal blur commits before focusout chooses its DOM path. Flush callbacks run before post-flush focus commands or
// autofocus; retired owners never dispatch teardown blur.
export const onFocus = (e, kind, f, l) => {
  const own = Owner,
    run = () => e.isConnected && !own?.gone && f();
  return l(kind, () => (Flushing ? FocusQueue.push(run) : run()));
};
// the nearest handler hears the ClipboardEvent record, a paste's files minted `doc:` handles (files.js); the default (a
// field's own paste) proceeds unless it calls preventDefault() (#125)
export const onClipboard = (e, kind, f, l) => {
  if (kind === 'paste') OnHooks.pasteWarm?.();
  return l(
    kind,
    atBody(e, kind, ev => {
      ev.stopPropagation();
      const outer = KeyEvent;
      KeyEvent = ev;
      try {
        f([ev.clipboardData?.getData('text/plain') ?? '', kind === 'paste' ? (OnHooks.pasted?.(ev) ?? []) : []]);
      } finally {
        KeyEvent = outer;
      }
    }),
  );
};
export const onToggle = (e, kind, f, l) => l(kind, ev => f([ev.oldState, ev.newState]));
// a capture's `FrameCapture`, always the refusal (frame-navigation.js, LLP 1020.002 D7)
export const onFrameCapture = (e, kind, f, l) =>
  l('exactcapture', ev => f(['path', 'width', 'height', 'error', 'message'].map(k => ev.detail[k])));
// an iframe's navigation records, the subset a page sees (frame-navigation.js, LLP 1020.002 D2)
export const onFrameNavigation = (e, kind, f) => {
  inflight.n++;
  import('./frame-navigation.js')
    .then(m =>
      m.watch(e, (name, r) => {
        if (name === kind) f(m.positional(r));
      }),
    )
    .finally(() => inflight.n--);
};
// a press on an inline atom (LLP 1045.000 D7), its `AtomEvent` record the event's detail (atoms.js, markup-editor.js)
export const onAtompress = (e, kind, f, l) =>
  l(kind, ev => {
    if (Array.isArray(ev.detail)) f(ev.detail);
  });
export const onSelectionChange = (e, kind, f, l) => {
  return sourceField(e)
    ? l(kind, () => f(inputRecord(e, e.value, false)))
    : onEnd(onSelection(e, (text, a, b, rects) => f([text, a, b, rects])));
};
/** `select`: the editor's selection facts (runner Event::Select); a field's `InputEvent` (x2apps codeedit #2). */
export function onSelect(e, f) {
  e.$select = f;
  e.addEventListener('select', () => {
    if (textField(e)) f(inputRecord(e, e.value, false));
  });
}
/** The `InputEvent` record (contract/types selection.rs's order); a type
 * with none has its caret after its text, a control 0, 0, `none`. */
function inputRecord(e, value, checked, event) {
  const n = e.value?.length ?? 0,
    field = sourceField(e);
  return [
    value,
    checked,
    field ? (e.selectionStart ?? n) : 0,
    field ? (e.selectionEnd ?? n) : 0,
    field ? (e.selectionDirection ?? 'none') : 'none',
    event?.inputType ?? '',
    event?.data ?? null,
    event?.cancelable ?? false,
    event?.isComposing ?? false,
  ];
}
/** A context menu's popover (LLP 1021 §5.1): after the node's own `contextmenu` (both fire), the popover its
 * `contextpopover` names opens anchored to it, the browser's own menu prevented and no ancestor hearing the event
 * (macOS consumes the click too); a text field's edit menu stays the browser's (navigation.js `editMenu`), and a
 * disabled or inert node opens nothing. */
export function cp(e) {
  e.addEventListener('contextmenu', ev => {
    if (
      ev.$cp ||
      !e.getAttribute('contextpopover') ||
      editMenu(ev.target) ||
      e.matches(':disabled') ||
      inertAncestor(e)
    )
      return;
    ev.$cp = 1;
    ev.preventDefault();
    ev.stopPropagation();
    contextAt(ev, () => {
      if (!e.isConnected || e.matches(':disabled') || inertAncestor(e)) return;
      const p = liveById(e.getAttribute('contextpopover') ?? '');
      if (p && !p.matches(':popover-open')) return [p, e];
    });
  });
}
