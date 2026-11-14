// @ref LLP 1166.002 D1 — verify reads the accessibility tree (`tree --ax`, LLP 1080.002) once at each screen it checks
// and reports the three findings one read gives (LLP 1080.002 D8): a control with no name, an element exposed while its
// view is hidden, and one reachable outside an open modal. They count as check's findings do, and app.json's
// `verify.accept` takes them as it takes check's: by kind and the testId of the view the element joined. The report's
// group is verify-findings.mjs `axGroup`.
import { accepts } from './agent-check.mjs';
import { axClean } from './agent-ax.mjs';
import { AX_HOSTS } from './verify-findings.mjs';

const KINDS = new Set(['unnamed', 'exposed-hidden', 'outside-modal']);
/** `tree --ax`'s largest `limit` (LLP 1080.002 D7): a long screen is read whole where the reply's bytes allow. */
const LIMIT = 2000;
const firstLine = (text) => String(text ?? '').split('\n').find((l) => l.trim())?.trim() ?? '';

/** The tree at the screen `s` shows, read once and reported as `axRead` reports it; null on a host verify does not read
 * it on. A read that fails is a note, `{ screen, note }`, never a failure of the run. */
export async function axHere(s, screen, accept = []) {
  if (!AX_HOSTS.has(s.host)) return null;
  try { return axRead(await s.tree(undefined, { ax: true, limit: LIMIT }), screen, accept); }
  catch (e) { return { screen, note: firstLine(e.message) }; }
}

/** Who a finding is about: the joined view's testId, else its id. */
const who = (f) => (f.testId != null ? (/\s/.test(f.testId) ? JSON.stringify(String(f.testId)) : String(f.testId)) : `#${f.id ?? '?'}`);

/** One line a finding, worded as check's are: its kind, who, what; `hint` false leaves out the remedy. */
export function axLine(f, { hint = true } = {}) {
  const what = f.kind === 'unnamed' ? `${/^[aeiou]/i.test(f.detail) ? 'an' : 'a'} ${f.detail} with no name${hint ? ': a screen reader says only its role; give it aria-label="…", or aria-labelledby="<id>" of the text that labels it' : ''}`
    : f.kind === 'exposed-hidden' ? `${f.detail} is exposed to a screen reader`
    : `${f.detail} is reachable while a modal is open (${f.basis})`;
  return `  ${f.kind.padEnd(8)} ${who(f)}: ${what}`;
}

/** The entry of `verify.accept` that takes a finding: check's rule (agent-check.mjs `accepts`) on the joined view's testId. */
const acceptOf = (accept, f) => accepts(accept, { kind: f.kind, id: 0 }, new Map([[0, { props: { testId: f.testId } }]]));

/** Why a read cannot say "no findings" (`axClean`): the screen changed during it, its reply was cut, or part of the screen
 * was out of its reach (a frame; on iOS what the host names, as the native tab bar); null when it can. */
function incompleteness(reply) {
  if (axClean(reply)) return null;
  const ax = reply.ax, cut = ax.truncated && (ax.truncated.elements ? 'the reply was cut short' : 'a name or value was cut at 200 characters');
  const excluded = (ax.coverage?.excluded ?? []).map((x) => (x.frame != null ? 'a frame is not read' : String(x.reason ?? x.root).split(/[;:]/)[0]));
  return [reply.spanned && 'the screen changed during the read', cut, ...new Set(excluded)].filter(Boolean).join('; ') || 'its coverage is incomplete';
}

/** What verify reports of one `tree --ax` reply at `screen`, in check's shape: `{ screen, source, findings, accepted,
 * incomplete? }`, each finding with its `line` and `node`, each accepted one with the entry that took it (`accept`); or
 * `{ screen, note }` where the host has no tree to give. An incomplete read keeps its findings and says why it is
 * incomplete (LLP 1080.002 D8). A read UIKit answered without its accessibility runtime (the host names the process
 * as excluded) is a note too: UIKit then derives no label from a title and no text field element, so a control
 * VoiceOver names would be reported unnamed; the note is the host's own reason, which says how to turn it on. */
export function axRead(reply, screen, accept = []) {
  const ax = reply?.ax;
  if (!ax || ax.unavailable) return { screen, note: `unavailable: ${ax?.reason ?? 'no reply'}` };
  const runtime = ax.coverage?.excluded?.find((x) => x.root === 'process');
  if (runtime) return { screen, note: runtime.reason };
  const found = (ax.findings ?? []).filter((f) => KINDS.has(f.kind)).map((f) => ({ ...f, node: who(f), line: axLine(f) }));
  const taken = found.map((f) => acceptOf(accept, f)), incomplete = incompleteness(reply);
  return { screen, source: ax.source, ...(incomplete ? { incomplete } : {}), findings: found.filter((_, i) => !taken[i]),
    accepted: found.flatMap((f, i) => (taken[i] ? [{ ...f, line: `${axLine(f, { hint: false })}; accepted: ${taken[i].why}`, accept: taken[i] }] : [])) };
}
