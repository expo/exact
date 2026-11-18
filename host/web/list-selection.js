// Logical text selection over a virtualized list's rows, mounted or not;
// navigation.js reports the list's geometry. Loaded after paint only for
// lists; ordinary selection stays native.
globalThis.exact.installListSelection = function ({ root, lists, index, text }) {
  let frame = null;
  function schedule() {
    if (frame === null && lists.size) frame = requestAnimationFrame(flush);
  }
  function flush() {
    frame = null;
    readSelection();
  }
  function sync() { schedule(); }
  let selected = null, lastList = null, applying = false, nativeStamp = [], selectionPending = false;
  const rowSelector = '[data-listitemkey]', paragraphSelector = '[data-exact-text]';
  const style = document.createElement('style');
  style.textContent = '::highlight(exact-list-selection){background:Highlight;color:HighlightText}';
  document.head.append(style);
  const element = node => node?.nodeType === Node.ELEMENT_NODE ? node : node?.parentElement;
  const editor = target => element(target)?.closest('input,textarea,select,[contenteditable]:not([contenteditable="false"])');
  function owner(node) {
    for (let el = element(node); el && el !== root; el = el.parentElement) if (lists.has(el)) return el;
    return null;
  }
  function paragraphs(row) {
    if (getComputedStyle(row).display === 'none') return [];
    if (row.matches(paragraphSelector)) return [row];
    return [...row.querySelectorAll(paragraphSelector)].filter(el => {
      for (let p = el; p && p !== row; p = p.parentElement) if (lists.has(p) || getComputedStyle(p).display === 'none') return false;
      return true;
    });
  }
  // A text the reader cannot select (`user-select: none` on it or above) is
  // no part of the selection: the runner leaves it out of the copy, and it
  // still counts toward a row's paragraph ordinals.
  function selectable(el) {
    for (let p = el; p && p !== root; p = p.parentElement) {
      const value = getComputedStyle(p).userSelect;
      if (value !== 'auto') return value !== 'none';
    }
    return true;
  }
  // Offsets count the paragraph's text nodes, which differ from a Markdown
  // or image paragraph's authored source; a copy that cuts the paragraph
  // clips this text instead, without the spans display hides.
  function rendered(p) {
    let text = '';
    const hidden = [], walk = document.createTreeWalker(p, NodeFilter.SHOW_TEXT);
    for (let n; (n = walk.nextNode());) {
      for (let el = n.parentElement; el && el !== p; el = el.parentElement) {
        if (getComputedStyle(el).display === 'none') { hidden.push([text.length, text.length + n.length]); break; }
      }
      text += n.data;
    }
    return { text, hidden };
  }
  // The list's children that hold its fixed lead or trail content (LLP 1070
  // §14), which collection-glue.js and the JS target's list.js mark with
  // their side.
  function fixedChildren(list, trail) {
    return [...list.children].filter(el => el.$collectionFixed === (trail ? 'trail' : 'lead'));
  }
  // An endpoint names a keyed row, or the fixed lead or trail as one region
  // whose paragraphs count across its children, as the runner's
  // `ListTextPosition` and the Mac's `Position` do. A row's key survives its
  // retirement; `row` orders endpoints and is refreshed from the key.
  function position(node, offset) {
    const p = element(node)?.closest(paragraphSelector), list = owner(p);
    if (!p || !list) return null;
    let at = null;
    for (let el = p; el !== list && !at; el = el.parentElement) {
      if (el.$collectionFixed) {
        const trail = el.$collectionFixed === 'trail';
        at = { fixed: trail, key: '', row: trail ? Infinity : -1, paragraph: fixedChildren(list, trail).flatMap(paragraphs).indexOf(p) };
      } else if (el.matches(rowSelector)) {
        at = { fixed: null, key: el.dataset.listitemkey, row: Number(el.getAttribute('aria-posinset')) - 1, paragraph: paragraphs(el).indexOf(p) };
      }
    }
    if (!at || at.paragraph < 0) return null;
    const range = document.createRange();
    range.selectNodeContents(p);
    try { range.setEnd(node, offset); } catch { return null; }
    return { list, ...at, offset: range.toString().length, rendered: rendered(p) };
  }
  // Selection.toString() asks for rendered text and forces layout even for a
  // collapsed selection. Endpoint identity/offsets detect native changes without
  // reading geometry, including distinct text nodes inside the same inline run.
  const stamp = s => s ? [s.anchorNode, s.anchorOffset, s.focusNode, s.focusOffset] : [];
  function capture() {
    const s = getSelection();
    if (!s || s.isCollapsed) return;
    const a = position(s.anchorNode, s.anchorOffset), b = position(s.focusNode, s.focusOffset);
    if (a && b && a.list === b.list) selected = { list: a.list, a, b, all: false };
  }
  // The target's `text` slices authored source between two endpoints, which
  // a paragraph the selection covers whole copies (Markdown, image labels).
  // A paragraph an endpoint cuts copies the rendered text the reader
  // selected, captured with the endpoint, without display-hidden spans.
  function copied(list, a, b) {
    const [lo, hi] = compare(a, b) <= 0 ? [a, b] : [b, a], start = { ...lo, offset: 0 }, end = { ...hi, offset: 0xffffffff };
    const loCut = lo.offset > 0, hiCut = hi.offset < hi.rendered.text.length;
    if (lo.row === hi.row && lo.paragraph === hi.paragraph) {
      const whole = text(list, start, end);
      return whole && (loCut || hiCut) ? clip(lo.rendered, lo.offset, hi.offset) : whole;
    }
    let out = text(list, start, end);
    if (loCut) out = splice(out, text(list, start, { ...lo, offset: 0xffffffff }), clip(lo.rendered, lo.offset, lo.rendered.text.length), true);
    if (hiCut) out = splice(out, text(list, { ...hi, offset: 0 }, end), clip(hi.rendered, 0, hi.offset), false);
    return out;
  }
  // `out` with its first (`atStart`) or last paragraph `from` replaced by `to`.
  function splice(out, from, to, atStart) {
    if (!from || !(atStart ? out.startsWith(from) : out.endsWith(from))) return out;
    const rest = atStart ? out.slice(from.length) : out.slice(0, out.length - from.length);
    if (to) return atStart ? to + rest : rest + to;
    return atStart ? rest.replace(/^\n\n/, '') : rest.replace(/\n\n$/, '');
  }
  function clip({ text, hidden }, start, end) {
    let out = '', at = start;
    for (const [a, b] of hidden) {
      if (b <= at || a >= end) continue;
      out += text.slice(at, Math.max(at, a)); at = Math.min(end, b);
    }
    return out + text.slice(at, end);
  }
  function clear() { selected = null; CSS.highlights?.delete('exact-list-selection'); }
  function compare(a, b) { return (a.row > b.row) - (a.row < b.row) || a.paragraph - b.paragraph || a.offset - b.offset; }
  function endpoint(p, offset) {
    const walk = document.createTreeWalker(p, NodeFilter.SHOW_TEXT);
    let n;
    while ((n = walk.nextNode())) { if (offset <= n.length) return [n, offset]; offset -= n.length; }
    return [p, p.childNodes.length];
  }
  function paint() {
    if (!selected) return;
    const { list, a, b, all } = selected;
    if (!list.isConnected || !lists.has(list)) { clear(); return; }
    if (!all) {
      for (const at of [a, b]) if (at.fixed == null && (at.row = index(list, at.key)) === 0xffffffff) { clear(); return; }
    }
    const [lo, hi] = all || compare(a, b) <= 0 ? [a, b] : [b, a], ranges = [];
    const regions = all ? [[0, paragraphs(list)]] : [[-1, fixedChildren(list, false).flatMap(paragraphs)],
      ...[...list.querySelectorAll(rowSelector)].filter(row => owner(row) === list)
        .map(row => [Number(row.getAttribute('aria-posinset')) - 1, paragraphs(row)]),
      [Infinity, fixedChildren(list, true).flatMap(paragraphs)]];
    for (const [ordinal, region] of regions) {
      for (const [i, p] of region.entries()) {
        const at = { row: ordinal, paragraph: i, offset: 0 };
        const end = { ...at, offset: p.textContent.length };
        if (!all && (compare(end, lo) < 0 || compare(at, hi) > 0) || !selectable(p)) continue;
        const startOffset = !all && ordinal === lo.row && i === lo.paragraph ? lo.offset : 0;
        const endOffset = !all && ordinal === hi.row && i === hi.paragraph ? hi.offset : end.offset;
        if (endOffset <= startOffset) continue;
        const r = document.createRange();
        r.setStart(...endpoint(p, startOffset)); r.setEnd(...endpoint(p, endOffset)); ranges.push(r);
      }
    }
    if (globalThis.Highlight && CSS.highlights) CSS.highlights.set('exact-list-selection', new Highlight(...ranges));
  }
  function readSelection() {
    if (!selectionPending) return;
    // Even reading Selection.anchorNode can flush browser layout. Reconcile
    // only after layout, never inside the batch's write phase.
    nativeStamp = stamp(getSelection()); applying = false;
    selectionPending = false; paint();
  }
  document.addEventListener('selectionchange', () => {
    if (applying || stamp(getSelection()).every((value, i) => value === nativeStamp[i])) return;
    capture(); paint();
  });
  root.addEventListener('pointerdown', event => {
    applying = false; selectionPending = false;
    lastList = owner(event.target);
    if (!event.shiftKey) clear();
  }, true);
  document.addEventListener('keydown', event => {
    applying = false; selectionPending = false;
    if (editor(event.target)) return;
    if (event.key === 'Escape') { clear(); return; }
    if (event.key.toLowerCase() !== 'a' || !(event.metaKey || event.ctrlKey) || event.altKey || event.defaultPrevented) return; // a key handler's preventDefault() keeps it, as on macOS and Linux
    const list = owner(getSelection()?.anchorNode) ?? (lastList?.isConnected ? lastList : null)
      ?? (lists.size === 1 ? lists.keys().next().value : null);
    if (!list) return;
    selected = { list, all: true };
    event.preventDefault(); getSelection().removeAllRanges(); nativeStamp = stamp(getSelection()); paint();
  });
  document.addEventListener('copy', event => {
    if (editor(event.target) || !selected || !event.clipboardData) return;
    paint();
    if (!selected) return;
    const value = selected.all ? text(selected.list, null, null) : copied(selected.list, selected.a, selected.b);
    event.clipboardData.setData('text/plain', value); event.preventDefault();
  });
  return {
    sync,
    forget(el) { lists.get(el)?.observer?.disconnect(); },
    before() { if (!applying) capture(); applying = true; },
    after() {
      if (!lists.size) { clear(); applying = false; selectionPending = false; nativeStamp = []; return; }
      // Read selection/visibility at observer delivery or the next frame's
      // read phase, before the next batch can dirty layout again.
      selectionPending = true; schedule();
    },
  };
};
