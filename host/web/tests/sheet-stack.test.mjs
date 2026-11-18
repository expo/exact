import { hostFixtureFile as readFileSync } from './glue-fixture.mjs';
// LLP 1115.000 D4.8: a sheet over a sheet in a window 900 px wide or more, in Chrome, Firefox and WebKit, held to
// UIKit's form sheets on an iPad (iPadOS 27; the schema's `_metrics` holds the measurements). The neutral sheet and
// the navigation projection, which both web targets run, over routes as the hosts write them.
import { test, expect } from 'bun:test';
import { resolve } from 'node:path';
import { withLook, appleColors } from '../look.mjs';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { bundleHostModule } from '../../web-js/glue.mjs';

const read = (path) => readFileSync(new URL(path, import.meta.url), 'utf8');
const sheet = withLook(read('../index.html')).match(/<style>([\s\S]*?)<\/style>/)[1].replace(appleColors(), '');
// A page and a sheet over it, a card until a test writes its content; a test adds the sheets over that one.
const routes = `<div id="exact-root"><main id="nav" navigationKey="1" navigationBack="back" style="position:relative;height:600px">
  <div navigationKey="1"><header><h1>Inbox</h1></header></div>
  <div navigationKey="2" navigationPresentation="modal" id="first" style="position:absolute;inset:0"><header><h2>New</h2></header><button type="button">Add</button></div>
</main></div>`;
/** A page of those routes showing the sheet, projected as both web hosts project. A projection reads a stack in the
 * next frame, so a test looks a frame after it projects one. */
const stackPage = async (browser, options, source = read('../navigation.js')) => {
  const page = await browser.newPage(options);
  await page.setContent(`<!doctype html><style>${sheet}</style>${routes}<script type="module">${source}
    const host = document.getElementById('exact-root'), nav = document.getElementById('nav');
    navigation.connect(host, () => false, () => {});
    window.select = key => { document.getElementById('nav').setAttribute('navigationKey', key); navigation.project(host, () => {}); };
    window.unprojected = () => { navigation.project = () => {}; };
    window.settle = () => { for (const a of document.getAnimations()) a.finish(); };
    window.closeSheet = el => { if (!sheetLeaves(el)) el.remove(); };
    window.box = el => { const r = el.getBoundingClientRect(); return [r.x, r.y, r.width, r.height]; };
    // A dialog's layout: its body, content and action from its top, and whether a click reaches the action.
    window.layout = route => {
      const action = route.querySelector('header button'), body = route.querySelector('header').nextElementSibling;
      const top = route.getBoundingClientRect().y, a = action?.getBoundingClientRect();
      return action && { boxes: [body, body.firstElementChild, action].map(el => box(el).map((v, i) => i === 1 ? v - top : v)),
        reachable: [a.top + 1, a.y + a.height / 2, a.bottom - 1].every(y => action.contains(document.elementFromPoint(a.x + a.width / 2, y))) };
    };
    select('2'); window.ready = true;</script>`);
  await page.waitForFunction('window.ready === true');
  return page;
};

