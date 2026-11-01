// Duo Lab's data module (LLP 1078 D8): the words and rows behind the four
// screens. Everything is generated deterministically here — no network, no
// storage, no clock — so the bake answers every resource once and the app is
// the same on every host and at every pose.
import type { Answer, Sources } from './app.contract.d.ts';

export const appId = 'com.exact.duolab';
export const grants = '';

// A small linear congruential generator: the same document on every bake.
function lcg(seed: number): () => number {
  let s = seed >>> 0;
  return () => {
    s = (Math.imul(s, 1664525) + 1013904223) >>> 0;
    return s / 4294967296;
  };
}

const WORDS = (
  'fold hinge panel segment posture viewport inset band margin layout continuous folded ' +
  'open book half closed cover inner display scale raster text cache reflow column pane ' +
  'list detail sheet keyboard route push back tab scroll offset frame commit settle clock ' +
  'web kernel runner host agent plan bake contract style length width height left right ' +
  'top bottom row gap padding border radius shadow image caption feed note draft'
).split(' ');

const PLACES = ['North Shore', 'Ochre Dunes', 'Alpine Water', 'Winter Ridge'];
// What each picture shows (assets/provenance.json), a few ways each, so the rows read as a collection's captions.
const CAPTIONS = [
  ['Turquoise water breaks around a granite headland.', 'A small cove below golden coastal grass.', 'A winding Atlantic shoreline in cool morning light.', 'Surf around the pale rocks of the headland.'],
  ['A single dune ridge curves toward the horizon.', 'Long blue shadows across ochre sand.', 'An empty desert under a pale apricot sky.', 'Morning light on sculpted dunes, no footprints.'],
  ['Dark pines reflected in a still alpine lake.', 'Warm grey rocks at the edge of blue-green water.', 'Granite peaks beyond a calm lake in late summer.', 'A quiet morning on the water below the mountains.'],
  ['A snowy ridge above lavender clouds.', 'The crest catches a thin ribbon of sunlight.', 'Layered mountains over a sea of cloud.', 'Winter light on a high ridge.'],
];
const ASSETS = ['assets/north-shore.png', 'assets/ochre-dunes.png', 'assets/alpine-water.png', 'assets/winter-ridge.png'];
const PEOPLE = ['ada', 'bram', 'cleo', 'dev', 'esme', 'finn', 'greta', 'hollis', 'ines', 'jude'];

function sentence(next: () => number, words: number): string {
  const out: string[] = [];
  for (let i = 0; i < words; i++) out.push(WORDS[Math.floor(next() * WORDS.length)]);
  const s = out.join(' ');
  return s[0].toUpperCase() + s.slice(1) + '.';
}
function paragraph(next: () => number, sentences: number): string {
  const out: string[] = [];
  for (let i = 0; i < sentences; i++) out.push(sentence(next, 8 + Math.floor(next() * 10)));
  return out.join(' ');
}

