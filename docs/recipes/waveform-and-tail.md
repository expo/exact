# Recipe: a voice note's waveform and a bubble's tail

Drawings in a row are one `svg` each: a waveform is one `path` app.ts writes, not 40 boxes.

```contract
shape Note
  id: string
  wave: string
  duration: string

component Voice
  resource notes = loadNotes() as shape Note[] else empty()
  view
    list virtualized=true appearance="none" flex=1 min-height=0
      for n in notes key=n.id
        row align-items="flex-end" padding="3px 12px"
          row align-items="center" gap=8 padding="8px 12px" border-radius="18px 18px 4px 18px"
            background-color="-exact-fill" testId=`note-${n.id}`
            image "symbol:play" aria-label="Play" font="-exact-headline"
            svg viewBox="0 0 160 28" width=160 height=28 aria-hidden="true" testId=`wave-${n.id}`
              path d=n.wave fill="-exact-secondary-label"
            text n.duration font="-exact-caption1" color="-exact-secondary-label"
          svg viewBox="0 0 8 13" width=8 height=13 aria-hidden="true" testId=`tail-${n.id}`
            path d="M0 0 C0 6 2 11 8 13 H0 Z" fill="-exact-fill"
```

```ts
import type { Answer, Result, Sources } from './app.contract.d.ts';

export const grants = '';

/** One bar per level (0 to 1), 2 wide every 4, centred on 28: a single path. */
const wave = (levels: number[]) => levels.map((v, i) => {
  const h = 4 + 24 * v, y = (28 - h) / 2;
  return `M${i * 4} ${y}h2v${h}h-2z`;
}).join('');
const notes: Result<'loadNotes'> = [
  { id: '1', wave: wave(Array.from({ length: 40 }, (_, i) => Math.abs(Math.sin(i / 3)))), duration: '0:23' },
];
const sources: Sources = { loadNotes: () => notes };
export const answer: Answer = (source, args, store, storage, native) => sources[source](args, store, storage, native);
```

```contract-test
test "a note shows its waveform, its duration and its tail"
  clock data
  expect tree has "wave-1"
  expect tree has "tail-1"
  expect text "note-1" == "0:23"
```

- One `path` per waveform is one node to lay out and paint, where 40 boxes are 40; the
  row's height stays its own, so a virtualized list measures it before it draws.
- Shape the path once, in app.ts, when the note loads; the view does no arithmetic.
- The tail is a second `svg` beside the bubble, whose corner on that side is nearly square
  (`4px`), so the two meet without overlapping: `-exact-fill` can be translucent, and an
  overlap draws darker. Theirs: the `svg` before the bubble, the small corner bottom-left,
  and `transform="translate(8, 0) scale(-1, 1)"` on the `path`, which keeps it in the viewBox.
- `fill` takes a role (`-exact-fill`), as `background-color` does, so both follow dark mode.