// iPadOS 27, two UIKit form sheets of one size: the covered one loses 16 pt per side and rises 10 pt, or as far as the
// 32 pt it keeps from its window's top lets it; the next is 20 pt shorter than it is alone, which for one with a
// preferred content size is that size, and its top stands 20 pt below the covered one's. So a card is 20 px shorter,
// and a dialog, 20 px taller than its content alone, is as tall as its content: its body and its content stand as
// they do alone, whatever its author wrote on its body (here each body's style, and what its padding and border add
// to its height), and its action is higher by what the stack took. The step is 16 px per side and that rise whatever
// the sheet's own style makes of its size: each such style, and the least height it allows. A minimum bounds what
// the stack takes from a sheet's height, and a maximum height bounds nothing, a sheet's being the look's. The 600 px
// window is one a card fills, where the covered card keeps its top.
const frames = [['', 0], ['min-height:710px', 710], ['min-height:720px', 720], ['min-height:800px', 800], ['max-height:300px', 0], ['min-width:700px', 0], ['max-width:400px', 0], ['aspect-ratio:1', 0]];
const dialogBodies = [['overflow:auto', 0], ['', 0], ['overflow:auto;padding:24px', 48], ['box-sizing:content-box;padding:24px;overflow:auto', 48], ['overflow:auto;padding:24px;border:3px solid transparent', 54]];
for (const engine of ['chromium', 'firefox', 'webkit'])
for (const [bodyHeight, bodyWrapped] of [[null, false], [2000, false], [200, false], [2000, true], [200, true]]) test(`${engine}: wide sheets step back like iPad form sheets (body ${bodyHeight}, component ${bodyWrapped})`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    for (const reducedMotion of ['no-preference', 'reduce']) {
      const page = await stackPage(browser, { viewport: { width: 1024, height: 1000 }, reducedMotion });
      for (const [width, height, root] of [[1024, 1000, 16], [900, 600, 20]])
      // A card in each frame; a dialog with each body, and one dialog in each frame but the ratio's, which is a card's.
      for (const [frame, least, bodyStyle, outer] of bodyHeight === null ? frames.map(f => [...f, '', 0])
        : [...dialogBodies.map(body => ['', 0, ...body]), ...(bodyHeight === 200 && !bodyWrapped ? frames.slice(1, -1).map(f => [...f, ...dialogBodies[0]]) : [])]) {
        await page.setViewportSize({ width, height });
        const { box: single, naturalHeight, alone } = await page.evaluate(({ root, bodyHeight, bodyWrapped, bodyStyle, outer, frame }) => {
          document.documentElement.style.fontSize = `${root}px`;
          const route = document.getElementById('first');
          route.style.cssText = `position:absolute;inset:0;${frame}`;
          route.innerHTML = bodyHeight === null
            ? '<header><h2>Sheet</h2></header><button>Open</button><div></div>'
            : `<header><h2>Dialog</h2><button style="margin-bottom:0">Save</button></header><div style="${bodyStyle}"><div style="height:${bodyHeight}px">Content</div></div>`;
          if (bodyWrapped) route.innerHTML = `<div style="display:flex;flex-direction:column">${route.innerHTML}</div>`;
          settle();
          const action = route.querySelector('header button');
          const naturalHeight = bodyHeight === null ? Infinity : 20 + bodyHeight + outer + route.querySelector('h2').getBoundingClientRect().height + action.getBoundingClientRect().height + parseFloat(getComputedStyle(action).marginTop);
          return { box: box(route), naturalHeight: Math.min(naturalHeight, 45 * root, innerHeight - 64), alone: layout(route) };
        }, { root, bodyHeight, bodyWrapped, bodyStyle, outer, frame });
        expect(single[3], `${engine} ${frame} ${bodyStyle} single height`).toBeCloseTo(Math.max(naturalHeight, least), 1);
        // Content that fits leaves the action clear, inside the card.
        if (bodyHeight === 200 && !frame) expect([alone.reachable, Math.max(...alone.boxes.slice(0, 2).map(b => b[1] + b[3])) <= alone.boxes[2][1], alone.boxes[2][1] + alone.boxes[2][3] <= single[3]]).toEqual([true, true, true]);
        // A body that scrolls leaves it so however tall its content, in a dialog a component wraps too.
        if (bodyHeight === 2000 && bodyStyle.startsWith('overflow:auto')) expect([`${engine} ${width} ${bodyStyle} component ${bodyWrapped}`, alone.reachable, alone.boxes[2][1] + alone.boxes[2][3] <= single[3]]).toEqual([`${engine} ${width} ${bodyStyle} component ${bodyWrapped}`, true, true]);
        for (const wrapped of [false, true]) {
          const got = await page.evaluate(async wrapped => {
            const below = document.getElementById('first'), nav = document.getElementById('nav');
            const listItem = el => { const item = document.createElement('div'); item.setAttribute('data-exact-list-item', ''); nav.append(item); item.append(el); return item; };
            if (wrapped) listItem(below);
            const above = below.cloneNode(true); above.id = 'above'; above.setAttribute('navigationKey', '3');
            const aboveItem = wrapped ? listItem(above) : null;
            if (!wrapped) nav.append(above);
            select('3');
            // Two frames, so the sheets' sizes have been observed once since the projection.
            await new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done))); settle();
            const css = getComputedStyle(below), shade = getComputedStyle(below, '::after');
            const out = { below: box(below), above: box(above), scale: parseFloat(css.scale), layout: layout(above),
              dim: [getComputedStyle(nav, '::before').opacity, shade.opacity, getComputedStyle(above, '::after').opacity],
              origin: css.transformOrigin.split(' ').map(parseFloat), size: [parseFloat(css.width), parseFloat(css.height)], radius: css.borderTopLeftRadius };
            closeSheet(above); select('2');
            await new Promise(requestAnimationFrame); settle();
            if (above.isConnected) {
              // A close keeps the stacked card's frame at the start of its slide.
              for (const a of above.getAnimations()) { a.pause(); a.currentTime = 0; }
              out.leaving = box(above);
            }
            out.restored = box(below);
            above.remove(); aboveItem?.remove();
            if (wrapped) { const item = below.parentElement; nav.append(below); item.remove(); }
            return out;
          }, wrapped);
          // The stack takes 20 px from a sheet's height, down to its own minimum. The covered sheet rises 10 px, or as
          // far as the window's 32 px margin lets it, and the upper sheet's top stands 20 px below the covered one's.
          const scale = (single[2] - 32) / single[2], cut = single[3] - Math.max(naturalHeight - 20, least), rise = Math.min(10, Math.max(0, single[1] - 32));
          for (const [actual, expected] of [[got.below, [single[0] + 16, single[1] - rise, single[2] - 32, single[3] * scale]],
            [got.above, [single[0], single[1] + 20 - rise, single[2], single[3] - cut]], [got.restored, single]]) {
            // Firefox quantizes transformed bounds to 1/60 px.
            for (let i = 0; i < 4; i++) expect(actual[i], `${engine} ${width} ${wrapped} body ${bodyHeight} ${frame} ${bodyStyle} dimension ${i}`).toBeCloseTo(expected[i], 1);
          }
          // A dialog gives up the room above its action, and its body takes what of that its sheet kept.
          for (const [i, higher, taller] of alone ? [[0, 0, 20 - cut], [1, 0, 0], [2, cut, 0]] : []) for (const j of [0, 1, 2, 3]) expect(got.layout.boxes[i][j], `${engine} ${width} ${wrapped} body ${bodyHeight} ${frame} ${bodyStyle} layout ${i} ${j}`).toBeCloseTo(alone.boxes[i][j] - (j === 1 ? higher : 0) + (j === 3 ? taller : 0), 1);
          expect(got.layout?.reachable, `${engine} ${width} ${wrapped} body ${bodyHeight} ${frame} ${bodyStyle}`).toBe(alone?.reachable);
          expect(got.scale).toBeCloseTo(scale, 5);
          expect(got.dim).toEqual(['1', '0.6', '0']);
          // The step moves no origin: a transform of the author's would turn about the sheet's centre.
          for (const i of [0, 1]) expect(got.origin[i], `${engine} ${width} ${frame} origin ${i}`).toBeCloseTo(got.size[i] / 2, 1);
          if (reducedMotion !== 'reduce') for (let i = 0; i < 4; i++) expect(got.leaving[i]).toBeCloseTo(got.above[i], 1);
          expect(parseFloat(got.radius)).toBeCloseTo(await page.evaluate(() => 32 * (CSS.supports('corner-shape', 'superellipse(2)') ? 1.52 : 1)), 2);
        }
      }
      await page.close();
    }
  } finally { await browser.close(); }
}, 90000);

