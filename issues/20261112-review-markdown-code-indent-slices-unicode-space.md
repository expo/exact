# Unicode whitespace in an indented code block panics the Markdown readers

**Status:** Open
**Systems:** Markdown parser, LLP reader, harness streaming replies
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `apps/markdown/parse/src/lib.rs:489–491` removes a code
fence's indentation by taking the smaller of two byte counts and slicing
the code line there. Both counts come from Unicode-aware `trim_start()`.
The minimum need not be a UTF-8 character boundary. A fence indented by
one ASCII space followed by a code line beginning with U+2003 EM SPACE
panics at byte 1 of that three-byte character. Two spaces also panic.
This is ordinary valid text, including when copied from another editor.

The harness duplicates that logic in `settle.rs:283–285`. Its incremental
code path reaches the same panic while a model reply streams. The agent
thread has no panic cleanup around `round()` (`agent.rs:73–103`), so a
panic there leaves the session's turn installed; later prompts queue
behind a worker that has exited. Interrupt marks the turn cancelled but
depends on that worker to call `release()` and end ownership.

Reproduced by linking the existing `markdown_parse` library and calling
its public `parse()` function with an indented text fence and a code line
starting with EM SPACE:

```text
one ASCII space before fence; EM SPACE in code: panic=true
two ASCII spaces before fence; EM SPACE in code: panic=true
start byte index 1 is not a char boundary; it is inside '\u{2003}'
```

A second probe includes the actual harness `settle.rs`, `markdown.rs`,
and `highlight.rs`, with plain transcript records and the actual record
helpers. It first pushes an indented fence and an ASCII code line, then
pushes an EM SPACE line. This exercises the incremental path directly:

```text
actual Reply::push incremental code path: panic=true
.../apps/harness/data/src/settle.rs:285:10
```

Sources and output are in `/tmp/exact-review-20261112/markdown-indent/`.
Run `python3 run.py` there for the parser probe and `python3 stream-run.py`
for the streaming probe. Both catch the panic in their own process. No
app, terminal session, or model was launched.

Strip only the indentation characters the Markdown grammar permits,
preserving other leading Unicode characters as code content. Keep byte
cuts on character boundaries in both implementations. Cover one- and
two-space fences, EM SPACE and NBSP at the start of code lines, and
streaming cuts before and after those characters. The full and
incremental parsers must agree without panicking.

Checked all open and closed filesystem issues and the full bodies of the
275-issue GitHub snapshot. The closed quote-prefix indentation issue and
harness streamed-image settling issue concern different parsing paths.
No report covers this code-block UTF-8 slice.
