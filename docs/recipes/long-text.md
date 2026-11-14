# Recipe: a long document or article

A document longer than a screen (an article, a help page, a Markdown note, a
transcript) is one `text` per block: a paragraph, a heading, a whole list, a
whole code block, a quote. The blocks sit in a `column` in the `scroll`, keyed:

```contract
shape Block
  id: number
  text: string
shape Article
  blocks: Block[]

component Reader
  resource article = loadArticle() as shape Article
  view
    scroll height="100%"
      column padding=16 gap=12
        for block in article.blocks key=block.id
          text block.text markup="markdown"
```

Make the blocks in `app.ts` or on the server, at the boundaries the source
already marks:

- **Markdown:** a block is what a Markdown parser calls one. A fenced code block
  or a list can hold blank lines and is still one block, so a split at every
  blank line is wrong.
- **Plain text:** a block is a paragraph, as the text separates them: by blank
  lines, or by single line breaks where it has no blank lines. A stanza's or an
  address's lines stay together in one text. Line breaks can be `\n`, `\r\n` or
  U+2029.
- **Never inside a paragraph.** A paragraph longer than Apple's limit (below)
  stays one text: set without shaping is better than broken.
- **Footnotes** are numbered within one text, so split blocks each count from
  [1]. Write the numbers into the source before splitting, or keep a document
  with footnotes in one text.

```ts
import type { Answer, Sources } from './app.contract.d.ts';
export const grants = '';
const blocks = [
  '# Folding phones',
  'A paragraph is one text, however long it is.',
  '- A list is one block\n- with all of its items',
  '```\nlet code = "one block, blank lines and all"\n\nlet more = code\n```',
];
const sources: Sources = {
  loadArticle: () => ({ blocks: blocks.map((text, id) => ({ id, text })) }),
};
export const answer: Answer = (source, args, store, storage, native) => sources[source](args, store, storage, native);
```

Why not one text for the whole document:

- **Apple's text layout declines a long string.** Core Text's `CTTypesetter.h`
  calls proper layout of some text "unreasonable effort" and makes it opt-in
  (`kCTTypesetterOptionAllowUnboundedLayout`). Past a length it does not
  document, iOS and macOS set a text without kerning, ligatures or Arabic
  joining, and run its right-to-left text left to right. The length was 10,240
  UTF-16 units on iOS 27.1 and macOS 27.2; it can change, and this rule does
  not depend on it.
- **Layout starts again at each paragraph:** bidi
  ([UAX #9](https://www.unicode.org/reports/tr9/), P1) and line breaking
  ([UAX #14](https://www.unicode.org/reports/tr14/)) do, so a paragraph alone
  lays out as it does in a longer text. A split inside one loses kerning,
  ligatures and joining across it, and its lines cannot wrap across it.
- **Work follows the blocks:** each text is measured, cached and painted on
  its own, and texts out of view are not painted.

One Markdown text leaves a blank line between its blocks; here the column's
`gap` is that space.