for (const engine of ['chromium', 'firefox', 'webkit'])
for (const reducedMotion of ['no-preference', 'reduce'])
for (const presentation of ['', 'fullscreen', 'modal']) test(`${engine}: sheet stacking follows the presenting sheet (${presentation || 'pushed'}, ${reducedMotion})`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await stackPage(browser, { viewport: { width: 1024, height: 1000 }, reducedMotion });
    for (const width of [1024, 400]) {
      await page.setViewportSize({ width, height: 1000 });
      for (const wrapped of [false, true]) {
        const got = await page.evaluate(async ({ wrapped, presentation }) => {
          const nav = document.getElementById('nav'), first = document.getElementById('first');
          const add = (key, presentation) => {
            const route = first.cloneNode(true); route.id = `route-${key}`; route.setAttribute('navigationKey', key);
            if (presentation) route.setAttribute('navigationPresentation', presentation);
            else route.removeAttribute('navigationPresentation');
            if (presentation === 'fullscreen') route.style.backgroundColor = 'black';
            let parent = nav;
            if (wrapped) { parent = document.createElement('div'); parent.setAttribute('data-exact-list-item', ''); nav.append(parent); }
            parent.append(route); return route;
          };
          const finish = () => { for (const a of document.getAnimations()) a.finish(); };
          finish();
          const presenter = add('3', presentation); select('3'); finish();
          const above = add('4', 'modal'); select('4');
          await new Promise(requestAnimationFrame);
          const moving = presenter.getAnimations().map(a => a.transitionProperty).sort();
          finish();
          const rect = above.getBoundingClientRect(), under = presenter.getBoundingClientRect(), scale = getComputedStyle(presenter).scale;
          // Pushing inside the upper sheet keeps its shortened frame, and what the sheet covers as it stood.
          const pushed = add('5', ''); select('5');
          await new Promise(requestAnimationFrame); finish();
          const pushedRect = pushed.getBoundingClientRect(), still = presenter.getBoundingClientRect();
          const out = { top: rect.y, height: rect.height, under: [under.y, under.height], still: [still.y, still.height], moving, scale, pushed: [pushedRect.y, pushedRect.height] };
          for (const route of [pushed, above, presenter]) { const parent = route.parentElement; route.remove(); if (wrapped) parent.remove(); }
          select('2'); finish(); return out;
        }, { wrapped, presentation });
        const stacked = presentation !== 'fullscreen';
        // Wide, the step is a scale and a translation; narrow, a scale about the sheet's top.
        expect(got.moving).toEqual(stacked && reducedMotion !== 'reduce' ? width === 1024 ? ['scale', 'translate'] : ['scale'] : []);
        const [top, height] = width === 1024 ? stacked ? [150, 700] : [140, 720] : stacked ? [22, 978] : [12, 988];
        expect([got.top, got.height, got.pushed]).toEqual([top, height, [top, height]]);
        if (stacked) expect(parseFloat(got.scale)).toBeCloseTo(width === 1024 ? 528 / 560 : .93, 5);
        else expect(got.scale).toBe('none');
        // A wide sheet over a sheet keeps its shorter frame when a third covers it, and rises to where a covered one stands.
        const covered = width === 1024 ? [130, (presentation === 'modal' ? 700 : 720) * 528 / 560] : [12, 988 * .93];
        if (stacked) for (const i of [0, 1]) expect(got.under[i], `${engine} ${width} ${presentation} covered ${i}`).toBeCloseTo(covered[i], 1);
        expect(got.still, `${engine} ${width} ${presentation} under a push`).toEqual(got.under);
      }
    }
    await page.close();
  } finally { await browser.close(); }
}, 90000);

// UIKit steps a form sheet back only under a sheet of its own width and height, each as it is alone, and one 0.4 pt
// taller is another size. Under a sheet of another size the covered one keeps its frame under the whole dim, which
// the page takes a second time, and the one over it stands as it would alone, a card 20 px shorter. The sizes are
// compared at each projection, and when a sheet is resized with none, as an image loading in it resizes it.
for (const engine of ['chromium', 'firefox', 'webkit'])
for (const [below, above] of [[508, 308], [308, 508], [308, 309], [308, 308.4]]) test(`${engine}: a ${below} px sheet keeps its frame under a ${above} px sheet`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await stackPage(browser, { viewport: { width: 1024, height: 1000 } });
    // Asked for less motion, once: the second dim comes with no fade.
    for (const [wrapped, reducedMotion] of [[false, 'no-preference'], [true, 'no-preference'], ...(below === 508 ? [[false, 'reduce']] : [])]) {
      await page.emulateMedia({ reducedMotion });
      const got = await page.evaluate(async ({ below, above, wrapped }) => {
        const nav = document.getElementById('nav'), first = document.getElementById('first');
        // A dialog that tall alone, which over a sheet stands shorter by twice the stack's offset.
        const dialog = (route, height) => {
          route.innerHTML = '<header><h2>Dialog</h2><button>Save</button></header><div style="height:200px"></div>';
          const alone = route.getBoundingClientRect().height + 2 * parseFloat(getComputedStyle(route).getPropertyValue('--exact-sheet-stack-offset'));
          route.lastElementChild.style.height = `${200 + height - alone}px`;
        };
        const item = wrapped ? document.createElement('div') : null;
        if (item) { item.setAttribute('data-exact-list-item', ''); nav.append(item); item.append(first); }
        dialog(first, below); select('2'); settle();
        const over = first.cloneNode(false); over.id = 'over'; over.setAttribute('navigationKey', '3');
        const overItem = item?.cloneNode(false);
        nav.append(overItem ?? over); overItem?.append(over);
        const look = () => { settle(); return { below: box(first), above: box(over), scale: getComputedStyle(first).scale, shade: getComputedStyle(first, '::after').opacity, again: getComputedStyle(nav, '::after').opacity === '1' }; };
        const projected = async () => { select('3'); await new Promise(requestAnimationFrame); return look(); };
        // With no projection: what stands once a frame has laid the new size out.
        const resized = async () => { await new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done))); return look(); };
        const out = { alone: box(first) };
        dialog(over, above); select('3'); await new Promise(requestAnimationFrame);
        // What the page's second dim comes in by.
        out.dim = document.getAnimations().filter(a => a.effect.target === nav).map(a => `${a.effect.pseudoElement} ${a.transitionProperty}`);
        out.unequal = look();
        dialog(over, below); out.equal = await projected();
        dialog(over, above); out.grown = await resized();
        dialog(over, below); out.shrunk = await resized();
        over.innerHTML = '<header><h2>Card</h2></header>'; out.card = await projected();
        over.remove(); overItem?.remove();
        if (item) { nav.append(first); item.remove(); }
        select('2'); settle();
        return out;
      }, { below, above, wrapped });
      const near = (actual, expected, what) => { for (const i of [0, 1, 2, 3]) expect(actual[i], `${engine} ${wrapped} ${what} ${i}`).toBeCloseTo(expected[i], 1); };
      const y = (1000 - below) / 2;
      near(got.alone, [232, y, 560, below], 'alone');
      for (const [name, unequal] of [['projected', got.unequal], ['resized', got.grown]]) {
        near(unequal.below, got.alone, `${name} under another size`); near(unequal.above, [232, (1000 - above) / 2 + 10, 560, above - 20], `${name} another size`);
        expect([name, unequal.scale, unequal.shade, unequal.again]).toEqual([name, 'none', '1', true]);
      }
      for (const [name, equal] of [['projected', got.equal], ['resized', got.shrunk]]) {
        near(equal.below, [248, y - 10, 528, below * 528 / 560], `${name} under its own size`); near(equal.above, [232, y + 10, 560, below - 20], `${name} its own size`);
        expect([name, parseFloat(equal.scale).toFixed(4), equal.shade, equal.again]).toEqual([name, (528 / 560).toFixed(4), '0.6', false]);
      }
      expect(got.dim).toEqual(reducedMotion === 'reduce' ? [] : ['::after opacity']);
      near(got.card.below, got.alone, 'under a card'); near(got.card.above, [232, 150, 560, 700], 'a card');
      expect([got.card.scale, got.card.shade, got.card.again]).toEqual(['none', '1', true]);
    }
    await page.close();
  } finally { await browser.close(); }
}, 90000);

