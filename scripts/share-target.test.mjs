// The share target's Android activity (LLP 1133.024 D5): the derived filters and the member the reader reads.
// The web's share location (D2), which the web agent's `share:` drive navigates to.
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { androidShareActivity, webShareLocation } from './share-target.mjs';

const fieldnotes = {
  action: '/share', method: 'POST', enctype: 'multipart/form-data',
  params: { title: 'title', text: 'text', files: [{ name: 'files', accept: ['text/plain', '.md', 'image/*'] }] },
};

test('text and files: SEND takes text and every accepted type, SEND_MULTIPLE the files', () => {
  const xml = androidShareActivity(fieldnotes).join('\n');
  assert.match(xml, /android:name="dev\.exact\.app\.MainActivity" android:launchMode="singleTask" tools:replace="android:launchMode"/);
  const [send, multiple] = xml.split('</intent-filter>');
  assert.match(send, /action\.SEND"/);
  assert.deepEqual([...send.matchAll(/mimeType="([^"]+)"/g)].map((m) => m[1]), ['text/plain', 'text/markdown', 'image/*']);
  assert.match(multiple, /action\.SEND_MULTIPLE"/);
  assert.deepEqual([...multiple.matchAll(/mimeType="([^"]+)"/g)].map((m) => m[1]), ['text/plain', 'text/markdown', 'image/*']);
});

test('the meta-data carries action and params only, escaped for an attribute', () => {
  const value = androidShareActivity(fieldnotes).join('\n').match(/share_target" android:value="([^"]*)"/)[1];
  assert.deepEqual(JSON.parse(value.replaceAll('&quot;', '"').replaceAll('&lt;', '<').replaceAll('&amp;', '&')), { action: '/share', params: fieldnotes.params });
});

test('text only has no SEND_MULTIPLE; no target, no activity', () => {
  const xml = androidShareActivity({ action: '/s', params: { text: 't' } }).join('\n');
  assert.doesNotMatch(xml, /SEND_MULTIPLE/);
  assert.equal(androidShareActivity(undefined), null);
});

test("a share's fields replace the action's query and stay before its fragment, as the Web Share Target spec has it", () => {
  // https://w3c.github.io/web-share-target/, "Handling incoming shares": a GET share sets the action URL's query.
  const at = (action, share) => webShareLocation({ action, params: { title: 'title', text: 'text' } }, share);
  // Form-encoded, as the spec's serializer writes it: a space is `+`.
  assert.equal(at('/share', { text: "hello there! (it's ~5*)" }), '/share?text=hello+there%21+%28it%27s+%7E5*%29');
  assert.equal(at('/share?from=sheet', { text: 'hello' }), '/share?text=hello');
  assert.equal(at('/share#preview', { text: 'hello' }), '/share?text=hello#preview');
  assert.equal(at('/share?from=sheet#preview', { title: 'T', text: 'hello' }), '/share?title=T&text=hello#preview');
  // Nothing shared under a member: the action's path and fragment, its query still replaced.
  assert.equal(at('/share?from=sheet#preview', {}), '/share#preview');
  assert.equal(at('/share', {}), '/share');
  for (const action of ['/share', '/share?from=sheet', '/share#preview'])
    assert.equal(new URL(at(action, { text: 'hello' }), 'https://example.test').searchParams.get('text'), 'hello');
});
