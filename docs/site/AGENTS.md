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

## Publishing

`.github/workflows/docs.yml` builds the site after a push to `main` that changes `docs/` and
publishes that build to the Cloudflare Pages project `exact-docs`. For a pull request with the
`docs-preview` label it only builds, and `docs-preview-publish.yml` publishes that build at
`https://pr-<number>.exact-docs.pages.dev` until the pull request closes or loses the label
(`docs-preview-cleanup.yml`). A job that builds runs the branch's code and holds no secret; a job
that holds the Cloudflare token uploads a build's files without running them.
`bun run build:pages` makes what is uploaded: `dist.pages`, which is `dist` with each page at
`<path>.html` (Pages redirects `/start` to `/start/` for a `start/index.html`), a `_headers` within
Pages' 100 rules, and the worker. The worker (`worker/index.ts`) answers a request that prefers
`text/markdown` as `bun start` does, sends the Markdown to `ChatGPT-User`, `Claude-User` and
`Perplexity-User` when they do not ask for it, and marks every host but the site's origin `noindex`. `bun
run test:worker` tests the worker, and `bun run test:pages` the tree under `wrangler pages dev`.

`constants/redirects.ts` holds the redirects. A moved page goes in `redirects`. A redirect or a
rewrite for any other path goes in `nonPageRules`, as the fields of a rule in [Pages'
syntax](https://developers.cloudflare.com/pages/configuration/redirects/): status 200 makes a
rewrite, and `from` can have a splat. The build writes both lists to `dist/_redirects`. Pages
applies those rules. `bun start` does not: it answers a moved page's old path with a fallback
page, and the path of any other rule with a 404.

## Prompts

A page shows the prompt in `prompts/<its source>`, after its introduction: `prompts/recipes/tabs.md`
is the prompt on `docs/recipes/tabs.md`'s page. A recipe with no file of its own shows
`prompts/recipes.md`, with `{{SOURCE}}` replaced by the recipe's path, and `bun run build` names
each recipe that does. To give a new recipe its own prompt, add `prompts/recipes/<name>.md` in the
form of the others: the task, the questions only the person can answer, the recipe's steps, a
test, the platform checks, the feedback step and what to report.

## Search

The search dialog (Cmd+K) asks the Algolia app and index in `constants/search.ts` with its
search-only key, so it works the same locally and on the deployed site. As on docs.expo.dev,
the index is the Algolia Crawler's: it crawls the deployed site from `sitemap.xml` on a schedule
and writes DocSearch records (`hierarchy.lvl0` to `lvl6`, `content`, `url`), so nothing here
holds a write key. Its configuration lives in the Algolia dashboard (Data sources, Crawler).
