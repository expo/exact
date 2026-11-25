# Script messages

Data code holding `browser.automation` can receive events from an injected script on macOS, iOS and Android. Pass a channel in a named sandbox.

```ts
import { addPreloadScript, type ScriptMessage } from '@exact/browser-automation';

export const grants = 'browser.automation';

export const tools = {
  'script.message'({ channel, data, source }: ScriptMessage) {
    if (channel === 'playback') console.log(source.realm, source.origin, data);
  },
};

export function watchPlayback() {
  return addPreloadScript({
    id: 'player',
    sandbox: 'float-track',
    arguments: [{ type: 'channel', value: { channel: 'playback' } }],
    functionDeclaration: `post => {
      document.addEventListener('pause', e => post({ event: 'pause', t: e.target.currentTime }), true);
    }`,
  });
}
```

| Operation | Arguments or delivery |
| --- | --- |
| `addPreloadScript` | Each `arguments` entry is a `ChannelValue`. The function receives the channels at document start. Each posts to the session that supplied it. Closing the session removes all its preloads, including those without arguments. On Android register the sandbox before navigating. |
| `callFunction` | A top-level `ChannelValue` becomes a post function at that position among ordinary JSON arguments. A realm target selects its existing sandbox. |
| `script.message` | The handler receives the channel name, JSON data and the posting world's realm, frame id (`context`) and host-supplied origin. An admitted post keeps its source through navigation, partition replacement and unmount. A view held for redisplay can still post. Once the frame releases that view, the host drops new posts before queuing and journals the first drop per package session. Use `target: { realm }` for a wallet reply after approval. A reply to a replaced document answers `no such frame`. |
| Admission | Only the host event calls the reserved `tools["script.message"]` receiver. The receiver needs `browser.automation` and runs with the runner’s `device` origin. A channel without a named sandbox, or on the web, answers `NotSupportedError`. |
| Bounds | More than 65,536 UTF-8 bytes of serialized JSON throws a catchable `QuotaExceededError`. One package session waits at least 50 ms after a handler finishes before starting the next one. Up to 32 posts wait. On overflow the oldest unsent post is discarded. The runner journals the first overflow and reports another only after the unsent queue has emptied. |

Exact accepts `{ type: "channel", value: { channel } }` and delivers plain JSON by value. It adds the posting document’s origin to BiDi’s source. [ChannelValue](https://w3c.github.io/webdriver-bidi/#type-script-ChannelValue) · [script.message](https://w3c.github.io/webdriver-bidi/#event-script-message)