// The step's geometry is the host's own and lasts as long as its transform does. A host writes a route's inline style
// whole, so the geometry is kept out of it: a return holds its origin though the route's style is written as the
// upper sheet closes. Nothing of the step stays once the sheet is back. A sheet with a transform of its author's is
// not stepped, so theirs keeps its origin, however the target wrote it (inline, as the wasm target does, or as a
// class over the look's rules, as the JS target writes a static style) and whether it was there before the sheet
// was covered or came while it was stepped, which gives the step up.
const transforms = ['scale:0.9', 'rotate:2deg', 'transform:rotate(2deg)', 'translate:0 12px', 'transform-origin:0 0'];
for (const engine of ['chromium', 'firefox', 'webkit']) test(`${engine}: a step back leaves nothing on the sheet and moves no authored transform`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await stackPage(browser, { viewport: { width: 1024, height: 1000 } });
    for (const [authored, by, late] of [['', 'inline', false], ...transforms.flatMap(t => [[t, 'inline', false], [t, 'class', false], [t, 'inline', true], [t, 'class', true]])]) {
      const got = await page.evaluate(async ({ authored, by, late }) => {
        const nav = document.getElementById('nav'), first = document.getElementById('first');
        const rules = document.getElementById('authored') ?? document.head.appendChild(Object.assign(document.createElement('style'), { id: 'authored' }));
        rules.textContent = by === 'class' ? `#exact-root#exact-root#exact-root .authored{${authored}}` : '';
        // The sheet's style as its target writes it, with the author's transform or without.
        const write = (on, extra = '') => { first.classList.toggle('authored', on && by === 'class'); first.style.cssText = `position:absolute;inset:0;${on && by === 'inline' ? `${authored};` : ''}${extra}`; };
        const frames = () => new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done)));
        const state = () => ({ box: box(first), origin: getComputedStyle(first).transformOrigin, inline: first.getAttribute('style'),
          marks: first.getAttributeNames().filter(name => /^data-exact-sheet-(step|geometry)$/.test(name)) });
        write(true); select('2'); settle();
        const out = { alone: state() };
        write(!late); select('2'); settle();
        const over = first.cloneNode(true); over.id = 'over'; over.className = ''; over.setAttribute('navigationKey', '3'); over.style.cssText = 'position:absolute;inset:0;';
        nav.append(over); select('3');
        await frames(); settle();
        out.stepped = state().marks.includes('data-exact-sheet-step');
        // The author's transform comes in an update of the covered sheet.
        if (late) { write(true); select('3'); await frames(); settle(); }
        out.covered = state();
        // The upper sheet closes in the update that writes the lower one's style again.
        closeSheet(over); write(true, 'color:red;'); select('2');
        await new Promise(requestAnimationFrame);
        for (const a of first.getAnimations()) { a.pause(); a.currentTime = 0; }
        out.returning = state();
        settle(); await new Promise(requestAnimationFrame);
        out.back = state();
        over.remove(); write(false); select('2'); settle();
        out.rules = document.adoptedStyleSheets.reduce((count, sheet) => count + sheet.cssRules.length, 0);
        return out;
      }, { authored, by, late });
      const what = `${engine} ${authored} ${by}${late ? ' late' : ''}`;
      const near = (actual, expected, when) => { for (const i of [0, 1, 2, 3]) expect(actual[i], `${what} ${when} ${i}`).toBeCloseTo(expected[i], 1); };
      // Alone, an authored scale turns about the sheet's centre.
      if (!authored || authored === 'scale:0.9') near(got.alone.box, authored ? [260, 176, 504, 648] : [232, 140, 560, 720], 'alone');
      const covered = authored ? got.alone.box : [248, 130, 528, 720 * 528 / 560];
      near(got.covered.box, covered, 'covered'); near(got.returning.box, covered, 'as the return starts'); near(got.back.box, got.alone.box, 'back');
      expect([what, got.stepped, got.covered.marks.includes('data-exact-sheet-step'), got.back.marks, got.back.origin]).toEqual([what, !authored || late, !authored, [], got.alone.origin]);
      for (const state of [got.alone, got.covered, got.returning, got.back]) expect(state.inline).not.toContain('--exact');
      expect(got.rules).toBe(0);
    }
    // Two sheets opened at once: the lower is stepped while it slides in, its translation the look's own.
    const sliding = await page.evaluate(async () => {
      const nav = document.getElementById('nav'), first = document.getElementById('first');
      const add = key => { const route = first.cloneNode(true); route.id = `route-${key}`; route.setAttribute('navigationKey', key); nav.append(route); return route; };
      const lower = add('3'); add('4'); select('4');
      await new Promise(requestAnimationFrame);
      const out = [getComputedStyle(lower).translate !== 'none', lower.hasAttribute('data-exact-sheet-step')];
      for (const key of ['3', '4']) document.getElementById(`route-${key}`).remove();
      select('2'); settle();
      return out;
    });
    expect(sliding).toEqual([true, true]);
    await page.close();
  } finally { await browser.close(); }
}, 90000);

// An animation or a transition of the author's on a transform is the author's for as long as it is on the sheet,
// whatever its first frame is: the step is given up when it starts, and the sheet ends where it would alone. Each
// starts after the sheet is covered, as a class over the look's rules, and its time is set, not waited for. As the
// step is given up the sheet's centre stays where it was: the author's transform turns about it, and what is left of
// the step leaves by the look's own transitions. An author's `transition` names every transition the sheet has, so
// under one that names only the scale the step's translation leaves at once.
const animations = {
  'keyframes on scale': '@keyframes own{from{scale:1}to{scale:0.9}} #exact-root#exact-root#exact-root .own{animation:own 1s linear forwards}',
  'a transition on scale': '#exact-root#exact-root#exact-root .own{transition:scale 1s linear;scale:0.9}',
  'keyframes on transform': '@keyframes own{from{transform:none}to{transform:scale(0.9)}} #exact-root#exact-root#exact-root .own{animation:own 1s linear forwards}',
};
for (const engine of ['chromium', 'firefox', 'webkit']) test(`${engine}: a covered sheet's own animation takes its transform from the step`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await stackPage(browser, { viewport: { width: 1024, height: 1000 } });
    // An update that starts one projects, as a host's does. Keyframes started with none re-read the stack by their own
    // start; a transition's events do not, the step's own being transitions too.
    for (const [name, rules] of Object.entries(animations)) for (const projected of name === 'a transition on scale' ? [true] : [true, false]) {
      const got = await page.evaluate(async ({ rules, projected }) => {
        const nav = document.getElementById('nav'), first = document.getElementById('first');
        const sheet = document.getElementById('authored') ?? document.head.appendChild(Object.assign(document.createElement('style'), { id: 'authored' }));
        sheet.textContent = rules;
        const frames = () => new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done)));
        // Every animation on the sheet at a share of its own length.
        const at = share => { for (const a of first.getAnimations()) { a.pause(); a.currentTime = share * a.effect.getComputedTiming().endTime; } };
        const centre = () => { const [, y, , height] = box(first); return y + height / 2; };
        first.className = ''; first.style.cssText = 'position:absolute;inset:0;'; select('2'); settle();
        const over = first.cloneNode(true); over.id = 'over'; over.setAttribute('navigationKey', '3'); nav.append(over); select('3');
        await frames(); settle();
        const out = { stepped: first.hasAttribute('data-exact-sheet-step'), before: centre() };
        first.className = 'own';
        if (projected) select('3');
        await frames();
        at(0); out.start = centre(); out.released = !first.hasAttribute('data-exact-sheet-step');
        at(1); out.end = box(first);
        first.className = ''; over.remove(); select('2'); settle();
        return out;
      }, { rules, projected });
      const what = `${engine} ${name}${projected ? '' : ' with no projection'}`;
      expect([what, got.stepped, got.released]).toEqual([what, true, true]);
      expect(got.start, `${what} centre as it starts`).toBeCloseTo(name === 'a transition on scale' ? 500 : got.before, 1);
      for (const i of [0, 1, 2, 3]) expect(got.end[i], `${what} end ${i}`).toBeCloseTo([260, 176, 504, 648][i], 1);
    }
    await page.close();
  } finally { await browser.close(); }
}, 90000);

