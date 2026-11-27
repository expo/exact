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

`empty().owner` is `""`. Use nonempty IDs. A [failed refresh](progressive-tabs.md) keeps the owner’s rows. Empty replies clear them.

```ts
import type { Answer, Result } from './app.contract.d.ts';

const API = 'https://jsonplaceholder.typicode.com';
export const grants = `net.fetch ${API}`;

function isPost(p: unknown): p is { title: string } {
  return p !== null && typeof p === 'object' && 'title' in p && typeof p.title === 'string';
}

export const answer: Answer = async (_source, [owner]): Promise<Result<'loadFeed'>> => {
  const response = await fetch(`${API}/posts?_limit=10`, { exactTimeout: 10000 });
  if (!response.ok) throw new Error('The feed request failed.');
  const body: unknown = await response.json();
  if (!Array.isArray(body) || !body.every(isPost)) throw new Error('Invalid feed');
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

`fail fetch` rejects before `tap` returns. Test loading by holding a reply on a
[test server](rest-backend.md). To supply a response, use
`answer fetch "<prefix>" with 200 {"text":"Hello"}`.

For private data, check principal and auth generation again after an await before
caching. A → B → A invalidates A's first answer. Await dependencies in the
[source](../contract-for-agents.md#data-requests-and-side-effects).

Reading `native.available` or calling `native.call` can retain an answer across
launches. Keep [native access](native-module.md) out of fetch sources. Inspect
retention in `state.resourceStatus`. `with` controls boot admission. For a
launch-supplied endpoint, see `contract vocab env`.

On the web, `net.fetch self` admits `fetch('/_data/page.json', {credentials: 'omit'})`
on each tenant. Pass its validated absolute page URL to the renderer, or set
`app.origin` at build time. Native sources need absolute URLs and their own network grants.
