# On the web, a clipboard image write reading an app file is ordered after the read, so an older copy can overwrite a newer one

**Status:** Closed
**Resolution:** A web clipboard write takes its place and becomes the write in flight before it reads its app file, so a later write, `cancel` or `destroy` during the read supersedes it.
**Systems:** packages/clipboard/web/index.js
**Severity:** P3
**Author:** Claude (Opus 5.5), from GPT-6 Astra's whole-project review (2026-11-10)
**Date:** 2026-11-10

Line references are at `46f3d608`.

## What is wrong

`write` supersedes any earlier write still decoding (LLP 1133.006 D2): it takes a number
from `latest` and becomes `inFlight` (`packages/clipboard/web/index.js:85-91`). For an
`image/png` given as an `app:/` path it first awaits the file
(`index.js:78`), and only then takes its number. While that read is in flight the write
holds no number and is not `inFlight`, so:

- a later write that finishes during the read is overwritten: the earlier write takes the
  newer number after its read, passes the check at `:93` and writes;
- `cancel(request)` (`:118`) and `destroy()` (`:119`) look at `inFlight` and miss it, so
  it writes after its operation has ended.

## To see it

Confirmed by reading the code only. Copy A: `write([{ 'image/png': 'app:/data/big.png' }])`.
While its file is read, copy B: `writeText('later')`, which completes. When A's read
finishes, A writes its image and the clipboard holds A, the older copy. The same holds
when A's operation is cancelled during the read.

## What a fix is

Take the number and become `inFlight` before the first `await`, and check `superseded`
after each await (the read and the decode), as `:93` does after decoding. A test with a
`files.read` that resolves after a second write.
