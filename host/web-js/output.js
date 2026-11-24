// Physical output on a page freeze: a commit's sounds, haptics and MIDI, and those of commits whose tree update a view
// transition still holds, are refused or played before the page freezes, and are not played again by their tree
// update. Linked only by a plan that makes such output (sounds.js, midi.js, transport.js, a haptic command), whose
// `watchOutput` hands it to `commit` (rt.js `useOutput`).
import { Sounds, Origin, Invocation, say, useOutput } from './rt.js';
import { watchPage } from './facts.js';
const Held = new Set(), Settled = new WeakSet(), Invocations = new WeakMap();
const physical = name => name === 'haptic' || Sounds.own.has(name);
const has = commands => commands.some(([name]) => name === 'reload' || physical(name));
function settle(commands) {
  if (!has(commands) || Settled.has(commands)) return;
  for (let i = 0; i < commands.length;) {
    const [name, , origin] = commands[i];
    if (!physical(name)) { i++; continue; }
    say(`command ${name}`);
    const refused = Invocation.run(Invocations.get(commands) ?? 0, () => Origin.refused?.(name, origin));
    if (refused) commands.splice(i, 1); else i++;
  }
  Sounds.apply?.(commands);
  Sounds.midiApply?.(commands);
  for (let i = commands.length - 1; i >= 0; i--) if (physical(commands[i][0])) commands.splice(i, 1);
  Settled.add(commands);
  Held.delete(commands);
}
const Output = {
  /** A commit's commands, asked under `invocation`. */
  made(commands, invocation) { if (has(commands)) Invocations.set(commands, invocation); },
  /** Commands whose tree update a view transition defers. */
  hold(commands) { if (has(commands)) Held.add(commands); },
  /** Their tree update ran, or never will. */
  done(commands) { Held.delete(commands); },
  /** The commit that froze the page: every held commit's output and its own settle now, and the sequences stop. */
  freeze(commands, sound) {
    Sounds.settling = true;
    try {
      for (const c of new Set([...Held, commands])) settle(c);
      Sounds.committed?.(sound);
    } finally {
      Sounds.settling = false;
    }
  },
  /** Whether a freeze already settled these commands' output. */
  settled: commands => Settled.delete(commands),
};
/** Physical output and a sequence need the freeze edge even if the plan reads no page fields. */
export function watchOutput() {
  useOutput(Output);
  watchPage(true);
}