// ------------------------------------------------------------- Fold's items
// What a fold does to a layout, each a title, a one-line summary and a body
// of short paragraphs (docs/recipes/foldables.md, Apple's iPhone Duo talks).
const ITEMS: [string, string, string[]][] = [
  ['Book and laptop', 'Folded partway, the inner display becomes two segments.', [
    'Held like a book, the two segments sit side by side. Held like a laptop, one sits above the other.',
    'An app learns which from its viewport: the posture is folded, and two segments count across or down. Opened flat, the display is one segment again.',
  ]],
  ['The band at the hinge', 'Keep controls off the fold.', [
    'Between the segments runs a band that is hard to read and to touch.',
    'Scrolling content, such as this list or a long document, may run across it. A button, a field or a pane of controls should not.',
  ]],
  ['A list and its detail', 'This screen is one: the list on one segment, its detail on the other.', [
    'On a book fold the list takes the leading segment and the detail the trailing one. On a laptop fold the list sits above and the detail below.',
    'The chosen item stays chosen as the device folds and unfolds.',
  ]],
  ['Reading order', 'The first pane goes on the leading segment.', [
    'Segments count from the left. In a right-to-left language the leading segment is the right one, so the first pane goes there.',
  ]],
  ['A pane of controls moves', 'A form keeps its width and moves as a whole.', [
    'A pane of controls keeps about one panel\'s width. On one segment it is centred; on a book fold it moves to the trailing segment, near the outer display as the device closes, where the system moves an alert.',
    'It scrolls only where its controls do not fit.',
  ]],
  ['Controls below the fold', 'On a laptop fold, controls go on the bottom segment.', [
    'The bottom segment rests on the table, where the fingers are. A sheet there is capped at the top of that segment.',
  ]],
  ['Typing across a fold', 'A fold keeps what a field holds.', [
    'Folded like a laptop, the keyboard covers the bottom segment.',
    'The text of a field survives a change of posture. The system may still end editing as the device moves between its displays.',
  ]],
  ['The outer display', 'Closed, the Duo is a narrow phone.', [
    'The outer display is one segment. The same screens lay out for it, with their bars along its side.',
  ]],
  ['Bars on the side', 'Bars that sit at the top and bottom of an iPhone move to the side.', [
    'On the outer display and on the inner display in landscape, navigation and tab bars move into a strip along the side of the device, where they are easier to reach.',
    'The strip stays on the same side of the device in right-to-left languages, and the content adapts around it.',
  ]],
  ['Safe areas', 'Content stays clear of the bars and the display\'s corners.', [
    'The side strip and the bars inset the safe area. A background may reach under them; text and controls stay inside.',
  ]],
  ['Sheets on the inner display', 'A sheet is centred on the inner display.', [
    'On the inner display a sheet is centred by default and its toolbar stays horizontal. On the outer display a sheet\'s toolbar runs down its side.',
  ]],
  ['The margins are the system\'s', 'Content keeps to the layout margins of the column it is in.', [
    'On a book fold the detail column keeps the system\'s 20-point margin from the band, and a screen\'s title and content keep to the same margins.',
  ]],
];
function items() {
  return ITEMS.map(([title, summary, body], i) => ({ id: i + 1, title, summary, body: body.join('\n\n') }));
}

// ---------------------------------------------------------- Images' rows
function imageRows() {
  const rows = [];
  for (let i = 0; i < 300; i++) {
    const k = i % ASSETS.length;
    const said = CAPTIONS[k][Math.floor(i / ASSETS.length) % CAPTIONS[k].length];
    rows.push({ id: i + 1, asset: ASSETS[k], caption: `${PLACES[k]}: ${said}` });
  }
  return rows;
}

// --------------------------------------------------------- Reflow's document
function document() {
  const next = lcg(951);
  // The screen's title is the document's heading
  const parts: string[] = ['A long document for the text caches: headings, paragraphs, lists and code, laid out at 951 points and again at 466.', ''];
  for (let h = 1; h <= 14; h++) {
    parts.push(`## ${h}. ${sentence(next, 3).slice(0, -1)}`, '');
    for (let p = 0; p < 4; p++) parts.push(paragraph(next, 5 + Math.floor(next() * 4)), '');
    parts.push(`### ${sentence(next, 2).slice(0, -1)}`, '');
    for (let b = 0; b < 5; b++) parts.push(`- ${sentence(next, 6 + Math.floor(next() * 8))}`);
    parts.push('', '```', `fn segment(x: u8, y: u8) -> Rect { grid[y * cols + x] } // ${h}`, `let posture = if any_division_active { "folded" } else { "continuous" };`, '```', '');
    for (let p = 0; p < 2; p++) parts.push(paragraph(next, 4 + Math.floor(next() * 4)), '');
    parts.push(`> ${sentence(next, 12)}`, '');
  }
  return { body: parts.join('\n') };
}

// ------------------------------------------------------------- Reflow's feed
function feed() {
  const next = lcg(669);
  const rows = [];
  for (let i = 0; i < 200; i++) rows.push({ id: i + 1, who: PEOPLE[i % PEOPLE.length], body: paragraph(next, 1 + Math.floor(next() * 3)) });
  return rows;
}

const sources: Sources = {
  items: () => items(),
  imageRows: () => imageRows(),
  document: () => document(),
  feed: () => feed(),
  keyboardRows: (): { id: number }[] => Array.from({ length: 40 }, (_, id): { id: number } => ({ id })),
  arrangementSupport: (): { available: boolean } => ({ available: false }),
};
export const answer: Answer = (source, args, store, storage, native): ReturnType<Answer> => {
  if (source === 'arrangementSupport' && native?.available) {
    try { return native.call({ op: 'arrangementSupport' }) as { available: boolean }; }
    catch { return { available: false }; }
  }
  return sources[source](args, store, storage);
};
