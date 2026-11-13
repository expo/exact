# Recipe: a read declared in Contract (a GET, a GraphQL query)

JSON from one endpoint needs no data module: declare the request as a `source` in
`app.contract`. With no `app.ts`, the native build links no JavaScript engine.

```contract
shape Current
  temperature_2m: number
  is_day: option<number>

source weather(latitude: number, longitude: number) as shape Current
  url "https://api.open-meteo.com/v1/forecast?latitude={latitude}&longitude={longitude}&current=temperature_2m"
  pointer "/current"

component Now
  resource now = weather(52.52, 13.41) as shape Current
  action again
    refresh now
  view
    column padding=20 gap=8
      if failed(now)
        text "The forecast couldn't be reached." testId="error"
        button press=again testId="retry"
          text "Try Again"
      else if not current(now)
        progress aria-label="Loading" testId="loading"
      text `${round(now.temperature_2m)}°` testId="temp"
```

- **`{param}`** is the argument as text, percent-encoded: `"a b & c"` goes as
  `a%20b%20%26%20c`. In the URL or a header it is a `string`, `number` or `bool`.
- **The origin is the grant**, written out before any `{…}`: no `grants` line.
- **The answer** is the JSON at `pointer` (`/data/items/0`), read field by field: other
  fields are left out, an absent `option` is `none`, anything else missing is `shape`.
  Rename in a `derive`.
- **A failure** is a TypeScript `fetch`'s: `offline`, `timeout`, `refused`; `error` for a
  status outside 200–299.

## GraphQL, with a token the data module keeps

```contract
shape Repo
  name: string

source repo(owner: string, name: string) as shape Repo
  url "https://api.github.com/graphql"
  header "Authorization" "Bearer {secret:github.token}"
  graphql "query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) { name } }"
  pointer "/data/repository"

component App
  resource r = repo("expo", "expo") as shape Repo else empty()
  view
    text r.name testId="repo"
```

A reply's `errors` fail it with `error` (`extensions.code` is `failure(r).reason`). The
secret needs `secret.keep github.token` in `app.ts`'s grants; while it is absent nothing
is sent (`refused`). A `send` takes `method "POST"` and `body json`.

Token refresh, pagination, offline queues, overlays, streams and uploads stay in `app.ts`.
