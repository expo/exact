// LLP 1133.028 §8.4 drive 11 for @exact/attestation's fixture app
// (test/fixture), on a target with no implementation: the status reads its
// declared default and data code's `attest` refuses NotSupportedError.
//
// usage: bun packages/attestation/test/absent.mjs <web|linux> [--keep]
import { checker } from '../../camera/test/fixture.mjs';
import { texts, until } from '../../photos/test/fixture.mjs';
import { build, launch, makeFixture } from '../../tracking-consent/test/fixture.mjs';

const argv = process.argv.slice(2), host = argv.find((a) => ['web', 'linux', 'windows'].includes(a)) ?? 'web';
const { check, failed } = checker();
const fixture = makeFixture('attestation');
try {
  build(fixture, host);
  const s = await launch(fixture, host);
  try {
    await s.clock('settle');
    check(host, 'drive 11: the declared default', (await texts(s)).status === 'Available: false, provider: none', (await texts(s)).status);
    await s.tap('prove');
    const proved = await until(s, 'proved', (v) => !!v);
    check(host, 'drive 11: attest refuses NotSupportedError', proved === 'error NotSupportedError', proved);
    const st = await s.state();
    check(host, 'drive 11: no ticket', !(st.pending ?? []).some((p) => p.device), JSON.stringify(st.pending));
  } finally { await s.close(); }
} finally {
  if (!argv.includes('--keep')) fixture.remove();
  else console.log(`kept ${fixture.app}`);
}
console.log(failed() ? `${failed()} failed` : 'all passed');
process.exit(failed() ? 1 : 0);
