# Recipe: serving and hosting

## Serving

`bun exact.mjs web-build --out dist` builds `dist/`; `bun exact.mjs serve dist`
serves on 127.0.0.1:8765 (`--port`, else `PORT`) and rebuilds edits.
`npm run build` and `npm start` are aliases.
For API pages, start `bun server.ts` or set `API_BASE_URL`. Static hosts follow
`serve --help`. Upload hidden `.exact/` assets too (Wrangler included); see
[web sign-in callbacks](../contract-for-agents.md#web-sign-in-callback).

## Hosting below a path

Set `"host": { "web": { "base": "/app/" } }` in `app.json`, then rebuild.
The base defaults to `/` and starts and ends with `/`.
`bun exact.mjs serve dist` reads the base from the build and serves it at `/app/`.
On a static host, mount `dist/` there and apply its file and fallback rules.
Serve the generated `robots.txt` and `.well-known/` association files at the origin
root as well: crawlers, Apple and Android discover
them there. Merge them with existing policies on a shared origin. Exact's server
exposes these discovery files at the root; other requests outside
the mount return 404.
The server redirects `/app` to `/app/`, keeping its query; the client also normalizes
that root if a static host serves its shell directly.

Keep route patterns and locations app-relative: `book "/book/:id"`,
`path("book", id)`, `push(nav, "/add")` and a location passed through an action
parameter use the same paths as a root build. Entries and `navigate` events also
carry app-relative locations. The runtime removes the mount from incoming browser
locations and adds it to link hrefs and browser history. A route named `/app/item` opens at `/app/app/item`.
Destinations are canonicalized before mounting: `"item"` and `"/../item"`
both open `/app/item`. A data source's grant-free `fetch("/assets/file.txt")` reads
the bundled asset within the mount and the page's build generation. Images, video
posters and Markdown links follow the same boundaries. Canonical and social-image
paths receive the mount before they are made absolute against `app.origin`; explicit
absolute URLs are kept. `host.web.head` remains literal HTML: use `./assets/…` for
bundled files there, and absolute URLs for destinations outside the site.

The JS renderer supports build, request and cached pages under the base, including
install pages and the web manifest. An explicit manifest `scope` is mounted too;
without one, its scope is the directory of the mounted `start_url`. Use `--render js`
if the app has a Rust renderer. A one-off `render.mjs dist /item` and a driver's
`--url /item` also take app-relative locations.
Preferred native claims use the mounted scope, excluding mounted `/.exact/` where
supported. Links on `app.origin` lose the base at a segment boundary on cold and
warm delivery, keeping query and fragment. Reloads retain the mount; agent URL
navigation uses it too. Extension origins keep their paths. Rebuild binaries when
base or origin changes.
The wasm target, Rust renderer, signed update store (`deploy` and baked plans), auth
callbacks and web/native dev loop require `/`.