// A stack follows its window across the look's breakpoint with no projection. A plan with sheets and no tablist or
// split has no listener for the window's width (`TABS`, `SPLITS` off), so the sheets of a stack are watched
// themselves, in the narrow form as in the wide one.
const sheetsOnly = await bundleHostModule(resolve(import.meta.dir, '../../..'), 'navigation.js', { unused: ['TABS', 'SPLITS'] });
for (const engine of ['chromium', 'firefox', 'webkit']) test(`${engine}: a stack of sheets follows its window across the wide breakpoint`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    for (const widths of [[400, 1024], [1024, 400, 1024]]) {
      const page = await stackPage(browser, { viewport: { width: widths[0], height: 1000 } }, sheetsOnly);
      await page.evaluate(async () => {
        const nav = document.getElementById('nav'), first = document.getElementById('first');
        settle();
        const over = first.cloneNode(true); over.id = 'over'; over.setAttribute('navigationKey', '3'); nav.append(over); select('3');
        await new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done))); settle();
        unprojected();
      });
      for (const width of widths.slice(1)) {
        await page.setViewportSize({ width, height: 1000 });
        const got = await page.evaluate(async () => {
          await new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done))); settle();
          const first = document.getElementById('first');
          return { box: box(first), stepped: first.hasAttribute('data-exact-sheet-step') };
        });
        expect([`${engine} ${widths} at ${width}`, got.stepped]).toEqual([`${engine} ${widths} at ${width}`, width === 1024]);
        if (width === 1024) for (const i of [0, 1, 2, 3]) expect(got.box[i], `${engine} ${widths} ${i}`).toBeCloseTo([248, 130, 528, 720 * 528 / 560][i], 1);
      }
      await page.close();
    }
  } finally { await browser.close(); }
}, 90000);

// A scale or a translation the author wrote is the author's where it is an identity too: the step's own cannot apply
// over it, so the sheet is not stepped, and stays where it is from one projection to the next. An identity rotation
// reads as a rotation and is the author's as well. `transform: none` is what a sheet has unwritten, and leaves the
// step whole.
const identities = { 'scale:1': 140, 'translate:0px': 140, 'rotate:0deg': 140, 'transform:none': 130 };
for (const engine of ['chromium', 'firefox', 'webkit']) test(`${engine}: an authored identity transform keeps the covered sheet in one place`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    for (const reducedMotion of ['reduce', 'no-preference']) {
      const page = await stackPage(browser, { viewport: { width: 1024, height: 1000 }, reducedMotion });
      for (const [identity, top] of Object.entries(identities)) for (const by of ['inline', 'class']) {
        const tops = await page.evaluate(async ({ identity, by }) => {
          const nav = document.getElementById('nav'), first = document.getElementById('first');
          const rules = document.getElementById('authored') ?? document.head.appendChild(Object.assign(document.createElement('style'), { id: 'authored' }));
          rules.textContent = by === 'class' ? `#exact-root#exact-root#exact-root .authored{${identity}}` : '';
          first.classList.toggle('authored', by === 'class');
          first.style.cssText = `position:absolute;inset:0;${by === 'inline' ? identity : ''}`; select('2'); settle();
          const over = first.cloneNode(true); over.id = 'over'; over.className = ''; over.setAttribute('navigationKey', '3'); over.style.cssText = 'position:absolute;inset:0;';
          nav.append(over);
          const tops = [];
          // The projection that covers it, and three more with nothing changed.
          for (let i = 0; i < 4; i++) {
            select('3');
            await new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done))); settle();
            tops.push(box(first)[1]);
          }
          over.remove(); first.classList.remove('authored'); first.style.cssText = 'position:absolute;inset:0;'; select('2'); settle();
          return tops;
        }, { identity, by });
        for (const [i, got] of tops.entries()) expect(got, `${engine} ${reducedMotion} ${identity} ${by} projection ${i}`).toBeCloseTo(top, 1);
      }
      await page.close();
    }
  } finally { await browser.close(); }
}, 90000);

