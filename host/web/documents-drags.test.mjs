// The web's drag tracker (documents-glue.js `hearDrags`; LLP 1094.001 D2): a node a drop still waiting for its files
// was over holds a newer drag's enter until that drop lands, a leave of the newer drag cancels it, and the newer
// drag's own drop hears its enter first.
import { expect, test } from 'bun:test';

/** The tracker over one zone that hears drags while `allowed()`, with the document's drag events in hand. */
async function tracker(allowed = () => true) {
  const listeners = {}, heard = [], before = { document: globalThis.document, exact: globalThis.exact };
  const zone = { parentNode: null, isConnected: true };
  try {
    globalThis.document = { addEventListener: (type, listener) => { listeners[type] = listener; } };
    globalThis.exact = {};
    await import(`./documents-glue.js?drags=${Date.now()}${Math.random()}`);
    for (const kind of ['dragenter', 'dragleave']) globalThis.exact.documents.hearDrags(zone, kind, allowed, () => heard.push(kind));
  } finally {
    Object.assign(globalThis, before);
  }
  const event = (target) => ({ target, dataTransfer: { types: ['Files'] } });
  const send = (type, target) => { const e = event(target); listeners[type](e); return e; };
  return { zone, heard, send };
}

test('a drop that waits holds a newer drag\'s enter at its nodes until it lands', async () => {
  const { zone, heard, send } = await tracker();
  send('dragenter', zone);
  const first = send('drop', zone);
  first.exactTaken = true;
  first.exactWaits();
  send('dragenter', zone);
  expect(heard).toEqual(['dragenter']);
  first.exactLands();
  expect(heard).toEqual(['dragenter', 'dragenter']);
});

test('a newer drag that leaves while the drop waits is neither entered nor left', async () => {
  const { zone, heard, send } = await tracker();
  send('dragenter', zone);
  const first = send('drop', zone);
  first.exactTaken = true;
  first.exactWaits();
  send('dragenter', zone);
  send('dragleave', zone);
  first.exactLands();
  expect(heard).toEqual(['dragenter']);
});

test('a newer drag that drops while the drop waits hears its enter before its drop, and once', async () => {
  const { zone, heard, send } = await tracker();
  send('dragenter', zone);
  const first = send('drop', zone);
  first.exactTaken = true;
  first.exactWaits();
  send('dragenter', zone);
  expect(heard).toEqual(['dragenter']);
  send('drop', zone).exactTaken = true;
  expect(heard).toEqual(['dragenter', 'dragenter']);
  first.exactLands();
  expect(heard).toEqual(['dragenter', 'dragenter']);
});

test('a held enter at a node the waiting drop\'s action disabled is dropped, with no leave after it', async () => {
  let allowed = true;
  const { zone, heard, send } = await tracker(() => allowed);
  send('dragenter', zone);
  const first = send('drop', zone);
  first.exactTaken = true;
  first.exactWaits();
  send('dragenter', zone);
  allowed = false;
  first.exactLands();
  send('dragleave', zone);
  expect(heard).toEqual(['dragenter']);
});
