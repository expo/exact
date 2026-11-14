# Recipe: keep a desktop feed and release it before suspension

Gate the resource on execution eligibility, so hiding a desktop window keeps
its socket while a host's freeze transition releases it in the same commit.

```contract
shape Page
  frozen: bool

component App
  resource page = exactPage() as shape Page
  resource feed = events() when not page.frozen as shape string else empty()
  view
    text feed testId="feed"
```

```ts
import type { Answer } from './app.contract.d.ts';

export const grants = 'net.websocket wss://feed.example.com';
export const answer: Answer = (source) => {
  if (source !== 'events') throw new Error('Unknown source: ' + source);
  return fetch('wss://feed.example.com/events', {
    exactStream: (event) => event.type === 'message' ? String(event.data) : event.type,
  });
};
```

Replace the endpoint and grant with your service's. Closing the gate lets the
stream go; the host cancels its request and closes the socket. No cleanup task
has to finish. The last answer stays visible, and reopening the gate asks again
and opens a fresh socket. Resources default to kept answers; add `transient`
for sensitive feed data.

On macOS, Linux and Windows desktops `frozen` stays false even with a hidden window.
Android sets it when the Activity stops without a promoted foreground-service
claim, or when the last such claim ends while hidden; a started Activity clears
it. iOS and tvOS set it in the background unless active audio with the background
audio mode keeps execution eligible. A configured playback category alone grants
nothing; ending playback while hidden freezes the page. A signed-phone run remains owed.
On the web it follows Page Lifecycle `freeze`/`resume` where fired, else false.
A browser promises nothing after hidden: save as changes happen and handle
reconnection; no lifecycle notification guarantees a final send will finish.

Drive the transition with `prefer frozen true`, then `prefer frozen false`.
`prefer visibility-state hidden` alone should keep the stream open. This checks
the plan's policy; qualify the host's timing on a device before relying on it.
