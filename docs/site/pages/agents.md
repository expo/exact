# Agent resources

Each section below is one way for an agent to read or use these docs, with the command, URL or header it needs.

## Start prompt

Give your agent one file before it builds an app with Exact. The home page has a copy button for this prompt:

```text
{{START_PROMPT}}
```

For a website (pages served to a browser), the guide is [Start here: a website with Exact](../../start-here-site.md) in place of [Start here: building an app with Exact](../../start-here.md). Everything else in the docs is lookup.

## Markdown pages

Add `.md` to a page's URL to get the page as Markdown: `/recipes/money` is `/recipes/money.md`, and the home page is `/index.md`. Each page's HTML names its Markdown with `<link rel="alternate" type="text/markdown">`.

`bun exact.mjs serve dist` (the site's `bun start`) also answers a page's own URL with its Markdown when the request's `Accept` header prefers `text/markdown`: `/recipes/money` then returns the content of `/recipes/money.md`, with `Vary: Accept`. Only `bun run build` writes the `.md` files. When a page is asked for after a source changed, `serve` builds the site again without them. Until `bun run build` runs again, `/recipes/money.md` is not found and `/recipes/money` returns HTML. A static host does not read the header, so add `.md` there.

The deployed site reads `Accept` the same way. At a page's canonical URL, `ChatGPT-User`, `Claude-User` and `Perplexity-User` get the Markdown even if the request does not prefer it. Each of these agents fetches one page for a person in a chat. A crawler such as `GPTBot` or `ClaudeBot` gets the HTML by default. Both versions of the page carry `Vary: Accept, User-Agent`.

Each Markdown page starts with frontmatter (`title`, `description` and `modificationDate`) and a pointer to `/llms.txt`. Its links go to other Markdown pages, or to the file on GitHub.

## llms.txt

`/llms.txt` lists every page with its title, its Markdown URL and a one-line description. The pages are grouped by section, nested as the guides nest. Each page's HTML names it in its head: `<link rel="llms-txt" href="/llms.txt">`. On a host that reads the site's `_headers` file, such as Cloudflare Pages or Netlify, each response also names it in two headers: `Link: </llms.txt>; rel="llms-txt", </sitemap.xml>; rel="sitemap"` and `X-Llms-Txt: /llms.txt`. The `Link` header names `/sitemap.xml` too, the list of page URLs for search engines.

A path with no page returns a 404 that names `/llms.txt`: as Markdown to a request that prefers Markdown, and on the deployed site as JSON to a request that prefers `application/json`.

## sitemap.md

`/sitemap.md` lists the same pages by section, nested as the guides nest. Each line has the page's Markdown URL, its type (Guide, How-to, Reference), the date it last changed, a summary, and its prerequisites: the page to read before it.

## Agent catalog

`/.well-known/ard.json` is a catalog in the format of the [Agentic Resource Discovery](https://agenticresourcediscovery.org/spec/) specification. It lists the files an agent can start from: `/llms.txt`, `/sitemap.md` and this page's Markdown. Each entry has a URL, a media type, a description and sample questions. `/.well-known/ai-catalog.json`, the path of the specification's predecessor, serves the same file.

## robots.txt

`/robots.txt` allows every crawler, and it states a content signal for all crawlers and for each AI crawler it names: `search=yes, ai-input=yes, ai-train=yes`. Search indexing, use as input for AI answers, and AI training are all permitted.

## Open in your agent

Each page has these actions, at the top of the page and in the rail beside it:

- **Copy Markdown:** copies the page as Markdown.
- **View as Markdown:** opens the page's `.md` in a new tab.
- **Claude, ChatGPT and Cursor:** open a new chat that reads the page.
- **Claude Code and Codex:** open the app with a prompt that reads the page.

## WebMCP tools

Each page offers six tools to an agent that controls the browser, through [WebMCP](https://webmachinelearning.github.io/webmcp/) (`document.modelContext`). The site has no MCP server: an agent without a browser reads the Markdown pages and `llms.txt`.

| Tool | Input | Result |
| --- | --- | --- |
| `search_docs` | `query`, `limit` (1 to 20) | The matching sections as JSON, each with a `title` and a `path` |
| `get_page_markdown` | `path`, `section` | The page as Markdown, or only the parts under the headings that match `section` |
| `get_page_info` | `path` | The page's `path`, `title`, `description`, `modificationDate` and `headings` as JSON |
| `get_site_overview` | None | The content of `/llms.txt` |
| `list_sections` | `area`, `section` | The sidebar's areas with the names of their sections, or the pages of one area or section, as JSON |
| `navigate_page` | `path` | Opens the page in the tab. Available while the search dialog is closed. |

Every input is required. An empty `path` means the current page, an empty `section` means the whole page or every section of the area, and an empty `area` lists the areas. Pass a `path` starting with `/`, as in `/recipes/money`, or a URL of these docs, as `llms.txt` writes them. `navigate_page` preserves the URL fragment.

A call answers with an object. When `outcome` is `ok`, `value` holds the result as a string. For input the tool cannot answer, such as a path with no page, `outcome` is `failed` and `value` says why. Only `navigate_page` changes what the tab shows.

The site declares these tools as any Exact app does: [Recipe: tools an agent or the system calls](../../recipes/tools.md).

## Terminal lookups

An app that `bun scripts/exact.mjs new` makes has an `AGENTS.md` and a `CLAUDE.md` that tell its agent where to look. From the app's folder, these commands answer without the website:

```sh
bun exact.mjs docs <heading words>      # a section of a guide
bun exact.mjs contract vocab <name>     # a tag or property
bun exact.mjs pattern                   # the patterns; pattern <name> writes one, tested
bun exact.mjs help                      # every verb
```

These guides are for lookup by section, not for reading front to back:

- [Contract for agents](../../contract-for-agents.md)
- [Agent pitfalls](../../agent-pitfalls.md)
- [Contract grammar](../../contract-grammar.md)
- [Reference](../../reference.md)

## Feedback

Each Markdown page has a feedback block below its title: a command and an HTTP request that report an error on that page. In an app's folder, the agent can also record what went wrong:

```sh
bun exact.mjs feedback note "<what happened>"
```

`bun exact.mjs feedback status` says whether notes are sent. `EXACT_FEEDBACK=0` keeps them on the machine.
