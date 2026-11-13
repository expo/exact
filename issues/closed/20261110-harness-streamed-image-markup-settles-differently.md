# A streamed harness reply with `<img>` markup settles into different blocks than the same reply parsed whole

**Status:** Closed
**Resolution:** The harness parses replies with `Html::LITERAL`, so every raw HTML line is one literal block whether a reply is parsed whole or settled block by block.
**Systems:** apps/harness/data (settle.rs, markdown.rs), apps/markdown/parse
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

`apps/harness/data/src/settle.rs` (at `dd6f5dcd`) cuts a streaming reply into groups and parses each finished group alone. Its header says why that is sound (`settle.rs:6-9`): "The parser keeps no state from one block to the next, so parsing a finished block's lines alone gives exactly the blocks the whole reply gives there."

That is not true of raw HTML images, which the harness parses with the reader's default (`markdown.rs:50`; `parse` uses `Html::GITHUB`, `apps/markdown/parse/src/lib.rs:238`):

- `html_images` reads image markup over several lines up to the next blank one (`lib.rs:789`), and the elements it leaves open stay open across blocks (`open`, `going`, `closes_open`, `lib.rs:261-380`).
- A line that starts with an image and goes on with text opens a paragraph that continues on the next lines (`inline_image`, `lib.rs:381,889`).

`groups` treats every line that starts with `<` as a block of one line (`settle.rs:443-444`).

## The failure

Each reply streamed seven bytes at a time through `Reply::push` and `finish`, against `markdown::blocks(reply)`:

| Reply | Whole | Settled |
|---|---|---|
| `<img src="https://x.test/a.png"`⏎`  alt="The logo">` | `[image: The logo]` | `<img src="https://x.test/a.png"` and `alt="The logo">` as text |
| `<p align="center">`⏎`<img src="…" alt="Logo">`⏎`</p>` | `[image: Logo]` | `[image: Logo]`, then a paragraph `</p>` |
| `<img src="…" alt="Logo"> Exact`⏎`is a framework.` | one paragraph | two paragraphs |

The tail is drawn one way while it streams and the transcript keeps the other once the group settles.

## Verification

Ran a test in `settle.rs` over the three replies; each differed as shown. The same test streamed 20,000 random replies built from Markdown lines without raw HTML (headings, lists, quotes, fences, tables, rules) at three token sizes; none differed.

## Direction

A terminal draws no image, so the harness can parse replies with `Html::LITERAL` (`markdown_parse::parse_with`): every `<` line is then one literal block in both paths, and the header's claim holds.

## Related

None found. QUEUE's "find across a run boundary" is the LLP reader's.
