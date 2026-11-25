// How `exact add` writes each of a package's grant lines (LLP 1133.001 D8):
// `grantLines` the lines, `grantAdvice` them in words. A test parses every
// package's lines with the real parsers (bake/src/reach_grant_lines_tests.rs).
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(fileURLToPath(new URL('..', import.meta.url)));

/** The lines that grant each of `grants` as the app writes them, one array a grant: its own line, then any line it
 * needs beside it. A `device.*` grant follows the device table (`runner/src/device.rs`, LLP 1069.008 D1):
 * `device.<name> purpose.<key>` with its purpose's strings key, `device.<name>` alone for a row nothing asks the person
 * about, beside the line a row needs (`device.push` beside `device.notifications purpose.<key>`). Every other
 * namespace is written as its own grammar has it: `auth.apple <client id>`, `payment.apple-pay <merchant id>`, a line
 * that names no target alone (LLP 1020.002 D9), any other `<its target>`. */
export function grantLines(grants, table = readFileSync(resolve(ROOT, 'runner/src/device.rs'), 'utf8')) {
  const row = (name) => {
    const r = new RegExp(`name: "${name}",([\\s\\S]*?)\\.\\.ROW`).exec(table)?.[1] ?? '';
    return { purpose: !/purpose: false/.test(r), needs: /needs: Some\("([^"]+)"\)/.exec(r)?.[1] ?? null };
  };
  const device = (name) => row(name).purpose ? `device.${name} purpose.<key>` : `device.${name}`;
  const targets = { 'auth.apple': '<client id>', 'auth.google': '<client id>', 'auth.passkey': '<rp id>', 'payment.apple-pay': '<merchant id>' };
  const alone = ['browser.automation', 'clipboard.read', 'payment.google-pay', 'doc.keep'];
  return grants.map((grant) => {
    if (grant.startsWith('device.')) {
      const name = grant.slice('device.'.length), { needs } = row(name);
      return needs ? [device(name), device(needs)] : [device(name)];
    }
    return [alone.includes(grant) ? grant : `${grant} ${targets[grant] ?? '<its target>'}`];
  });
}

/** How each of a package's grants is written, in words: its lines ([`grantLines`]), what a placeholder is. */
export function grantAdvice(grants, table) {
  const say = (placeholder) => ({ '<key>': 'a strings key whose text is the purpose', '<client id>': "a client id the provider issued",
    '<rp id>': 'the domain its passkeys are for', '<merchant id>': 'an Apple Pay merchant ID', '<its target>': 'what it reaches' })[placeholder];
  const one = ([line, ...beside]) => {
    const placeholder = /<[^>]+>$/.exec(line)?.[0];
    const how = placeholder ? `\`${line}\`, ${say(placeholder)}` : `\`${line}\` alone`;
    return beside.length ? `${how}, beside ${beside.map((b) => `\`${b}\``).join(' and ')}` : how;
  };
  const all = grantLines(grants, table).map(one);
  return all.length > 1 ? `one of ${all.join('; ')}` : all[0];
}
