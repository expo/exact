// Platform-scoped simulator substring matching and interactive TTY picker (LLP 1189 D4).
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { chooseSimulator, compareRuntimeVersionDesc, formatRuntime, isTargetPlatformRuntime, matchSimulatorQuery, parseRuntimeVersion, simulators } from './devices.mjs';
export { compareRuntimeVersionDesc, formatRuntime, isTargetPlatformRuntime, matchSimulatorQuery, parseRuntimeVersion };

/** Whether an interactive TTY picker may prompt in this process. */
export function canPromptInteractive(args = [], { env = process.env, stdin = process.stdin, stdout = process.stdout } = {}) {
  return Boolean(stdin?.isTTY && stdout?.isTTY && !env.CI && !env.EXACT_NON_INTERACTIVE && !args.includes('--no-interactive') && !args.includes('--json'));
}

/** Sort simulator candidates for interactive display: booted first, then last used, then newest runtime, then name. */
export function sortSimulatorCandidates(candidates, lastUsed = null) {
  return [...candidates].sort((a, b) =>
    Number(b.state === 'Booted') - Number(a.state === 'Booted')
    || Number(b.udid === lastUsed) - Number(a.udid === lastUsed)
    || compareRuntimeVersionDesc(a.runtime, b.runtime)
    || a.name.localeCompare(b.name));
}

function readLastUsed(dir, tv = false) {
  if (!dir) return null;
  try { return readFileSync(resolve(dir, '.exact', tv ? 'simulator-tv' : 'simulator'), 'utf8').trim() || null; } catch { return null; }
}

export function rememberSimulator(dir, udid, tv = false) {
  if (!dir || !udid) return;
  const file = resolve(dir, '.exact', tv ? 'simulator-tv' : 'simulator');
  try { mkdirSync(dirname(file), { recursive: true }); writeFileSync(file, `${udid}\n`); } catch {}
}

/** Prompt the user on a TTY to pick one simulator from `candidates`. Returns the chosen device, or `null` if cancelled. */
export async function pickSimulatorInteractive(candidates, {
  lastUsed = null,
  heading = 'Choose an iOS Simulator',
  stdin = process.stdin,
  stdout = process.stdout,
} = {}) {
  const ordered = sortSimulatorCandidates(candidates, lastUsed);
  if (!ordered.length) throw new Error('no available simulators to choose from (add one in Xcode)');
  const render = () => {
    stdout.write(`\n${heading}:\n`);
    ordered.forEach((d, i) => {
      const state = d.state === 'Booted' ? 'Booted' : 'Shutdown';
      const mark = d.udid === lastUsed ? ' *' : '';
      stdout.write(`  ${String(i + 1).padStart(2)}. ${d.name} (${formatRuntime(d.runtime)}, ${state})${mark}  [${d.udid.slice(0, 8)}]\n`);
    });
    stdout.write(`Select [1-${ordered.length}] (Enter for 1, q/Esc to cancel): `);
  };
  render();
  return new Promise((done) => {
    // A caller already reading keys (the dev loop's controls) keeps reading once the choice is made.
    const wasRaw = Boolean(stdin.isRaw), wasFlowing = stdin.readableFlowing === true;
    if (stdin.isTTY && typeof stdin.setRawMode === 'function') stdin.setRawMode(false);
    stdin.resume();
    stdin.setEncoding('utf8');
    let line = '';
    const finish = (choice) => {
      stdin.off('data', onData);
      stdin.off('end', onEnd);
      if (stdin.isTTY && typeof stdin.setRawMode === 'function' && wasRaw) stdin.setRawMode(true);
      else if (!wasFlowing) stdin.pause();
      done(choice);
    };
    const onEnd = () => finish(null);
    const onData = (chunk) => {
      const text = String(chunk);
      if (text.includes('\x03') || text === '\x1b') { stdout.write('\n'); return finish(null); }
      line += text;
      const nl = line.search(/\r|\n/);
      if (nl < 0) return;
      const answer = line.slice(0, nl).trim();
      line = '';
      if (!answer) return finish(ordered[0]);
      if (answer.toLowerCase() === 'q' || answer === '\x1b') return finish(null);
      const index = Number.parseInt(answer, 10);
      if (Number.isInteger(index) && index >= 1 && index <= ordered.length) return finish(ordered[index - 1]);
      stdout.write(`Enter a number between 1 and ${ordered.length}, or q to cancel: `);
    };
    stdin.on('data', onData);
    stdin.once('end', onEnd);
  });
}

/** Human-facing simulator resolver for `dev` and `ios --run`: supports `--pick` and prompts on
 * multi-booted ambiguity when on an interactive TTY; preserves deterministic errors otherwise. */
export async function resolveHumanSimulator(pick, {
  tv = false,
  app,
  dir,
  hint = '--sim <udid|name> (or EXACT_SIM)',
  forcePick = false,
  interactive = canPromptInteractive(),
  listSimulators = simulators,
  pickInteractive = pickSimulatorInteractive,
} = {}) {
  const lastUsed = readLastUsed(dir, tv);
  if (forcePick) {
    if (!interactive) throw new Error('--pick requires an interactive terminal (use --sim <udid|name> or EXACT_SIM)');
    const available = listSimulators().filter((d) => isTargetPlatformRuntime(d.runtime, tv));
    const chosen = await pickInteractive(available, { lastUsed, heading: `Choose ${tv ? 'an Apple TV' : 'an iOS'} Simulator` });
    if (!chosen) return null;
    rememberSimulator(dir, chosen.udid, tv);
    return chosen;
  }
  try {
    return chooseSimulator(pick, { tv, app, dir, hint });
  } catch (error) {
    if (error?.code !== 'EXACT_SIMULATOR_AMBIGUOUS' || !interactive || !Array.isArray(error.candidates)) throw error;
    const chosen = await pickInteractive(error.candidates, { lastUsed, heading: `Multiple ${tv ? 'Apple TV' : 'iOS'} simulators are booted — choose one` });
    if (!chosen) return null;
    rememberSimulator(dir, chosen.udid, tv);
    return chosen;
  }
}