// Whose the scale is follows the rules that apply, not what was found before: a `scale: 1` of the author's that a
// media query or an ancestor's class takes away leaves the sheet to the step, and one that comes back takes the step
// off, at the stack's next read (a resize's own, or a projection's).
for (const engine of ['chromium', 'firefox', 'webkit']) test(`${engine}: a covered sheet steps once its author's identity scale no longer applies`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    for (const reducedMotion of ['reduce', 'no-preference']) {
      const page = await stackPage(browser, { viewport: { width: 1024, height: 1000 }, reducedMotion });
      const look = (of = page) => of.evaluate(async () => {
        await new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done))); settle();
        const first = document.getElementById('first');
        return [...box(first).slice(0, 3), first.hasAttribute('data-exact-sheet-step')];
      });
      const cover = (rules, of = page) => of.evaluate(rules => {
        const nav = document.getElementById('nav'), first = document.getElementById('first');
        (document.getElementById('authored') ?? document.head.appendChild(Object.assign(document.createElement('style'), { id: 'authored' }))).textContent = rules;
        first.className = 'authored'; select('2'); settle();
        document.getElementById('over')?.remove();
        const over = first.cloneNode(true); over.id = 'over'; over.className = ''; over.setAttribute('navigationKey', '3'); nav.append(over); select('3');
      }, rules);
      const near = (got, expected, what) => { for (const i of [0, 1, 2]) expect(got[i], `${engine} ${reducedMotion} ${what} ${i}`).toBeCloseTo(expected[i], 1); expect([what, got[3]]).toEqual([what, expected[3]]); };
      // Under a media query: the window is resized past it with no projection.
      await cover('@media (min-width: 1000px) { #exact-root#exact-root#exact-root .authored{scale:1} }');
      near(await look(), [232, 140, 560, false], 'under the query');
      await page.setViewportSize({ width: 950, height: 600 });
      near(await look(), [211, 32, 528, true], 'past the query');
      await page.setViewportSize({ width: 1024, height: 1000 });
      near(await look(), [232, 140, 560, false], 'under the query again');
      // A width past the query that resizes no card, in a plan with no tablist or split and with no projection.
      const sheets = await stackPage(browser, { viewport: { width: 950, height: 1000 }, reducedMotion }, sheetsOnly);
      await cover('@media (min-width: 1000px) { #exact-root#exact-root#exact-root .authored{scale:1} }', sheets);
      near(await look(sheets), [211, 130, 528, true], 'short of the query');
      await sheets.evaluate(() => unprojected());
      await sheets.setViewportSize({ width: 1024, height: 1000 });
      near(await look(sheets), [232, 140, 560, false], 'under the query, no card resized');
      await sheets.setViewportSize({ width: 950, height: 1000 });
      near(await look(sheets), [211, 130, 528, true], 'short of the query again');
      await sheets.close();
      // Under an ancestor's class: the class goes and comes in an update, which projects.
      await cover('#exact-root#exact-root#exact-root.plain .authored{scale:1}');
      await page.evaluate(() => { document.getElementById('exact-root').className = 'plain'; select('3'); });
      near(await look(), [232, 140, 560, false], 'under the class');
      await page.evaluate(() => { document.getElementById('exact-root').className = ''; select('3'); });
      near(await look(), [248, 130, 528, true], 'without the class');
      await page.evaluate(() => { document.getElementById('exact-root').className = 'plain'; select('3'); });
      near(await look(), [232, 140, 560, false], 'under the class again');
      await page.close();
    }
  } finally { await browser.close(); }
}, 90000);

// A stack that ends leaves nothing: the window is heard only while a sheet is over a sheet, and the upper sheet's
// lengths go when its slide takes it out of the document, which no projection follows.
for (const engine of ['chromium', 'firefox', 'webkit']) test(`${engine}: a stack that ends leaves no rule and hears no resize`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await stackPage(browser, { viewport: { width: 1024, height: 1000 } });
    const got = await page.evaluate(async () => {
      let hears = 0;
      for (const [name, by] of [['addEventListener', 1], ['removeEventListener', -1]]) {
        const was = window[name];
        window[name] = (type, ...rest) => { if (type === 'resize') hears += by; return was.call(window, type, ...rest); };
      }
      const nav = document.getElementById('nav'), first = document.getElementById('first');
      const frames = () => new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done)));
      const over = first.cloneNode(true); over.id = 'over'; over.setAttribute('navigationKey', '3'); nav.append(over); select('3');
      await frames(); settle();
      const id = over.getAttribute('data-exact-sheet-geometry');
      const rules = () => document.adoptedStyleSheets.flatMap(sheet => [...sheet.cssRules]).filter(rule => rule.selectorText.includes(`"${id}"`)).length;
      const stacked = { rules: rules(), hears };
      // The update that closes the sheet projects; its slide ends after.
      closeSheet(over); select('2'); unprojected();
      const sliding = over.isConnected;
      settle(); await frames();
      return { stacked, sliding, gone: !over.isConnected, rules: rules(), hears };
    });
    expect(got).toEqual({ stacked: { rules: 1, hears: 1 }, sliding: true, gone: true, rules: 0, hears: 0 });
    await page.close();
  } finally { await browser.close(); }
}, 90000);

// What left the document takes what the stack held for it. A root replaced with its two sheets open, three times,
// each new one projected; then the last removed with no projection after.
for (const engine of ['chromium', 'firefox', 'webkit']) test(`${engine}: a root removed with its sheets open leaves nothing watched`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await stackPage(browser, { viewport: { width: 1024, height: 1000 } });
    const got = await page.evaluate(async () => {
      let hears = 0;
      for (const [name, by] of [['addEventListener', 1], ['removeEventListener', -1]]) {
        const was = window[name];
        window[name] = (type, ...rest) => { if (type === 'resize') hears += by; return was.call(window, type, ...rest); };
      }
      const observed = new Set();
      for (const name of ['observe', 'unobserve']) {
        const was = ResizeObserver.prototype[name];
        ResizeObserver.prototype[name] = function (target, ...rest) { name === 'observe' ? observed.add(target) : observed.delete(target); return was.call(this, target, ...rest); };
      }
      const frames = () => new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done)));
      const sheets = '<div navigationKey="1"><header><h1>Inbox</h1></header></div>' + ['2', '3'].map(key => `<div navigationKey="${key}" navigationPresentation="modal" style="position:absolute;inset:0"><header><h2>New</h2></header></div>`).join('');
      // The routes the stack still watches or holds a rule for, though they have left the document.
      const left = () => ({ watched: [...observed].filter(el => el.hasAttribute('navigationKey') && !el.isConnected).length,
        rules: document.adoptedStyleSheets.flatMap(sheet => [...sheet.cssRules]).filter(rule => !document.querySelector(rule.selectorText)).length, hears });
      const out = [];
      for (let round = 0; round < 3; round++) {
        const next = document.createElement('main');
        next.id = 'nav'; next.setAttribute('navigationBack', 'back'); next.style.cssText = 'position:relative;height:600px'; next.innerHTML = sheets;
        document.getElementById('nav').replaceWith(next); select('3');
        await frames(); settle();
        out.push({ ...left(), stepped: next.children[1].hasAttribute('data-exact-sheet-step') });
      }
      document.getElementById('nav').remove();
      await frames();
      return [...out, left()];
    });
    expect(got).toEqual([...[0, 1, 2].map(() => ({ watched: 0, rules: 0, hears: 1, stepped: true })), { watched: 0, rules: 0, hears: 0 }]);
    await page.close();
  } finally { await browser.close(); }
}, 90000);

