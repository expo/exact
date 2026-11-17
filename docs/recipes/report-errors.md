# Recipe: send a data module's errors to your server

A data module hears its errors as a page does. Listen for `error` and
`unhandledrejection` on the module's global, and send each one to your server from
the listener.

```contract
component App
  mutation saved as shape string
  action save
    send saved = saveNote()
  view
    column padding=16
      button press=save testId="save"
        text "Save"
```

```ts
import type { Answer, Result } from './app.contract.d.ts';

const API = 'https://api.example.com';
const REPORTS = 'https://errors.example.com/report';
export const grants = `net.fetch ${API} ${REPORTS}`;

function report(kind: string, reason: unknown): void {
  const text = reason instanceof Error ? `${reason.name}: ${reason.message}` : String(reason);
  // In the listener itself, before any await, with its failure caught.
  fetch(REPORTS, { method: 'POST', body: JSON.stringify({ kind, text }) }).catch(() => {});
}

addEventListener('error', (event) => report('error', event.error));
addEventListener('unhandledrejection', (event) => report('unhandled rejection', event.reason));

export const answer: Answer = async (): Promise<Result<'saveNote'>> => {
  try {
    const response = await fetch(`${API}/notes`, { method: 'POST', body: 'note' });
    if (!response.ok) throw new Error(`The save failed: HTTP ${response.status}.`);
    return 'saved';
  } catch (e) {
    // A failed answer is not an `error` event: report it, then fail the answer.
    reportError(e);
    throw e;
  }
};
```

- **What reaches a listener.** `reportError(e)` and a throw that no code caught (in a
  `queueMicrotask` callback, an abort listener or an answer's timer) are `error`
  events. A rejection that no code handled is an `unhandledrejection` event. An answer
  that throws or rejects only fails (`failure(saved)` in the Contract), so call
  `reportError` in its `catch` to hear it too.
- **What a listener may send, natively.** A plain HTTP request made in the listener
  itself, with `method`, `headers`, `body`, `signal`, `redirect`, `cache` or the
  `exactTimeout` options. No answer waits for it: the answer that called `reportError`
  gives its value first. These are refused:
  - a fetch after an `await`, in the reply's `.then`, or in a microtask the listener
    queued;
  - a stream or socket;
  - a fetch at bake, during the module's evaluation, or on a worker placement;
  - a seventeenth report while sixteen are in flight.

  A report that fails does not set off another.
- **On the web build** the listener is the page's: it hears every error on the page,
  and a fetch it makes later is sent too. A module written as above behaves the same
  on both.
- **What the logs keep.** Every report still reaches `logs` (`data: unhandled
  rejection: …` or `console: Uncaught …`), whatever a listener does. One that no
  listener cancels with `event.preventDefault()` also reaches the platform's log
  (Console.app, logcat or standard error), as a page's console prints it. The full
  rule is in the [reference](../reference.md).
