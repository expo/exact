# The JS build writes the app's name into `<title>` unescaped, and a name holding `<` leaves every page rendered under Bun untitled

**Status:** Closed
**Resolution:** Fixed by 6e97307a, which writes the name into the shell's title through `attr` (`&lt;3`), so the render server reads it back whole.
**Systems:** host/web-js/build.mjs, host/web-js/render.mjs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`build.mjs` writes the shell with `<title>${manifest.name}</title>` (`host/web-js/build.mjs:802`). The same template escapes the language and the icon's attributes with `attr` (`:784-785`, `:799`); the name goes in as written.

Two readers of that title then get it wrong:

- The render server reads the name back out of the shell with `/<title>([^<]*)<\/title>/` and unescapes it (`host/web-js/render.mjs:89`), then escapes it where it writes a page's title, `og:site_name`, `og:title` and `twitter:title` (`render.mjs:92-100`, `:208`). A name holding `<` does not match the pattern, and the name is the empty string.
- In the shell itself (a route drawn in the browser), the HTML parser decodes a character reference in a title (HTML, "RCDATA state"), including the legacy ones that need no semicolon. Nothing sets the title afterwards for a page that declares none.

The native render entry is given the name as `--name` (`build.mjs:986`) and is not affected.

## The failure

`app.json` with `"name": "I <3 NY"`, and a `render=build` route whose page declares no `head title`. `bun host/web-js/build.mjs <app> --render js` writes

```
shell.html:  <title>I <3 NY</title>
index.html:  <title></title> <meta property="og:site_name" content=""> <meta property="og:title" content=""> <meta name="twitter:title" content="">
```

The native render entry, over the same plan and shell, writes `<title>I &lt;3 NY</title>` and the three metas with `I &lt;3 NY`.

The tab title in Chromium:

| `"name"` | the shell | the page rendered under Bun |
|---|---|---|
| `I <3 NY` | `I <3 NY` | empty |
| `<Code>` | `<Code>` | empty |
| `Dollars&cents` | `Dollars¢s` | `Dollars&cents` |
| `Fish & Chips` | `Fish & Chips` | `Fish & Chips` |

Expected: the app's name in every cell.

## Verification

Built an app under each name with `--render js` and with `--render none`, read `shell.html` and `index.html`, ran `target/host-dev/caltrain-render --plan dist/app.plan --name 'I <3 NY' --shell dist/shell.html --build`, and read the tab's title in the pinned Chromium with JavaScript off and, 1.5 s after loading, with it on: the same both times.

## Direction

Escape the name where `build.mjs` writes the title (`&`, `<`), which is what `render.mjs` already assumes when it reads it.

## Related

`issues/20261110-page-splice-reads-dollar-patterns.md` is another unescaped splice in `build.mjs` (a `$` pattern in a replacement string). Nothing names the title.
