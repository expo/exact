# A `>` in an attribute is written unescaped by the Bun render, and its hero image scan ends the tag there

**Status:** Closed
**Resolution:** `dom.js` writes `<` and `>` in an attribute value as `&lt;` and `&gt;`, as `document.rs` and HTML's serializer do, so `heroImage` reads every attribute of a tag and preloads the image the page shows first.
**Systems:** host/web-js/dom.js, host/web-js/render.mjs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`dom.js` says it serializes "by the rules `host/web/src/document.rs` writes (escaping, void elements, boolean attributes)" (`host/web-js/dom.js:1-6`). The Rust writer escapes `<`, `>`, `&` and a carriage return everywhere, and `"` in an attribute (`host/web/src/document.rs:1269-1280`). `dom.js` escapes only `&`, `"` and a carriage return in an attribute (`dom.js:8`), so a `<` or `>` in an attribute's value is written as it is.

A browser reads that correctly. The render server's own scan does not: `heroImage` reads a tag as `<name` then every character that is not `>` (`host/web-js/render.mjs:254`), so the tag ends at a `>` inside a quoted value and the rest of its attributes are read as text. An `img` cut before its `src` is passed over (`render.mjs:259-260`), and an element cut before its `inert` or `visibility: hidden` is read as shown (`render.mjs:268`).

`host/render/src/page.rs:445` scans the same way, and is safe only because its input has no bare `>` in a tag.

## The failure

```
routes nav
  home "/" render=build

component App
  view
    column
      image "assets/one.png" alt="1 > 0" width=10 height=10
      image "assets/two.png" alt="second" width=10 height=10
```

`bun host/web-js/build.mjs <app> --render js` writes

```
<link rel="preload" as="image" href="assets/two.png">
<img class="c2" alt="1 > 0" src="assets/one.png" decoding="async" …>
<img fetchpriority="high" class="c2" alt="second" src="assets/two.png" decoding="async" …>
```

The native render entry, over the same plan, writes

```
<link rel="preload" as="image" href="assets/one.png">
<img fetchpriority="high" alt="1 &gt; 0" src="assets/one.png" class="c2">
<img alt="second" src="assets/two.png" class="c2">
```

which is the page's first image (LLP 1048.007 D5). The page still shows both images; the wrong one is fetched first.

## Verification

Built the app with `--render js`, ran `target/host-dev/caltrain-render --plan dist/app.plan --shell dist/shell.html --build` over the same plan, and diffed the two documents: the lines above.

Not run: a covered route whose attribute holds a `>` before its `inert`.

## Direction

Escape `<` and `>` in `dom.js`'s attribute values, as `document.rs` does; `heroImage` then reads what `hero_image` reads.

## Related

`issues/20261110-streamed-page-has-no-hero-image-preload.md` is the native stream path never calling `hero_image`. `host/web-js/page-style.mjs:152-166` reads quoted values and is not affected.
