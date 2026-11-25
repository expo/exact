// LLP 1188 D4: check's `edge` and `fold`, on the boxes measured on an iPhone Duo (iOS 27.1), open under a side bar.
import { expect, test } from 'bun:test';
import { judge, line } from './agent-check.mjs';

const OPEN = { x: 0, y: 0, w: 951, h: 669, safe: { x: 0, y: 82, w: 867, h: 553 } };
const BOOK = [{ x: 455.5, y: 0, w: 40, h: 669 }], LAPTOP = [{ x: 0, y: 455.5, w: 669, h: 40 }];
const control = (id, x, y, w, h, more) => ({ id, owner: 0, control: 'button', sized: 'platform', x, y, w, h, ...more });
const text = (id, x, y, w, h, more) => ({ id, owner: 0, text: 'Text', size: 17, x, y, w, h, ...more });
const found = (items, obs = {}) => judge({ viewport: OPEN, owners: [OPEN], items, ...obs }, null, { host: 'ios' })
  .filter((f) => f.kind === 'edge' || f.kind === 'fold').map((f) => `${f.kind} ${f.id}`).sort();

test('edge: what lies outside its owner\'s safe area, where no scroller can bring it in', () => {
  expect(found([
    control(1, 796, 619, 55, 34), // a floating button, 18 points into the home indicator's 34
    control(2, 796, 585, 55, 50), // at the safe area's end
    control(3, 16, 640, 835, 29, { room: { bottom: 400 } }), // a row with more of its list below it
    control(4, 16, 640, 835, 28, { room: { bottom: 3, top: 900 } }), // and one at the end of a list that leaves it there
    control(5, 860, 300, 60, 34, { room: { bottom: 400, top: 80 } }), // under the side bar, which scrolling up and down leaves there
    control(6, 0, 560, 867, 109), // a bar's fill, drawn to the owner's end with its middle clear
    control(7, 796, 639, 55, 30), // at the owner's end, its middle in the indicator's area
    control(8, 0, 0, 951, 669), // a map
    control(9, 796, 619, 55, 34, { cover: { kind: 'bar', name: 'UITabBar' } }), // covered, which says so
    control(10, 796, 619, 55, 34, { cover: { kind: 'bar', name: 'UITabBar' }, disabled: true }), // which `covered` leaves out
    control(11, 796, 619, 55, 34, { clip: { x: 796, y: 619, w: 55, h: 10 } }), // what its clips leave of it is clear
    control(15, 700, 619, 55, 34, { vis: { x: 700, y: 619, w: 0, h: 0 } }), // under a bar's height, beside its glass
    control(12, 796, 619, 55, 34, { owner: undefined }), // UIKit's own
    text(13, 16, 640, 300, 20), text(14, 300, 640, 40, 20, { of: 1 }), // a caption, and a button's own label
  ])).toEqual(['edge 1', 'edge 10', 'edge 13', 'edge 15', 'edge 4', 'edge 5', 'edge 7']);
  expect(found([control(1, 796, 619, 55, 34)], { keyboard: 405 })).toEqual([]); // the keys are over the indicator
  expect(found([control(1, 796, 0, 55, 100), control(2, 400, 619, 55, 34)], { keyboard: 660 })).toEqual(['edge 1', 'edge 2']); // an accessory bar's height is not
});

test('fold: what a division crosses and no scroller holds, by what its clips leave of it', () => {
  const items = [
    control(1, 210, 410, 217, 35), control(2, 440, 410, 217, 35), // a timer's Start, and its Reset across the fold
    text(3, 307, 273, 253, 113), // its time
    text(4, 16, 100, 835, 20, { ink: { x: 16, y: 100, w: 200, h: 20 } }), // a short line in a box as wide as the page
    control(5, 16, 200, 835, 52, { scrolls: true, room: { bottom: 40 } }), // a row
    control(9, 16, 260, 835, 52, { scrolls: true }), // and one of a form that fits
    control(10, 440, 320, 44, 44, { scrolls: true }), text(11, 383, 320, 188, 80, { scrolls: true }), // whose button lies in the fold, and whose time crosses it
    control(6, 0, 0, 951, 669),
    control(8, 440, 560, 217, 35, { cover: { kind: 'alert', name: 'a system alert' } }),
    control(7, 440, 500, 217, 35, { cover: { kind: 'node', by: 9, name: 'View' } }), // a banner over its middle does not mend it
  ];
  expect(found(items, { divisions: BOOK })).toEqual(['fold 10', 'fold 11', 'fold 2', 'fold 3', 'fold 7']);
  expect(found(items)).toEqual([]); // flat
  expect(found([control(1, 440, 410, 217, 35)], { divisions: [{ x: 455.5, y: 0, w: 40, h: 300 }] })).toEqual([]); // past the division's end
  const tall = { x: 0, y: 0, w: 669, h: 951, safe: { x: 0, y: 82, w: 669, h: 786 } };
  expect(found([control(1, 111, 526, 217, 34), text(2, 208, 389, 253, 113)], { viewport: tall, owners: [tall], divisions: LAPTOP })).toEqual(['fold 2']);
});