// UIKit lays a form sheet's dim over all beneath it, and one of another size than the sheet it covers has the page's
// (iPadOS 27): so each such sheet dims again what is beneath the sheet it covers, a full-screen route it was presented
// over as the page. Here a page, a white full-screen route, and sheets 508, 308 and 408 px tall: how dark each stands,
// from the boxes over it.
for (const engine of ['chromium', 'firefox', 'webkit']) test(`${engine}: a sheet of another size dims again all beneath the sheet it covers`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await stackPage(browser, { viewport: { width: 1024, height: 1000 } });
    const got = await page.evaluate(async () => {
      const nav = document.getElementById('nav'), first = document.getElementById('first');
      const frames = () => new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done)));
      const full = document.createElement('div');
      full.setAttribute('navigationKey', 'full'); full.setAttribute('navigationPresentation', 'fullscreen'); full.style.cssText = 'position:absolute;inset:0;background:white';
      first.before(full);
      const sheet = (key, height) => {
        const route = key === '2' ? first : nav.appendChild(first.cloneNode(false));
        route.id = `sheet${key}`; route.setAttribute('navigationKey', key);
        route.innerHTML = `<header><h2>Dialog</h2><button>Save</button></header><div style="height:${height}px"></div>`;
        return route;
      };
      // How much of what is under a box it hides: its colour's alpha, where the box is drawn, by its opacity.
      const box = (el, pseudo) => { const css = getComputedStyle(el, pseudo); return ['none', 'normal'].includes(css.content) ? 0 : parseFloat(css.opacity) * parseFloat(css.backgroundColor.match(/[\d.]+(?=\)$)/)[0]); };
      const dark = (...boxes) => 1 - boxes.reduce((light, [el, pseudo]) => light * (1 - box(el, pseudo)), 1);
      const scrim = () => box(nav, '::before');
      const out = { scrim: 0, stacks: [] };
      const routes = [sheet('2', 300)];
      for (const [key, height] of [['3', 100], ['4', 200]]) {
        routes.push(sheet(key, height)); select(key);
        await frames(); settle();
        out.scrim = scrim();
        out.stacks.push({ page: dark([nav, '::before'], [nav, '::after']), full: dark([full, '::before'], [full, '::after']), sheets: routes.slice(0, -1).map(route => dark([route, '::before'], [route, '::after'])), top: dark([routes.at(-1), '::before'], [routes.at(-1), '::after']) });
      }
      return out;
    });
    // One dim, two and three of them, laid one over another.
    const dims = n => 1 - (1 - got.scrim) ** n;
    expect(got.scrim).toBeGreaterThan(0);
    for (const [at, expected] of [[0, { page: dims(2), full: dims(2), sheets: [dims(1)], top: 0 }], [1, { page: dims(3), full: dims(3), sheets: [dims(2), dims(1)], top: 0 }]]) {
      const stack = got.stacks[at];
      for (const [name, actual, darkness] of [['page', stack.page, expected.page], ['full', stack.full, expected.full], ['top', stack.top, expected.top], ...expected.sheets.map((darkness, i) => [`sheet ${i}`, stack.sheets[i], darkness])]) {
        expect(actual, `${engine} ${at + 2} sheets, ${name}`).toBeCloseTo(darkness, 3);
      }
    }
    await page.close();
  } finally { await browser.close(); }
}, 90000);

// A sheet over a sheet of its own size, each as it is alone, however its author holds its height: the covered one
// steps back, and the upper one gives up what it can of the room and stands 10 px lower. Each case writes one style on
// a dialog's route, on the box a component wraps a dialog's content in, or on a card, and says what the stack takes.
// Where the stack cannot tell a dialog's height alone it lays the dialog out with its room, and no transition of the
// author's runs for that; a `transition: all` takes the step's own properties with it where a browser transitions a
// registered property, and the sheet steps all the same.
const held = [
  ['route', '', 20], ['route', 'min-height:calc(ALONE - 8px)', 8], ['route', 'min-height:50%', 0], ['route', 'min-height:calc(ALONE - 8px);transition:all 1s', 8],
  ['route', 'grid-template-rows:auto minmax(0,1fr) auto', 0], ['box', '', 20], ['box', 'height:400px', 0], ['box', 'max-height:250px', 0], ['box', 'min-height:calc(ALONE - 8px);transition:all 1s', 8],
  ['card', 'transition:all 1s', 20],
];
for (const engine of ['chromium', 'firefox', 'webkit']) test(`${engine}: two sheets of one size step however their author holds their height`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await stackPage(browser, { viewport: { width: 1024, height: 1000 } });
    for (const [where, authored, took] of held) {
      const got = await page.evaluate(async ({ where, authored }) => {
        const nav = document.getElementById('nav'), first = document.getElementById('first');
        const frames = () => new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done)));
        const content = where === 'card' ? '<header><h2>Card</h2></header><p>Content</p>' : '<header><h2>Dialog</h2><button>Save</button></header><div style="height:200px"></div>';
        const write = (route, style) => {
          route.style.cssText = `position:absolute;inset:0;${where === 'box' ? '' : style}`;
          route.innerHTML = where === 'box' ? `<div style="${style}">${content}</div>` : content;
        };
        // The height the content gives the dialog alone, which a case's style may name.
        write(first, ''); select('2'); settle();
        const style = authored.replace('ALONE', `${first.getBoundingClientRect().height}px`);
        write(first, style); select('2'); settle();
        const alone = box(first), over = first.cloneNode(true);
        over.id = 'over'; over.setAttribute('navigationKey', '3'); nav.append(over); select('3');
        // An author's transition takes what the stack changes too, and the stack reads the sheet again as it ends.
        for (const _ of [0, 1]) { await frames(); settle(); }
        // A read with nothing changed: what runs on the upper sheet or its box after it.
        select('3'); await frames();
        const running = document.getAnimations().filter(a => a.effect.target === over || a.effect.target === over.firstElementChild).map(a => a.transitionProperty);
        settle();
        const out = { alone, above: box(over), stepped: first.hasAttribute('data-exact-sheet-step'), running, marks: [over, over.firstElementChild].map(el => el.getAttribute('data-exact-sheet-alone')) };
        over.remove(); select('2'); settle();
        return out;
      }, { where, authored });
      const name = `${engine} ${where} ${authored}`;
      expect([name, got.stepped, got.running, got.marks]).toEqual([name, true, [], [null, null]]);
      for (const [i, expected] of [got.alone[0], got.alone[1] + 10, got.alone[2], got.alone[3] - took].entries()) expect(got.above[i], `${name} dimension ${i}`).toBeCloseTo(expected, 1);
    }
    await page.close();
  } finally { await browser.close(); }
}, 90000);

