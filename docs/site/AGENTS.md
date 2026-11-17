<!-- exact:begin (exact new writes this block; bun exact.mjs update rewrites it) -->
# site: an Exact app

`app.contract` is the view, `app.ts` the data module, `app.test.contract` the tests, `app.json`
the manifest: set `app.name` and `app.id` before building. The app uses Exact at
`../..` by path (`EXACT` overrides it).

This file is already in your instructions. Start from the nearest pattern: `bun exact.mjs
pattern` lists them and `pattern <name>` writes one, tested. ../../docs/start-here-site.md (a site) or
../../docs/start-here.md is the only required reading. `bun exact.mjs contract vocab <name>` answers a
name (none: the card); `bun exact.mjs docs <heading words>` a guide's
section. Run `bun exact.mjs verify` when the features work, and `verify ios|android` at the end for a
phone app. When an iPhone app is ready to try, run `bun exact.mjs share` and send its link;
repeat for each change the person should see. `bun exact.mjs help` lists the verbs, including
app.json's `"commands"`.
`bun exact.mjs update` refreshes files. Native verbs (`ios`, `mac`, `linux`, `android`, `windows`)
name their platform in app.json on first use and build in a generated workspace;
`update --without <platform>` stops shipping it.
`bun ../../scripts/exact.mjs setup --check` names what a native build lacks.

Generated, so don't edit: `exact.mjs` and this block.

## Feedback (required)

When a doc was wrong or missing, a workaround was needed, something took several tries or a
command confused you, record it: `bun exact.mjs feedback note "<what happened>"`. It attaches
exact2's commit and the last exit code, never output or code. `bun exact.mjs feedback status` says
whether notes are sent and whether to keep a diary; under `never`, skip all of this.
<!-- exact:end -->

## Serving

`bun run build` generates the pages and their Markdown, indexes and redirects. `bun start`
uses the framework server: loopback by default, `--lan` for other devices, and `--port <n>`
for a chosen port. Its automatic rebuild does not run this site's postbuild step; after
editing sources, stop the server, run `bun run build`, then start it again. A page request
between the edit and the full build removes the generated Markdown and indexes.

## Search

The search dialog (Cmd+K) asks the Algolia app and index in `constants/search.ts` with its
search-only key, so it works the same locally and on the deployed site. As on docs.expo.dev,
the index is the Algolia Crawler's: it crawls the deployed site from `sitemap.xml` on a schedule
and writes DocSearch records (`hierarchy.lvl0` to `lvl6`, `content`, `url`), so nothing here
holds a write key. Its configuration lives in the Algolia dashboard (Data sources, Crawler).
