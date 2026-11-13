// A session's input (scripts/agent.mjs `open`): what a person does to the app — a press, a held contact, typing, a
// share, an answer to a held device request — delivered through the carrier's own input path.
import { partTap, tapRefusal } from './agent-inspect.mjs';
import { existsSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { basename, resolve } from 'node:path';
import { namedRefusal } from './agent-aim.mjs';
import { dragTap, heldDown, heldPhase, holdTap, folderTree } from './agent-drag.mjs';
import { refuseDisabled } from './agent-test.mjs';
import {
  MalformedOperation,
  keyArguments,
  validateKey,
  pointerArguments,
  typeFor,
  ticketOf,
  holdOf,
  heldTicket,
  pickedPaths,
  tapArguments,
  pointerGap,
  clockSlices,
  padOp,
  postInto,
} from './agent-keys.mjs';
import { appShareTarget, driverShare, webShareLocation } from './share-target.mjs';
/** The input methods of session `s` over `carrier`, which `open` adds to `s`. */
export function sessionInput(s, { carrier, host, timing, touch, app }) {
  return {
    /** What this carrier's input actually is (LLP 1035.003 D2/D3): whether it can hold a contact across requests,
     * and how each form is delivered — `platform` (a real input event through the platform's own path),
     * `recognized` (an already-recognized event injected), `activation` (a hit-test and a direct call),
     * `presenter` (seekable native recognition without OS input injection), or `unsupported`. iOS activates and
     * injects; it synthesizes no touch (LLP 1008 §9), so its held contact is recognized (AgentPanIOS.swift), a real
     * one under --touch platform. */
    input:
      host === 'ios' || host === 'host-ios'
        ? {
            contact: true,
            hold: true,
            delivery: kind =>
              (kind === 'drag' && carrier.touches) ||
              (['press', 'down', 'hold', 'up', 'cancel'].includes(kind) && carrier.touch === 'platform')
                ? 'platform'
                : [
                      'contextmenu',
                      'dblclick',
                      'hover',
                      'pinch',
                      'wheel',
                      'down',
                      'move',
                      'hold',
                      'up',
                      'cancel',
                    ].includes(kind)
                  ? 'recognized'
                  : kind === 'drag'
                    ? 'unsupported'
                    : 'activation',
          }
        : host === 'linux' || host === 'windows' || host === 'android'
          ? {
              contact: true,
              hold: true,
              delivery: kind =>
                ['down', 'move', 'hold', 'up', 'cancel', 'contextmenu', 'mouse'].includes(kind)
                  ? 'presenter'
                  : 'platform',
            }
          : { contact: true, hold: true, delivery: () => 'platform' },
    /** The contact this session holds, `{x, y}` in the viewport's space, or null. */
    contact: null,
    /** A press on the target through the host's input path (an iframe target accepts guest `selector` or `x`/`y`);
     * with `{ wheel: [dx, dy] }`, a wheel over it (dy > 0 scrolls down); with `{ hover: true }`, the pointer moved
     * onto it (a hover — and off whatever it was over); with `{ down: true[, at: [x, y]] }`, a contact goes down on
     * it (at its centre, or at an offset from its corner) and stays down until `pointer('up')` (LLP 1035.003 D1).
     * Every reply says how it was delivered (`delivery`), by which carrier, in which mode. */
    // Canvas {mouse:true,at:[x,y]} is one primary mouse click on web, macOS, Windows and Linux;
    // {contextmenu:true,at:[x,y]} is secondary. Points are target-relative; plain
    // canvas taps and held down/move/up contacts remain fingers (LLP 1015.000).
    async tap(target, opts = {}) {
      if (holdOf(target)) {
        if (s.held) throw new Error(`a drag's finger is down (${s.held}): only reads and the clock until it lifts`);
        return s.answer('tap', target, opts.choice);
      }
      const button = tapArguments(opts);
      // `tap midi "<input>" clock <bpm> <ms>` (LLP 1125 D9): a slice at a time, the clock moved to its end, then its
      // ticks delivered.
      if (opts.midi?.clock) {
        let r = {},
          prev = 0;
        for (const { to, ticks } of clockSlices(opts.midi.clock)) {
          await s.clock(`+${to - prev}`);
          prev = to;
          r = await s.op({ op: 'midi', input: opts.midi.input, clock: ticks.map(k => [s.now - to + k, 0xf8]) });
          if (r?.error) break;
        }
        return s.landed({ ...r, delivery: 'substituted', carrier: host, mode: timing });
      }
      if (opts.midi)
        return s.landed({
          ...(await s.op({ op: 'midi', ...opts.midi })),
          delivery: 'substituted',
          carrier: host,
          mode: timing,
        }); // a MIDI input's message (LLP 1125 §4): the runner's input, the arm absent
      if (s.held) throw new Error(`a drag's finger is down (${s.held}): only reads and the clock until it lifts`);
      // a pad's stick (LLP 1127.003 D3)
      if (opts.stick !== undefined) return padOp({ op: 'tap', s, target, opts, host, timing });
      let node,
        part = await partTap({ s, host, touch }, target, opts);
      if (part) return part; // `tap <node>/<part>` (agent-inspect.mjs)
      try {
        node = await s.target(target, true);
      } catch (error) {
        throw await tapRefusal(s, target, error);
      }
      // a disabled control is refused the same on every host (LLP 1116.010 D3)
      await refuseDisabled(s, node, target, opts);
      // @ref LLP 1098 D10 — a media element's session action, as the platform's handler: no reveal, no box, never a
      // press.
      if (opts.mediaSession !== undefined)
        return s.landed({
          ...(await carrier.input(node.id, 'mediasession', opts)),
          tapped: node.id,
          target,
          delivery: 'substituted',
          carrier: host,
          mode: timing,
        });
      // @ref LLP 1042 §9.1 — Exact's picture-in-picture window, outside the app's: its controls by name, the Mac's
      // alone.
      if (opts.pictureInPicture !== undefined) {
        if (!['macos', 'mac'].includes(host))
          return s.tagged({
            tapped: node.id,
            target,
            delivery: 'unsupported',
            reason: `${host} draws no picture-in-picture window of Exact's; the browser's own is out of the page's reach`,
            carrier: host,
            mode: timing,
          });
        return s.landed({
          ...(await carrier.input(node.id, 'pictureinpicture', opts)),
          target,
          carrier: host,
          mode: timing,
        });
      }
      if (opts.mouse) {
        if (!['web', 'linux', 'android', 'windows', 'macos', 'mac'].includes(host))
          throw new Error(`${host} does not carry explicit mouse clicks`);
        if (node.entity !== undefined) throw new Error('mouse cannot be combined with an entity target');
      }
      if (node.entity !== undefined) {
        if (opts.drag) throw new Error(`drag: ${target} is a world entity; a drag starts on a view`);
        const { entity } = await s.op({ op: 'layout', ...node });
        if (entity?.visible?.inFrustum === false || entity?.visible?.behindCamera === true)
          throw new Error(
            `${target} is ${entity.visible.behindCamera ? 'hidden (behind the camera)' : 'off screen'}: layout ${target} shows the placed box`,
          );
        const b = entity?.screen;
        if (!b || ![b.x, b.y, b.w, b.h].every(Number.isFinite))
          throw new Error(
            `${target} has no screen box; layout ${target} shows visibility; state world:* shows camera/components`,
          );
        const x = b.x + b.w / 2,
          y = b.y + b.h / 2;
        const { hit } = await s.op({ op: 'layout', id: node.id, world: true, x, y });
        if (!hit)
          throw new Error(
            `${target} is not hit at ${x},${y}; layout ${target} shows its box; layout ${String(target).split(':')[0]} at ${x} ${y} shows the pick`,
          );
        if (hit.id !== entity.id)
          throw new Error(
            `${target} is behind ${hit.name ?? hit.id} at ${x},${y}; layout ${target} shows its box and layout ${String(target).split(':')[0]}:${hit.name ?? hit.id} shows the blocker`,
          );
        if (s.contact) throw new Error('a contact is already down; use `tap up` or `tap cancel` first');
        const down = await carrier.input(node.id, 'down', { x, y });
        const { phase, ...r } = down.delivery === 'unsupported' ? down : await carrier.input(null, 'up', {});
        return s.landed({
          ...r,
          tapped: node.id,
          target,
          entity: node.entity,
          at: [x, y],
          delivery: r.delivery ?? s.input.delivery('down'),
          carrier: host,
          mode: timing,
        });
      }
      // @ref LLP 1038 D11 — no native carrier turns browser history into a press.
      if (opts.history !== undefined && host !== 'web')
        return s.tagged({
          tapped: node.id,
          target,
          history: opts.history,
          delivery: 'unsupported',
          carrier: host,
          mode: timing,
        });
      // AppKit phases the gesture. Linux's completion fixture supplies a reader
      // sample only to a scroller that handles scrollend; other carriers refuse.
      const readerSample = host === 'linux' && node.handlers?.includes('scrollend');
      if (opts.gesture && !(host === 'macos' || host === 'mac' || readerSample))
        throw new Error(`${host} cannot phase a wheel; \`gesture\` is the AppKit carrier's`);
      if ((opts.fling || opts.hold || opts.release) && !(host === 'macos' || host === 'mac'))
        return s.tagged({
          tapped: node.id,
          target,
          delivery: 'unsupported',
          reason: `${host} has no window-server trackpad; fling, hold and release are the AppKit carrier's (LLP 1142 D3)`,
          carrier: host,
          mode: timing,
        });
      if (
        (opts.contextmenu || opts.dblclick) &&
        !['web', 'ios', 'macos', 'mac', ...(opts.dblclick ? [] : ['linux', 'android', 'windows'])].includes(host)
      )
        throw new Error(`${host} does not carry contextmenu/dblclick input`);
      if (button.length && s.contact) throw new Error(`${button[0]} requires the held contact to be released`);
      // A form this carrier cannot deliver as a hand's is answered `unsupported`, never sent as another input (#107).
      const gap = pointerGap(host, carrier.browser, opts);
      if (gap)
        return s.tagged({ tapped: node.id, target, delivery: 'unsupported', reason: gap, carrier: host, mode: timing });
      // A plain press at a point (`tap <target> at <x> <y>`): whatever a finger there reaches, as for a click
      // (LLP 1012 §1).
      const pressAt =
        !button.length &&
        !opts.wheel &&
        opts.down !== true &&
        opts.pinch === undefined &&
        !opts.drag &&
        opts.at !== undefined;
      const form =
        (button.length || opts.wheel || pressAt) && opts.at !== undefined
          ? (button[0] ?? (opts.wheel ? 'wheel' : 'a press'))
          : null;
      if (form) {
        if (
          ![
            'web',
            'linux',
            'android',
            'windows',
            'macos',
            'mac',
            'host',
            ...(pressAt || ['contextmenu', 'dblclick'].includes(form) ? ['ios', 'host-ios'] : []),
          ].includes(host)
        )
          // iOS: a press, and the long press and double tap that select text there (LLP 1115.000 D11.6)
          throw new Error(`${host} does not carry ${form} at an explicit point`);
        const b = (await s.layout()).nodes.find(n => n.id === node.id),
          [x, y] = opts.at;
        if (!b || x < 0 || y < 0 || x >= b.w || y >= b.h) throw new Error(`${form} at must be inside the target box`);
      }
      if (opts.pinch !== undefined && !['web', 'ios', 'macos', 'mac'].includes(host))
        return s.tagged({
          tapped: node.id,
          target,
          pinch: opts.pinch,
          delivery: 'unsupported',
          reason: `${host} has no pinch (LLP 1057.001 §4)`,
          carrier: host,
          mode: timing,
        });
      // @ref LLP 1070.000 §5 — a list's row by key: the runner's request on every carrier, not an input.
      if (opts.into) {
        const r = await s.op({ op: 'tap', id: node.id, into: opts.into });
        return s.tagged({ ...r, tapped: node.id, target, delivery: 'runner', carrier: host, mode: timing });
      }
      // A target out of view is scrolled into it first, as a person (and Playwright's actionability) would: ledger F7,
      // shop F11. An explicit contact point (`drag … from`, `down at`) is the author's: a visible one is not moved; one
      // out of view brings its view's middle in, as an unspecified point would (the host's reveal scrolls a view, not a
      // point). A point out of view after that (a view taller than the viewport, or a wheel, which reveals nothing) is
      // refused, named.
      const explicit = opts.drag?.from ?? opts.at,
        pointOut = async (point = explicit) => {
          const l = await s.layout(),
            b = l.nodes.find(n => n.id === node.id),
            vp = l.viewport;
          if (!b || !vp) return false;
          const x = b.x + point[0],
            y = b.y + point[1];
          return (x < 0 || y < 0 || x >= vp.w || y >= vp.h) && { x, y, vp };
        };
      const scrolled =
        opts.wheel || opts.history !== undefined || (explicit && !(await pointOut())) ? null : await s.reveal(node.id);
      const out = form && (await pointOut(opts.at));
      if (out)
        throw new Error(
          `${form} at ${opts.at.join(' ')} lands at (${out.x}, ${out.y}), outside the ${out.vp.w}×${out.vp.h} viewport${scrolled ? ' after the view scrolled in' : ''}: scroll the point into view first, or name one nearer the view's middle`,
        );
      // @ref LLP 1080.000 §11 — one whole gesture: press, one straight drag, hold, lift.
      if (opts.drag) return dragTap({ s, carrier, node, target, host, timing, tapRefusal, scrolled }, opts.drag);
      if (opts.holdState !== undefined)
        // a held state (LLP 1122 D8)
        return holdTap({ s, node, target, host, timing, tapRefusal, scrolled }, opts.holdState);
      const kind =
        opts.history !== undefined
          ? 'history'
          : opts.pinch !== undefined
            ? 'pinch'
            : opts.down
              ? 'down'
              : opts.drop
                ? 'drop'
                : opts.dragenter
                  ? 'dragenter'
                  : opts.dragleave
                    ? 'dragleave'
                    : opts.wheel
                      ? 'wheel'
                      : opts.hover
                        ? 'hover'
                        : (button[0] ?? 'press');
      if ((kind === 'down' || kind === 'press') && s.contact)
        throw new Error('a contact is already down; use `tap up` or `tap cancel` first'); // one finger at a time
      let at;
      if (kind === 'down' && opts.at) {
        const b = (await s.layout()).nodes.find(n => n.id === node.id);
        if (!b) throw new Error(`view ${node.id} has no box on screen`);
        at = { x: b.x + opts.at[0], y: b.y + opts.at[1] };
      }
      if (kind === 'down' && carrier.touches && touch === 'platform')
        return s.tagged({
          ...(await heldDown(s, node, at)),
          tapped: node.id,
          target,
          ...(scrolled ? { scrolled } : {}),
          carrier: host,
          mode: timing,
        });
      let r;
      try {
        r = await carrier.input(node.id, kind, { ...opts, ...at });
      } catch (error) {
        throw await tapRefusal(s, target, await s.named(error));
      }
      if (r.error) r.error = (await tapRefusal(s, target, await s.named(new Error(r.error)))).message;
      if (kind === 'down' && r.delivery !== 'unsupported') {
        s.contact = r.contact === false ? null : { x: r.at[0], y: r.at[1] };
        if (Number.isFinite(r.clock)) s.now = r.clock;
      }
      return s.landed({
        ...r,
        tapped: node.id,
        target,
        ...(scrolled ? { scrolled } : {}),
        delivery: r.delivery ?? s.input.delivery(kind),
        carrier: host,
        mode: r.mode ?? timing,
      });
    },
    /** A refusal that names nodes by id (a plain tap that would press a control inside its target), by their
     * testIds. */
    async named(error) {
      if (!/^tap #\d+ would press /.test(error?.message ?? '')) return error;
      const nodes = (await s.op({ op: 'tree' }).catch(() => null))?.nodes;
      if (nodes) error.message = namedRefusal(error.message, nodes);
      return error;
    },
    /** Scroll view `id` into view when its middle is out of it — its nearest scroll containers, then the page (the
     * host's `reveal`): `{from, to}` where its middle moved, or null when it was in view. */
    async reveal(id) {
      const r = await carrier.reveal(id);
      if (r.error) throw new Error(r.error);
      return r.scrolled ? { from: r.from, to: r.to } : null;
    },
    /** The held contact's next phase (LLP 1035.003 D1): `move` to `{x, y}` in the viewport or `by` `{dx, dy}`,
     * over `ms` of real time on platform carriers or event timestamps on Linux, without seeking its runner; `hold` for
     * `ms`; `up`; `cancel`.
     * The platform owns hit-testing, recognition, scrolling and animation: the app receives whatever it delivers,
     * and a carrier that cannot hold a contact answers `delivery: "unsupported"` rather than faking one. */
    async pointer(phase, opts = {}) {
      pointerArguments(phase, opts);
      if (s.held) throw new Error(`a drag's finger is down (${s.held}): only reads and the clock until it lifts`);
      // the clock's, as a drag's; real time under --timing platform, for UIKit's recognizers
      if (phase === 'hold' && timing !== 'platform' && opts.virtual === undefined) opts = { ...opts, virtual: true };
      // Modifiers that a move or an up names are held from that event on (#107), where the carrier can hold them.
      if (opts.modifiers !== undefined) {
        if (!['move', 'up'].includes(phase))
          throw new MalformedOperation(`tap ${phase}: modifiers are held through a down, a move or an up`);
        if (
          !String(opts.modifiers)
            .split('+')
            .filter(Boolean)
            .every(k => ['Shift', 'Control', 'Alt', 'Meta'].includes(k))
        )
          throw new MalformedOperation(
            `tap … modifiers: ${opts.modifiers} is not Shift, Control, Alt and Meta joined by +`,
          );
        const gap = pointerGap(host, carrier.browser, { ...opts, phase });
        if (gap) return s.tagged({ phase, delivery: 'unsupported', reason: gap, carrier: host, mode: timing });
      }
      // No session contact: the carrier says whether it can phase one (a touch
      // on Firefox/WebKit cannot). A mouse drag it already accepted has a
      // contact, so that one is booked below — the lift clears it, and a
      // virtual hold seeks the clock (drums R13, platformer R7).
      if (carrier.phasedTouch === false && !s.contact) return carrier.input(null, phase, opts);
      if (!s.contact && ['up', 'cancel'].includes(phase)) {
        const state = await s.state();
        const contacts = (state.world ?? []).flatMap(w =>
          (w.input?.controlContacts ?? []).filter(c => c.id < 4294967292).map(c => ({ ...c, canvas: w.canvas })),
        );
        if (contacts.length === 1) return s.op({ op: 'tap', id: contacts[0].canvas, contact: contacts[0].id, phase });
        if (contacts.length > 1) throw new Error('multiple restored contacts; release one with tap and its contact ID');
      }
      if (!s.contact) throw new Error('no contact is down (tap <target> down first)');
      if (s.contact.whole)
        return s[phase === 'up' ? 'landed' : 'tagged']({
          ...(await heldPhase(s, carrier, phase, opts)),
          carrier: host,
          mode: timing,
        }); // one real touch, played at the lift (agent-drag.mjs)
      const before = s.now,
        r = await carrier.input(null, phase, opts);
      if (r.delivery !== 'unsupported') {
        if (Number.isFinite(r.clock)) s.now = r.clock;
        if (r.contact === false || phase === 'up' || phase === 'cancel') s.contact = null;
        // A move refused partway answers where the delivered samples left the contact.
        else if (phase === 'move' && Array.isArray(r.at)) s.contact = { x: r.at[0], y: r.at[1] };
      }
      // A drag's press and hold are virtual time (platformer R7). Linux already sought and reports `clock`.
      if (phase === 'hold' && opts.virtual && opts.ms > 0 && !(Number.isFinite(r.clock) && r.clock > before))
        await s.clock(`+${opts.ms}`);
      return s.landed({
        ...r,
        phase,
        delivery: r.delivery ?? s.input.delivery(phase),
        carrier: host,
        mode: r.mode ?? timing,
      });
    },
    /** Deliver a location to a navigation root (LLP 1038 D11), or set an input's text through the host's text input
     * path; an iframe accepts `{text, selector}` or `{key, selector}` for its guest. */
    async type(target, text) {
      // `type <surface> message <text>`, `post` (LLP 1139.004 D1)
      if (text?.message !== undefined) return postInto(s, target, text.message, { host, timing });
      if (holdOf(target)) {
        if (s.held) throw new Error(`a drag's finger is down (${s.held}): only reads and the clock until it lifts`);
        return s.answer('type', target, typeof text === 'object' && text !== null ? JSON.stringify(text) : text);
      }
      if (typeof text === 'string' && text.startsWith('share:')) return s.share(target, text);
      const options = typeof text === 'object' && text !== null ? text : { text };
      // a pad button on a world canvas (LLP 1127.003 D3)
      if (/^Gamepad/.test(options.key ?? '')) return padOp({ op: 'type', s, target, opts: options, host, timing });
      keyArguments(options);
      if (options.key != null) await validateKey(options.key, carrier);
      if (s.held) throw new Error(`a drag's finger is down (${s.held}): only reads and the clock until it lifts`);
      const node = await s.find(target);
      const key = options.key;
      if (options.for !== undefined) {
        return typeFor({
          node,
          target,
          options,
          carrier,
          clock: spec => s.clock(spec),
          tagged: reply => s.landed(reply),
          delivery: s.input.delivery('key'),
          host,
          timing,
        });
      }
      // A field out of view is scrolled into it, as for a tap; a root (a navigation root's location) is the page.
      const scrolled = node.depth === 0 ? null : await s.reveal(node.id);
      const r =
        key != null
          ? await carrier.input(node.id, 'key', { ...options, key: String(key) })
          : await carrier.input(node.id, options.clipboard ? 'clipboard' : 'type', {
              ...options,
              text: String(options.text ?? ''),
            });
      return s.landed({
        ...r,
        typed: node.id,
        target,
        ...(scrolled ? { scrolled } : {}),
        delivery: r.delivery ?? s.input.delivery(key != null ? 'key' : 'type'),
        carrier: host,
        mode: timing,
      });
    },
    /** A share another app sends (LLP 1133.024 D9): `type <root> "share:?text=…&files=<path>"`, handed with the
     * app's `share_target` to the host's claim path; the web takes its text as its GET share target's location. */
    async share(target, href) {
      const shareTarget = appShareTarget(app),
        share = driverShare(href, process.cwd());
      if (host === 'web') return s.type(target, webShareLocation(shareTarget, share));
      const node = await s.find(target);
      return s.landed({
        ...(await s.op({ op: 'share', id: node.id, target: shareTarget, share })),
        carrier: host,
        mode: timing,
      });
    },
    /** Answer held device request `@N` (LLP 1069.007 D4), resolved before any view: `tap @N <choice>` (`cancel`, or
     * a choice the capability declares) or `type @N <value>` (a fixture path, a URL, JSON). `@<id>` names the hold
     * by the node its answer arrives at (a picker's `id`) or by its capability (`@open-directory`, `@pick`),
     * whatever its ticket (files F11). The hold is consumed once; a stale ticket is refused by name. The reply says
     * `delivery: "substituted"`. */
    async answer(op, target, value) {
      // A module's listed entry (LLP 1133.004 §7): `tap @ref` takes no choice; it is the entry's click. A listed call's
      // takes the person's act (LLP 1133.033 D21).
      if ((value == null || value === '') && op === 'tap' && ticketOf(target) != null)
        return s.landed(await s.op({ op, ticket: ticketOf(target) }));
      if (value == null || value === '')
        throw new MalformedOperation(
          `${op} ${target}: expected ${op === 'tap' ? 'a choice (cancel, …)' : 'a value'}; state shows the hold under pending`,
        );
      if (s.held) throw new Error(`a drag's finger is down (${s.held}): only reads and the clock until it lifts`);
      const pending = ticketOf(target) != null && op === 'tap' ? [] : ((await s.op({ op: 'state' })).pending ?? []);
      const ticket = ticketOf(target) ?? heldTicket(pending, target);
      if (op === 'tap') return s.landed(await s.op({ op, ticket, choice: String(value) }));
      // A picker's answer is files on this machine (LLP 1069.002 D9): each
      // made absolute here, where the host copies it from; the browser is
      // handed the bytes, as a real picker hands it a File.
      const held = pending.find(p => p.ticket === ticket);
      // An export's answer is where the copy goes (LLP 1069.010 D3): a
      // path on this machine; the browser hands back the bytes to write.
      // Trimmed, as the hosts trim it: a test's `pick` ends its paths with
      // a newline, which named the file `x.json\n` (drums R12).
      if (held?.device?.capability === 'export') {
        const to = resolve(String(value).trim());
        const r = await s.op({ op, ticket, text: to });
        if (typeof r.bytes === 'string') {
          writeFileSync(to, Buffer.from(r.bytes, 'base64'));
          delete r.bytes;
        }
        return s.landed(r);
      }
      // A document picker's answer is paths on this machine (LLP 1069.010
      // D2), minted as the person's choice; the browser is handed each
      // file's bytes, or a folder's tree, or a name to save to.
      const documents = ['open-file', 'open-directory', 'save-file'];
      if (documents.includes(held?.device?.capability)) {
        const paths = String(value)
          .split('\n')
          .map(p => p.trim())
          .filter(Boolean)
          .map(p => resolve(p));
        const req = { op, ticket, text: paths.join('\n') };
        if (host === 'web') {
          req.files = paths.map(p =>
            held.device.capability === 'open-directory'
              ? { name: basename(p), files: folderTree(p) }
              : held.device.capability === 'save-file'
                ? { name: basename(p), bytes: '' }
                : { name: basename(p), bytes: readFileSync(p).toString('base64') },
          );
        }
        return s.landed(await s.op(req));
      }
      if (held?.device?.capability !== 'pick') return s.landed(await s.op({ op, ticket, text: String(value) }));
      const paths = pickedPaths(value).map(p => resolve(p));
      const missing = paths.find(p => !existsSync(p) || !statSync(p).isFile());
      // split at its spaces: `pick` keeps one path whole
      const spaced = !String(value).includes('\n') && /\s/.test(String(value).trim());
      if (missing)
        throw new Error(
          `type ${target}: no such file ${missing}${spaced ? `; \`type @\` takes paths apart at each space, and \`pick ${target} ${JSON.stringify(String(value).trim())}\` keeps a path with a space whole` : ''}`,
        );
      const req = { op, ticket, text: paths.join('\n') + '\n' };
      if (host === 'web')
        req.files = paths.map(p => ({ name: basename(p), bytes: readFileSync(p).toString('base64') }));
      return s.landed(await s.op(req));
    },
  };
}
