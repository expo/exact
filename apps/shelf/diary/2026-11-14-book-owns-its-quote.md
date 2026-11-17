surfaces: web, macos, ios, linux
agent: Claude
task: move Add Quote from Photo's text read into the book screen (LLP 1035.005.004 D6, #476)

### Rough
`quoted` was the root's, so a photo's read outlived the book screen it was
started on: going Back before Vision answered still ran `showQuote`, and the
quote sheet opened over the library for a book no longer on screen.

### Lean in
`BookScreen`, the book route's whole screen, declares `mutation quoted … then
detected` and its picker. `detected` calls the root's `showQuote(lines)`,
passed as `showQuote=showQuote(e.params.id)`, which sets the sheet's book and
lines and pushes it. The root keeps the sheet (`quoteLines` is now its state)
and the languages to read in, passed as `languages`. The picker's id names
the stack entry, so a book opened over another book has its own.

### Needed
- a child's own mutations: provided by exact2 (LLP 1035.005.004 D6)

### Checkpoints
- "a photo's lines open the quote sheet, and leaving the book before they
  come opens none" (macOS and iOS, which read text) picks
  `packages/text-detection/fixture/quote.png` twice: the first read opens the
  sheet, the second, left by Back, does not. Against the root-owned `quoted`
  the second opened the sheet over the library.
- The web test of no Add Quote is now `on web, linux, windows`: macOS and iOS
  read text, so it failed there.
