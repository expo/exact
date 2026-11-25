# @exact/clipboard

A Paste button sends a mutation whose data source declares `clipboard.read`.
Call `await readText()` for the first plain-text item or `await read()` for the
list of items. Each item has `types` and `getType(type)`. On Android, the
window needs input focus or the read rejects with `NotAllowedError`. For an
image copied from another app’s content provider, `await item.getType("image/png")`
returns a sanitized PNG under `app:/tmp/`. One item can also carry
`text/plain`. An empty clipboard gives `""` or `[]`. An image alone gives
`""` to `readText()`.

Each input image must fit in 64 MiB and 64,000,000 pixels. The returned text
shares a 1 MiB UTF-8 budget and the returned PNGs share 64 MiB. Exceeding a
shared budget rejects the read with `DataError`. A failed representation leaves
the others available in clipboard order. If the only image is invalid or
oversized, the read rejects with `DataError`. If it times out, the read rejects
with `NotReadableError`.

Provider calls share a 1.5-second deadline. Android can open four image
streams at once and decodes one bitmap at a time to bound memory. When every
open stalls, queued items get a share of the remaining time. A second image
read during the first rejects with `NotReadableError`. Provider-backed
`readText()` has four workers of its own. Inline text needs no worker.

An agent drive reads the text copied in its own session by default.

See the [clipboard reference](../../docs/reference.md#clipboard-images) for
writes, platform formats and the Apple paste permission (LLP 1133.006 D6–D7).
