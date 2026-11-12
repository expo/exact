# Recipe: fetch with loading, error and retry

```contract
shape Feed
  owner: string
  posts: string[]

component App
  state account = "default"
  resource feed = loadFeed(account) as shape Feed else empty()
  action reload
    refresh feed
  view
    column
      button press=reload testId="refresh"
        text "Refresh"
      scroll refresh=reload refreshing=(pending(feed) and feed.owner == account) flex=1 min-height=0
        if pending(feed)
          progress aria-label="Loading" testId="loading"
        if failed(feed)
          text "Couldn't load the feed." testId="error"
          button press=reload testId="retry"
            text "Try Again"
        if feed.owner == account
          if feed.posts.length == 0 and not failed(feed)
            text "No posts yet."
          for p, i in feed.posts key=p
            text p testId=`post-${i}`
```

`empty()` sets `owner` to `""`; use nonempty IDs. The owner check hides A's rows
while B loads or fails. A failed refresh keeps A's rows; an empty answer clears them.

```ts
import type { Answer, Result } from './app.contract.d.ts';

const API = 'https://jsonplaceholder.typicode.com';
export const grants = `net.fetch ${API}`;

function isPost(p: unknown): p is { title: string } {
  return p !== null && typeof p === 'object' && 'title' in p && typeof p.title === 'string';
}

export const answer: Answer = async (_source, [owner]): Promise<Result<'loadFeed'>> => {
  const response = await fetch(`${API}/posts?_limit=10`, { exactTimeout: 10000 });
  if (!response.ok) {
    throw new Error('The feed request failed.');
  }
  const body: unknown = await response.json();
  if (!Array.isArray(body) || !body.every(isPost)) {
    throw new Error('The feed response is invalid.');
  }
  return { owner, posts: body.map((p): string => p.title) };
};
```

```contract-test
test "a failed refresh retains rows and retries"
  clock data
  fail fetch "https://jsonplaceholder.typicode.com"
  tap "refresh"
  expect tree has "post-0"
  clock data
  expect tree has "post-0"
  expect tree has "error"
  pass fetch "https://jsonplaceholder.typicode.com"
  tap "retry"
  clock data
  expect tree missing "error"
```

`fail fetch` rejects before `tap` returns. To assert loading, hold a reply with a
[test server](rest-backend.md), which can also supply an HTTP status/body.

For private data, gate by principal and auth generation, including after an await
before caching: A → B → A must reject the first A's answers. Await prerequisites
in the source; dependencies do not sequence requests ([guide](../contract-for-agents.md#data-requests-and-side-effects)).

**Across launches:** reading `native.available` or calling `native.call` makes a
resource read device state; its answer can be kept and shown before the next fetch.
Resolve a launch endpoint once in a mount mutation, then pass it as a resource
argument; keep native access out of fetch sources. Copy [a native module](native-module.md).
`with` controls boot admission, not persistence. `state.resourceStatus` explains retained answers.
