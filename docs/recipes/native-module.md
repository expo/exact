# Recipe: a native module in Swift

Put an `ExactModule` in `modules/apple/*.swift`; `let exactModule` names its class.
No app.json entry.

Declare device access in the activating source's `grants`
([permissions](permissions.md)). `device.photos-add purpose.savePhoto` derives
`NSPhotoLibraryAddUsageDescription` from that strings key. Grants derive
entitlements too. Apple `infoPlist` and `entitlements` dictionaries refuse every
key; see [bundle metadata](../reference.md#apple-bundle-metadata).

**Kept answers:** a resource reading `native.available` or calling `native.call`
reads device state; its answer is kept across launches. Keep native access out of
fetch sources. Read launch configuration in a mount mutation and
pass it as a resource argument:

```swift
import Foundation

final class Launch: ExactModule {
    override func later(_ request: [String: Any], reply: ExactReply) {
        reply.send(["url": ProcessInfo.processInfo.environment["API_BASE_URL"]
            ?? "https://jsonplaceholder.typicode.com"])
    }
}
let exactModule: ExactModule.Type = Launch.self
```

```contract
shape Endpoint
  url: string

shape Feed
  posts: string[]

component App
  mutation endpoint as shape Endpoint
  derive api = match endpoint { case some(e) => e.url, case none => "" }
  resource feed = loadFeed(api) as shape Feed else empty()
  task launch mount
    after(1, configure)
  action configure
    send endpoint = launchEndpoint()
  view
    column
      if api == "" or pending(feed)
        progress aria-label="Loading"
      for p in feed.posts key=p
        text p
```

```ts
import type { Answer, Result, Sources } from './app.contract.d.ts';

export const grants = 'net.fetch https:';
const sources: Sources = {
  launchEndpoint: async (_args, _store, _storage, native) =>
    native?.available ? await native.later({}) as Result<'launchEndpoint'>
      : { url: 'https://jsonplaceholder.typicode.com' },
  loadFeed: async ([api]) => {
    if (!api) return { posts: [] };
    const response = await fetch(`${api}/posts?_limit=10`);
    if (!response.ok) throw new Error('Feed request failed.');
    const posts = await response.json() as { title: string }[];
    return { posts: posts.map(p => p.title) };
  },
};
export const answer: Answer = (source, args, store, storage, native) =>
  sources[source](args, store, storage, native);
```

`call` runs on main within 100 ms; mark it `throws` to refuse. `later(request)` awaits one
`ExactReply.send` or `fail` from any thread. The web implements only `later`, in
`modules/web/index.js`.

Drive with `clock +1`, then `clock data`. Rebuild Swift with `bun exact.mjs ios`; `print` reaches agent `logs`.

Set the environment on the simulator app (`id` in app.json):

```sh
SIMCTL_CHILD_API_BASE_URL=https://jsonplaceholder.typicode.com xcrun simctl launch --terminate-running-process <udid> <bundle-id>
```

Use the same prefix on `bun exact.mjs agent ios "clock +1" "clock data" state`.
`-API_BASE_URL value` sets `UserDefaults`, not the environment.
