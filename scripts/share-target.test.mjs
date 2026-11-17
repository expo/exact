// The share target's Android activity (LLP 1133.024 D5): the derived filters and the member the reader reads.
// The web's share location (D2), which the web agent's `share:` drive navigates to.
// Apple's presentation (D6): the compose sheet unless the app declares the confirmation.
// Safari's page details (D4): asked for when the app maps `url`.
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { androidShareActivity, appleActivationRule, applePresentation, appleSharePage, shareTargetProblems, webShareLocation } from './share-target.mjs';

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

test("Apple's extension shows the compose sheet unless the app declares the confirmation, which has no post button to name", async () => {
  const { validate } = await import('./app.mjs');
  const schema = await Bun.file(new URL('./app.schema.json', import.meta.url)).json();
  const member = schema.properties.share_target;
  const declared = apple => ({ ...fieldnotes, apple });
  assert.equal(applePresentation(fieldnotes), 'compose');
  assert.equal(applePresentation(declared({ postTitle: 'Save' })), 'compose');
  assert.equal(applePresentation(declared({ presentation: 'confirmation' })), 'confirmation');
  for (const presentation of ['compose', 'confirmation']) assert.deepEqual(validate(declared({ presentation }), member, 'share_target', schema), []);
  assert.ok(validate(declared({ presentation: 'save' }), member, 'share_target', schema).length);
  // The extension runs no app code (D4), so the member has no word for an app's own page script.
  for (const key of ['preprocessing', 'NSExtensionJavaScriptPreprocessingFile']) assert.ok(validate(declared({ [key]: 'share/page.js' }), member, 'share_target', schema).length, key);
  assert.deepEqual(shareTargetProblems({ share_target: declared({ presentation: 'compose', postTitle: 'Save' }) }), []);
  assert.deepEqual(shareTargetProblems({ share_target: declared({ presentation: 'confirmation' }) }), []);
  assert.match(shareTargetProblems({ share_target: declared({ presentation: 'confirmation', postTitle: 'Save' }) }).join('\n'), /apple\.postTitle.*no post button/);
  // `postTitle` is the button's word as the app writes it; the extension looks nothing up in the app's strings.
  const words = member.properties.apple;
  assert.match(words.properties.postTitle.description, /as written/);
  assert.doesNotMatch(`${words.description} ${words.properties.postTitle.description}`, /strings key/);
});

test("Apple's extension asks Safari for the page when the app maps url, and only then", () => {
  const mapped = params => ({ action: '/share', params });
  assert.equal(appleSharePage(fieldnotes), false);
  assert.deepEqual(appleActivationRule(fieldnotes), { NSExtensionActivationSupportsText: true, NSExtensionActivationSupportsWebURLWithMaxCount: 1,
    NSExtensionActivationSupportsImageWithMaxCount: 1000, NSExtensionActivationSupportsFileWithMaxCount: 1000 });
  assert.equal(appleSharePage(mapped({ title: 'title', text: 'text' })), false);
  for (const params of [{ url: 'url' }, { title: 'title', url: 'link' }]) {
    assert.equal(appleSharePage(mapped(params)), true);
    assert.deepEqual(appleActivationRule(mapped(params)), { NSExtensionActivationSupportsWebURLWithMaxCount: 1, NSExtensionActivationSupportsWebPageWithMaxCount: 1 });
  }
});

test("an app's own activation rule stands: a dictionary gains the web page when the app maps url, and a predicate is baked as written", () => {
  const owned = (params, rule) => ({ action: '/share', params, apple: { NSExtensionActivationRule: rule } });
  const problems = target => shareTargetProblems({ share_target: target }).join('\n');
  // A dictionary gains the web page, the key Apple's guide requires of an extension that reads a webpage. A count
  // the app wrote stands, and the manifest's own object is not changed.
  const own = { NSExtensionActivationSupportsWebURLWithMaxCount: 1 };
  assert.deepEqual(appleActivationRule(owned({ url: 'url' }, own)), { NSExtensionActivationSupportsWebURLWithMaxCount: 1, NSExtensionActivationSupportsWebPageWithMaxCount: 1 });
  assert.deepEqual(own, { NSExtensionActivationSupportsWebURLWithMaxCount: 1 });
  assert.equal(appleActivationRule(owned({ url: 'url' }, { ...own, NSExtensionActivationSupportsWebPageWithMaxCount: 3 })).NSExtensionActivationSupportsWebPageWithMaxCount, 3);
  assert.equal(problems(owned({ url: 'url' }, own)), '');
  // With no `url` mapped the app's rule is the rule.
  assert.equal(appleActivationRule(owned({ text: 'text' }, own)), own);
  // A predicate is the app's text (D6: the authored rule beats the derived one). The bake neither adds to it nor
  // judges what it matches: one that takes the page's property list by a broader type, by its own type, or not at
  // all is baked as written, with `url` mapped or not.
  const predicate = type => `SUBQUERY(extensionItems, $item, SUBQUERY($item.attachments, $a, ANY $a.registeredTypeIdentifiers UTI-CONFORMS-TO "${type}").@count >= 1).@count >= 1`;
  for (const type of ['public.data', 'com.apple.property-list', 'public.url']) for (const params of [{ url: 'url' }, { text: 'text' }]) {
    assert.equal(appleActivationRule(owned(params, predicate(type))), predicate(type));
    assert.equal(problems(owned(params, predicate(type))), '', `${type} ${JSON.stringify(params)}`);
  }
});