test('what a scroller can bring out from under a control that stays is no overlap', () => {
  const overlaps = (more, button = {}) => judge({ viewport: OPEN, owners: [OPEN], items: [control(1, 796, 585, 55, 50, button), text(2, 700, 590, 140, 20, more)] }, null, { host: 'ios' })
    .filter((f) => f.kind === 'overlap').length;
  expect(overlaps({ room: { bottom: 60 } })).toBe(0); // a row passing under a floating button
  expect(overlaps({ room: { top: 900, bottom: 10 } })).toBe(1); // the last one, which no scrolling leaves clear of it
  expect(overlaps({ x: 800, w: 100, room: { bottom: 60 } })).toBe(1); // nor one that ends past the safe area
  expect(overlaps({})).toBe(1);
  expect(overlaps({ control: 'button', sized: 'platform', room: { bottom: 60 } })).toBe(0); // whichever of two controls is first
  expect(overlaps({ id: 0, control: 'button', sized: 'platform', room: { bottom: 60 } })).toBe(0);
  expect(overlaps({ room: { bottom: 60 } }, { room: { bottom: 60 } })).toBe(1); // the two move together
});

test('a control\'s label goes with it over what scrolls under it', () => {
  const overlaps = (row, label) => judge({ viewport: OPEN, owners: [OPEN], items: [
    control(1, 796, 585, 55, 50), text(3, 806, 600, 35, 20, { of: 1, ...label }), text(2, 700, 605, 140, 20, row),
  ] }, null, { host: 'ios' }).filter((f) => f.kind === 'overlap').length;
  expect(overlaps({ room: { bottom: 60 } })).toBe(0);
  expect(overlaps({ room: { bottom: 30 } })).toBe(2); // enough for the label and not for the button
  expect(overlaps({ room: { bottom: 60 } }, { of: 9 })).toBe(1); // another text stays in its way
  expect(overlaps({ room: { bottom: 60 } }, { of: 9, room: { bottom: 60 } })).toBe(1);
});

test('each says what it found and where to read what to write', () => {
  const said = judge({ viewport: OPEN, owners: [OPEN], divisions: BOOK, items: [control(1, 796, 619, 55, 34), control(2, 440, 410, 217, 35)] }, null, { host: 'ios' })
    .map((f) => line(f, new Map([[1, { props: { testId: 'add' } }], [2, { props: { testId: 'reset' } }]])));
  expect(said).toEqual([
    '  edge     add [796,619 55×34]: 18 pt of it lie outside the safe area, which ends 34 pt from the bottom; place it in the safe area (contract vocab edges)',
    '  fold     reset [440,410 217×35]: the fold crosses it (x 456 to 496 pt); keep it on one side of the fold (contract vocab edges)',
  ]);
});

test('a control passing its scroller\'s end is judged by its box', () => {
  const targets = (more) => judge({ viewport: OPEN, owners: [OPEN], target: 44, items: [control(1, 16, 620, 400, 60, { sized: 'author', reach: [[16, 620, 416, 635]], ...more })] }, null, { host: 'ios' })
    .filter((f) => f.kind === 'target').length;
  expect(targets({ passing: true })).toBe(0);
  expect(targets({})).toBe(1);
  expect(targets({ passing: true, w: 24 })).toBe(1); // narrow wherever it is
});

test('a text scrolled under a control that stays is read where the control is not', () => {
  const add = control(1, 796, 585, 55, 50), save = control(1, 16, 583, 835, 44);
  // White, and blue where the control is
  const contrast = (c, row) => {
    const data = new Uint8Array(OPEN.w * OPEN.h * 4).fill(255);
    for (let y = c.y; y < c.y + c.h; y++) for (let x = c.x; x < c.x + c.w; x++) data.set([0, 136, 255], (y * OPEN.w + x) * 4);
    return judge({ viewport: OPEN, owners: [OPEN], items: [c, text(2, 40, 593, 807, 18, row)] }, { width: OPEN.w, height: OPEN.h, data }, { host: 'ios' })
      .filter((f) => f.kind === 'contrast').map((f) => f.detail.bg);
  };
  const ink = [60, 60, 67, 0.6], room = { bottom: 60 };
  expect(contrast(save, { fg: ink, room })).toEqual([]); // a label over the page, under Save's fill
  expect(contrast(save, { fg: ink })).toEqual(['#0088ff']); // drawn there for good
  expect(contrast(add, { fg: [200, 200, 200, 1], room })).toEqual(['#ffffff']); // too light for the page, though its end is under Add
});
