# Recipe: wait for a click in a partitioned frame

In one mutation, arm the listener, click, then await its promise.

`bun exact.mjs add @exact/browser-automation`. In `app.ts`

```ts
import type { Answer, Sources } from './app.contract.d.ts';
import { addPreloadScript, evaluate } from '@exact/browser-automation';

export const grants = 'browser.automation';
const frame = { id: 'tab', sandbox: 'automation' };

const armClick = () => evaluate({ ...frame, expression: `(() => {
  const go = document.querySelector('#go');
  if (!go) throw Error('Missing #go');
  window.clicked = new Promise(resolve => {
    go.addEventListener('click', () => resolve('clicked'), { once: true });
  });
  return true;
})()` });

const click = () => evaluate({ ...frame, userActivation: true,
  expression: "document.querySelector('#go').click()" });

async function waitForClick() {
  const { result } = await evaluate({ ...frame, expression: 'window.clicked', awaitPromise: true });
  if (String(result) !== 'clicked') throw Error('No click');
  return { text: 'clicked' };
}

const sources: Sources = {
  setup: () => addPreloadScript({ ...frame, functionDeclaration: '() => {}' }).then(() => ({})),
  runPair: async () => {
    await armClick();
    await click();
    return waitForClick();
  },
};
export const answer: Answer = (source, args, store, storage, native) =>
  sources[source](args, store, storage, native);
```

Check `exactPage().iframePartition`. Use your URLs and a unique id. Match `e.url`
after redirects. On Android, navigate after registration. Without isolated
worlds, expect `NotSupportedError`.
Keep the document and enabled `<button type="button" id="go">` mounted. Rearm per click or navigation.

```contract
shape Done

shape Heard
  text: string

component Tab
  state err = ""
  mutation ready as shape Done then openPage
  mutation heard as shape Heard
  action failed(e: FrameNavigation)
    err = e.message
  action loaded(e: FrameNavigation)
    err = ""
    if ready == none
      if not pending(ready)
        send ready = setup()
    else if e.url == "https://a.test/form"
      send heard = runPair()
  action openPage
    navigate("tab", "https://a.test/form")
  view
    column
      iframe id="tab" src="https://a.test/" partition="persist:browser" navigatesuccess=loaded navigateerror=failed
      text err
      for item, i in [failure(ready), failure(heard)] key=i
        text (match item { case some(f) => f.message, case none => "" })
      text (match heard { case some(r) => r.text, case none => "" })
```

For an external click replace `runPair` with

```text
arm: async () => { await armClick(); return {}; },
waitForClick,
press: async () => { await click(); return {}; },
```

In `loaded` send `armed = arm()` at the form. Once armed, either send can run first.
Show `armed` and `acted` failures. For a manual click omit `press`,
`acted` and its send.

```text
  mutation armed as shape Done then listen
  mutation acted as shape Done
  action listen
    send heard = waitForClick()
    send acted = press()
```