// UIKit keeps a form sheet 32 pt from its window's top and bottom (iPadOS 27, a 1032×1376 window). A covered sheet
// rises 10 pt or as far as that margin lets it, and a sheet over a sheet, of its size or another, is 20 pt shorter
// about its centre with its top no higher than 52 pt: so two sheets that fill the window's height stand at 32 and at
// 52 on one foot, and a third raises the second from 52 to 32. Here cards in windows 600 px tall, where a card fills
// the height, and 796 px tall, where it stands 38 px from the top.
for (const engine of ['chromium', 'firefox', 'webkit']) test(`${engine}: sheets at the window's margin stand as UIKit's`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    for (const [height, alone] of [[600, [232, 32, 560, 536]], [796, [232, 38, 560, 720]]]) {
      const page = await stackPage(browser, { viewport: { width: 1024, height } });
      const got = await page.evaluate(async () => {
        const nav = document.getElementById('nav'), first = document.getElementById('first');
        const frames = () => new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done)));
        settle();
        const out = { alone: box(first) };
        const add = (key, style = '') => { const route = nav.appendChild(first.cloneNode(true)); route.id = `sheet${key}`; route.style.cssText += style; route.setAttribute('navigationKey', key); select(key); return route; };
        const second = add('3');
        await frames(); settle();
        out.two = [box(first), box(second)];
        const third = add('4');
        await frames(); settle();
        out.three = [box(first), box(second), box(third)];
        third.remove(); second.remove();
        // A sheet of another width keeps the covered one's frame.
        const narrow = add('3', 'max-width:500px');
        await frames(); settle();
        out.other = [box(first), box(narrow)];
        return out;
      });
      const near = (actual, expected, what) => { for (const i of [0, 1, 2, 3]) expect(actual[i], `${engine} ${height} ${what} ${i}`).toBeCloseTo(expected[i], 1); };
      const [x, y, w, h] = alone, top = Math.max(y - 10, 32), stepped = (tall) => [x + 16, top, w - 32, tall * 528 / 560];
      near(got.alone, alone, 'alone');
      near(got.two[0], stepped(h), 'covered'); near(got.two[1], [x, top + 20, w, h - 20], 'over it');
      near(got.three[0], stepped(h), 'under two'); near(got.three[1], stepped(h - 20), 'the second, covered'); near(got.three[2], [x, top + 20, w, h - 20], 'the third');
      near(got.other[0], alone, 'under another width'); near(got.other[1], [x + 30, Math.max(y + 10, 52), 500, h - 20], 'another width');
      await page.close();
    }
  } finally { await browser.close(); }
}, 90000);

// Reading how tall a dialog is alone leaves its slide alone. A dialog a component wraps, and one its `min-height`
// holds, each opened over a dialog of its size with its slide held at its start: after the stack is read the slide
// is still there, the sheet a window's height below its place.
for (const engine of ['chromium', 'firefox', 'webkit']) test(`${engine}: a dialog measured alone keeps its slide`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await stackPage(browser, { viewport: { width: 1024, height: 1000 } });
    for (const [where, authored] of [['box', ''], ['route', 'min-height:calc(ALONE - 8px)']]) {
      const got = await page.evaluate(async ({ where, authored }) => {
        const nav = document.getElementById('nav'), first = document.getElementById('first');
        const frames = () => new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done)));
        const content = '<header><h2>Dialog</h2><button>Save</button></header><div style="height:200px"></div>';
        const write = (route, style) => {
          route.style.cssText = `position:absolute;inset:0;${where === 'box' ? '' : style}`;
          route.innerHTML = where === 'box' ? `<div style="${style}">${content}</div>` : content;
        };
        write(first, ''); select('2'); settle();
        write(first, authored.replace('ALONE', `${first.getBoundingClientRect().height}px`)); select('2'); settle();
        const alone = box(first), over = first.cloneNode(true);
        over.id = 'over'; over.setAttribute('navigationKey', '3'); nav.append(over); select('3');
        // The slide starts with the sheet's first style: hold it there.
        void getComputedStyle(over).translate;
        const slides = () => over.getAnimations().filter(a => a.transitionProperty === 'translate');
        for (const slide of slides()) { slide.pause(); slide.currentTime = 0; }
        const started = slides().length;
        await frames();
        const out = { alone, started, kept: slides().length, y: box(over)[1], stepped: first.hasAttribute('data-exact-sheet-step') || first.getAnimations().length > 0 };
        settle(); over.remove(); select('2'); settle();
        return out;
      }, { where, authored });
      const name = `${engine} ${where} ${authored}`;
      expect([name, got.started, got.kept, got.stepped]).toEqual([name, 1, 1, true]);
      expect(got.y, `${name} top`).toBeCloseTo(got.alone[1] + 10 + 1000, 1);
    }
    await page.close();
  } finally { await browser.close(); }
}, 90000);

// A stack whose routes have all left goes, though the root they stood in stays: here two sheets of two sizes, so the
// page is dimmed again, and then every child of the root replaced by a placeholder. A projection finds no route to
// frame and leaves the root as it was; the stack's dim, its rules, its watch and its listener go all the same, at
// that projection, and with none at the window's next resize.
for (const engine of ['chromium', 'firefox', 'webkit'])
for (const then of ['a projection', 'a resize']) test(`${engine}: a stack whose routes are replaced leaves nothing on its root (${then})`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await stackPage(browser, { viewport: { width: 1024, height: 1000 } });
    const stacked = await page.evaluate(async () => {
      window.hears = 0;
      for (const [name, by] of [['addEventListener', 1], ['removeEventListener', -1]]) {
        const was = window[name];
        window[name] = (type, ...rest) => { if (type === 'resize') window.hears += by; return was.call(window, type, ...rest); };
      }
      window.observed = new Set();
      for (const name of ['observe', 'unobserve']) {
        const was = ResizeObserver.prototype[name];
        ResizeObserver.prototype[name] = function (target, ...rest) { name === 'observe' ? observed.add(target) : observed.delete(target); return was.call(this, target, ...rest); };
      }
      const nav = document.getElementById('nav'), first = document.getElementById('first');
      // What the stack holds on the root and for routes that have left it.
      window.held = () => ({ dims: getComputedStyle(nav).getPropertyValue('--exact-sheet-stack-page-dims').trim(), geometry: nav.hasAttribute('data-exact-sheet-geometry'),
        watched: [...observed].filter(el => el.hasAttribute('navigationKey') && !el.isConnected).length,
        rules: document.adoptedStyleSheets.flatMap(sheet => [...sheet.cssRules]).filter(rule => !document.querySelector(rule.selectorText)).length, hears });
      const over = first.cloneNode(true); over.id = 'over'; over.style.cssText += 'max-width:500px'; over.setAttribute('navigationKey', '3'); nav.append(over); select('3');
      await new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done))); settle();
      window.replaced = () => nav.replaceChildren(Object.assign(document.createElement('p'), { textContent: 'Loading' }));
      return held();
    });
    expect(stacked).toEqual({ dims: '1', geometry: true, watched: 0, rules: 0, hears: 1 });
    // The projection is looked at before a frame, so nothing an observer does is counted; the resize, a frame after.
    const left = then === 'a projection' ? await page.evaluate(() => { replaced(); select('3'); return held(); })
      : (await page.evaluate(() => replaced()), await page.setViewportSize({ width: 1000, height: 1000 }), await page.evaluate(async () => { await new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done))); return held(); }));
    expect(left).toEqual({ dims: '', geometry: false, watched: 0, rules: 0, hears: 0 });
    await page.close();
  } finally { await browser.close(); }
}, 90000);

