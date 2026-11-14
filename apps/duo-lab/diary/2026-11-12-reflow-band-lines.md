surfaces: ios
agent: Claude Code (Claude Opus 5.5)
task: Find why a long document's scroll made runs of late frames, and fix it.

### Rough

A frame-health report from an iPhone Duo, open, showed eight late updates in a row on Reflow, each about 20 ms, almost all of it in Exact's host phases, with the scroll pump asking for frames. The report gives the host phases only as a total, so the cause came from the code and the simulator. Reflow's document was one `markup="markdown"` text, one paragraph about 700 lines long at 919 points, which the rasterizer paints in bands. Each band's job laid out and measured every line of the paragraph, then kept the band's rows. A band's job now lays out only the lines that can paint in it (`TextRasterizer.lines(_:in:)`). Its pixels are the same; its bitmap can be narrower, as wide as its own lines' ink rather than the document's widest line.

Measured on the iPhone Duo simulator, open, with three real-touch flings down the document (415 to 624 UI updates a drive, three drives each):

- one text, every line: 6 to 11 updates over 8.5 ms, the longest 22.5 to 24.5 ms; 6 to 12 bands painted on the main thread and 3 to 8 waits on a worker.
- one text, the band's own lines: no update over 5 ms; 2 or 3 bands painted on the main thread and no waits.
- one text per block, every line: no update over 4 ms; no block is tall enough to be painted in bands.
- later, on a quieter machine (load 4 to 7): one text per block, no update over 3.5 ms; one text, the band's own lines, no update over 4 ms.

Painting only the port on the main thread while a worker's band is still on its way did not help on its own (up to 48 slow updates in a drive) and added nothing to the band's own lines.

Reflow's document is now also one text per Markdown block, in a column. The Markdown pieces put a short line between blocks (an 8-point newline with no line height of its own), but an Apple line box starts at the text's strut (CSS 2.1 §10.8.1), so with `line-height="26px"` that blank is 26 points, and so is the column's gap.

One text and the blocks do not draw the same pixels, and the blocks are right. `CTTypesetterCreateWithAttributedString` on a string longer than 10,240 UTF-16 units (measured; the header says only "unreasonable effort") gives a typesetter that does not shape: no kerning, no ligatures, no bidi reordering, no Arabic joining. With no options, `CTTypesetterCreateWithAttributedStringAndOptions` returns NULL for such a string; `kCTTypesetterOptionAllowUnboundedLayout` shapes it in full. The one text, expanded, is 48,868 units, so its lines were not kerned: 1 to 2.7 points wider than a block's, and one of the 155 blocks broke onto a fifth line where alone it takes four. The same holds for any Exact text that long: an 11,053-unit text laid its Arabic line out left to right, unjoined, on the iOS simulator. The unbounded option would shape it, but a whole-string typesetter then shapes all of it each time one is made: 6.3 ms a band on this Mac, against 0.3 ms unshaped and 0.5 ms for the band's own paragraphs. So apps get the rule instead (`docs/recipes/long-text.md`): one text per paragraph or block, never split inside a paragraph.

A split has its own trap: Exact's Markdown numbers footnotes within one text, so split blocks each mark theirs [1]. The recipe says so.

Thirteen apps that Claude Haiku built from the docs, each with a document longer than the limit, made their blocks where the recipe says and split no paragraph; the one paragraph longer than the limit stayed whole. They met two limits of Exact's Markdown that the reader docs did not state: LLP 1045 D2's dialect has no indented code and no second paragraph in a list item, so an indented code block is drawn as a paragraph and an item's second paragraph at the left margin. The docs now say so. `verify` also reported ordered and task lists as clipped at the largest text size, where nothing is cut off, and the first agent drive of an app with no iOS build stopped as stale after building it. Both are in `QUEUE.md`.

An XCTest run alone lays out `markup="markdown"` as its source unless it calls `ExactMarkdown.install()`: `MarkdownLink.installed` is static, and set only by the classes that install it. The band test installs it, checks the text was expanded, and puts back what was there.

Getting a measured fling took about 20 minutes:

- A real touch cannot tap the tab bar; `--touch drag` keeps drags real and taps the agent's.
- After `clock settle`, `perf frames capture start` answered `virtual-clock`; leaving it out, and waiting in wall time, worked.
- A drag that ends off the screen is the scroll view's own, not a touch, and a second fling during the first's deceleration did not move.
- After a reinstall the simulator showed the cover display again; `scripts/duo/hinge_helper.c` opened it.

What would have helped: the screen report splitting Exact's host phases, as a trace's late frames do.

The Duo smoke's `lab` leg sometimes fails two Combinations checks after a fold, "the keyboard went down" and "dismiss ended editing": in 1 of 2 runs of the code before this change, and in 2 of 6 runs with it or half of it.

### Lean in

`perf frames health scroll` and `perf frames track` on a simulator gave per-update work and per-frame offsets for real flings, enough to compare code variants without a phone.

### Needed

none

### Checkpoints

- iOS build: smooth
- driving the app: rough (see Rough)
