// The finding lines of a verify report: check's groups, each line's rule and place, and the lines by rule (LLP 1116.004
// D1.3), and its largest-text comparison. verify.mjs re-exports what its callers import.

/** check's text by scheme: each header's finding lines, how many its "… and N more" left out, and its advice
 * (LLP 1116.004 D1.3), which is never a finding. */
export function groups(text) {
  const out = [];
  for (const line of String(text ?? '').split('\n')) {
    if (/^check /.test(line)) { out.push({ head: line, lines: [], hidden: 0, advice: [] }); continue; }
    const g = out.at(-1), more = line.match(/^\s+… and (\d+) more/);
    if (!g || !line.trim()) continue;
    if (more) g.hidden += Number(more[1]);
    else if (/^\s+advice\s/.test(line)) g.advice.push(line);
    else g.lines.push(line);
  }
  return out;
}

const PLACES = 4; // where a rule's findings are, named after its first in full

/** A finding line's rule: a compiler warning's id, else its first word after any `(… only)` tag (`contrast`). */
export function ruleOf(line) {
  const t = line.trim(), id = /^\S+:\d+:\d+ warning \[([\w-]+)\]/.exec(t);
  return id ? id[1] : t.replace(/^\([^)]*\) /, '').split(/\s+/)[0];
}

/** Where a finding line points: a compiler warning's `file:line:col`, else what follows its rule, up to its first `: `. */
function placeOf(line) {
  const t = line.trim(), id = /^(\S+:\d+:\d+) warning \[/.exec(t);
  if (id) return id[1];
  const body = t.replace(/^\([^)]*\) /, '').replace(/^\S+\s+/, ''), at = body.indexOf(': ');
  return at < 0 ? body : body.slice(0, at);
}

/** `lines` (each indented two) by rule, the commonest first: a rule's one finding as it is; several as the rule and
 *  their count, the first in full, then where the next PLACES are and how many more. */
export function byRule(lines) {
  const rules = new Map();
  for (const l of lines) rules.set(ruleOf(l), [...(rules.get(ruleOf(l)) ?? []), l]);
  return [...rules].sort((a, b) => b[1].length - a[1].length).flatMap(([rule, all]) => {
    if (all.length === 1) return all;
    const rest = all.slice(1), more = rest.length - PLACES;
    return [`  ${rule} ×${all.length}`, `    ${all[0].trim()}`, `    also ${rest.slice(0, PLACES).map(placeOf).join('; ')}${more > 0 ? ` and ${more} more` : ''}`];
  });
}

/** Content whose web boxes do not represent iOS's navigation bar at the largest text size.
 * Read the header shape as kernel/src/bars.rs does (LLP 1115.001 D11), then honor the per-role
 * appearance opt-outs of HeaderShapeIOS.swift (LLP 1075.003 §9.10). Other header content stays judged. */
export async function iosNavigationContent(nodes, readStyle) {
  const children = n => (n?.children ?? []).map(id => nodes.get(id)).filter(Boolean);
  const parents = new Map([...nodes.values()].flatMap(n => children(n).map(c => [c.id, n])));
  // UIKit takes only the first document subview as its navigation root.
  const firstRoot = [...nodes.values()].find(n => !parents.has(n.id));
  const role = n => n.props?.accessibilityRole, tag = n => n.props?.semanticTag;
  const owner = n => {
    let at = parents.get(n.id);
    while (at && at.props?.navigationKey == null) at = parents.get(at.id);
    return at;
  };
  const layer = n => n.props?.popover != null || tag(n) === 'dialog';
  const heading = n => n.type === 'Text' && role(n) === 'heading';
  const holdsHeading = n => children(n).some(c => !layer(c) && (heading(c) || holdsHeading(c)));
  const authored = n => n.style?.appearance?.value === 'none' && ['authored', 'dynamic'].includes(n.style.appearance.source);
  const button = n => n.type === 'Pressable' || n.type === 'Control' && n.props?.type === 'button';
  const control = n => button(n) || ['View', 'NativeView'].includes(n.type)
    && ['button', 'link', 'checkbox', 'radio', 'switch'].includes(role(n)) || n.handlers?.includes('press');
  const byId = new Map([...nodes.values()].filter(n => n.props?.id != null).map(n => [n.props.id, n]));
  const asciiLower = s => String(s ?? '').replace(/[A-Z]/g, c => c.toLowerCase());
  const opensPopover = n => {
    if (!button(n)) return false;
    const p = n.props ?? {}, command = byId.get(p.commandfor);
    if (command) return command.props?.popover != null && /^(show|toggle)-popover$/.test(asciiLower(p.command));
    return byId.get(p.popovertarget)?.props?.popover != null && asciiLower(p.popovertargetaction) !== 'hide';
  };
  const projected = new Set(), headers = [];
  const include = (n, projects) => {
    if (layer(n) || !projects(n)) return;
    projected.add(n.id);
    children(n).forEach(c => include(c, projects));
  };
  for (const route of nodes.values()) {
    const p = route.props ?? {}, header = children(route)[0];
    if (p.navigationKey == null || p.navigationBack != null || p.navigationBar === 'none' || tag(header ?? {}) !== 'header' || authored(header)) continue;
    const parent = parents.get(route.id), panel = role(parent ?? {}) === 'tabpanel';
    const root = panel ? owner(parent) : parent;
    if (!root || root !== firstRoot || root.props?.navigationBack == null) continue;
    if (panel && ![...nodes.values()].some(n => role(n) === 'tablist' && owner(n) === root && children(n).some(tab =>
      role(tab) === 'tab' && tab.props?.accessibilityControls === parent.props?.id))) continue;
    const headings = [], items = [], searches = [], segments = [], readouts = [];
    let tap;
    const walk = n => {
      for (const c of children(n)) {
        if (layer(c)) continue;
        if (heading(c)) headings.push(c);
        else if (c.type === 'TextInput' && c.props?.type === 'search') searches.push(c);
        else if (role(c) === 'tablist') segments.push(c);
        else if (c.handlers?.includes('press') || opensPopover(c) || button(c) && role(c) === 'button' && c.props?.disabled && !holdsHeading(c)) {
          if (!tap && c.handlers?.includes('press') && holdsHeading(c)) { tap = c; walk(c); }
          else items.push(c);
        } else if (role(c) === 'status' && !holdsHeading(c)) readouts.push(c);
        else walk(c);
      }
    };
    walk(header);
    if (headings.length !== 1) continue;
    headers.push(header);
    // Only header contents need the styles used by HeaderTitle and a readout's face.
    const styled = new Map();
    const load = async n => {
      if (readStyle) styled.set(n.id, await readStyle(n.id));
      await Promise.all(children(n).filter(c => !layer(c)).map(load));
    };
    await load(header);
    const style = n => styled.get(n.id) ?? n.style ?? {};
    const shown = n => children(n).filter(c => style(c).display?.value !== 'none');
    const filled = n => style(n).background_color != null;
    const text = n => n.type === 'Text' ? n.props?.text ?? children(n).map(text).join('') : '';
    const paragraph = n => n.type === 'Text' && parents.get(n.id)?.type !== 'Text';
    const symbol = n => n.type === 'Image' && /^symbol:(?:sf\/)?.+/.test(n.props?.imageSource ?? '');
    const within = (n, ancestor) => {
      for (let at = n; at; at = parents.get(at.id)) if (at === ancestor) return true;
      return false;
    };
    const hidden = n => {
      for (let at = n; at && at !== header; at = parents.get(at.id)) if (style(at).display?.value === 'none') return true;
      return false;
    };
    const face = n => {
      if (paragraph(n)) return text(n) !== '';
      if (symbol(n)) return true;
      const kids = shown(n);
      if (!children(n).length && filled(n)) return parseFloat(style(n).width?.value) > 0 && parseFloat(style(n).height?.value) > 0;
      return kids.some(face);
    };
    const shownReadouts = readouts.filter(n => !hidden(n) && face(n));
    const roles = [...headings, ...items, ...shownReadouts, searches[0], segments[0], tap].filter(Boolean);
    const optedOut = new Set(roles.filter(authored).map(n => n.id));
    const projects = n => {
      for (let at = n; at && at !== header; at = parents.get(at.id)) if (optedOut.has(at.id)) return false;
      return true;
    };
    const authoredInside = n => roles.some(c => {
      if (!optedOut.has(c.id)) return false;
      for (let at = c; at && at !== header; at = parents.get(at.id)) if (at === n) return true;
      return false;
    });
    if (tap && projects(headings[0]) && !authoredInside(tap)) include(tap, projects);
    const tabs = segments[0];
    const segmented = tabs && children(tabs).some(n => button(n) && role(n) === 'tab' && n.handlers?.includes('press'));
    for (const n of [...headings, ...items, ...shownReadouts, searches[0], segmented ? tabs : null].filter(Boolean)) include(n, projects);
    if (!projects(headings[0])) continue;
    const apart = [...items, ...shownReadouts.filter(projects), searches[0], tabs].filter(Boolean);
    const apartInside = n => apart.some(c => within(c, n));
    const title = headings[0], titleTap = tap && projects(tap) ? tap : null;
    let wrapper = titleTap;
    if (!wrapper) for (let at = parents.get(title.id); at && at !== header; at = parents.get(at.id)) if (!apartInside(at)) wrapper = at;
    const passive = n => !control(n) && !filled(n);
    const glyphLine = n => {
      if (paragraph(n) || !passive(n)) return false;
      const kids = shown(n).filter(c => !apart.includes(c));
      return kids.every(c => passive(c) && (c.type === 'Image' && c.props?.imageSource?.startsWith('symbol:') || paragraph(c)))
        && kids.some(c => c.type === 'Image') && kids.some(c => paragraph(c) && text(c) !== '');
    };
    const placed = [], row = parents.get(title.id), rowKids = row ? shown(row).filter(c => !apart.includes(c)) : [];
    const headingImages = row && style(row).display?.value === 'flex' && (style(row).flex_direction?.value ?? 'row') === 'row'
      && rowKids.every(c => c === title || passive(c) && symbol(c)) ? rowKids.filter(c => c !== title) : [];
    let passed = false, subtitled = false, avatar = false, drawn = !!titleTap || headingImages.length > 0;
    const titleWalk = n => {
      for (const c of shown(n)) {
        if (layer(c)) continue;
        if (c === title) { passed = true; continue; }
        if (headingImages.includes(c)) { placed.push(c); continue; }
        if (control(c) && !within(title, c) || apart.includes(c)) continue;
        if (apartInside(c)) {
          if (passed && !subtitled && glyphLine(c)) { placed.push(c); subtitled = drawn = true; continue; }
          if (!(passed && filled(c))) titleWalk(c);
          continue;
        }
        if (wrapper && !passed && !avatar && !within(title, c) && filled(c) && children(c).some(face)) { placed.push(c); avatar = drawn = true; continue; }
        if (passed && !subtitled && paragraph(c) && text(c) !== '') { placed.push(c); subtitled = true; continue; }
        if (passed && !subtitled && glyphLine(c)) { placed.push(c); subtitled = drawn = true; continue; }
        if (passed && filled(c)) continue;
        titleWalk(c);
      }
    };
    titleWalk(wrapper ?? header);
    // Segments occupy the title view; only UIKit's plain subtitle can coexist with them.
    if (!(tabs && projects(tabs) && drawn)) for (const n of placed) include(n, projects);
  }
  // A box that holds only projected content (a column around the title) is the bar's too: its authored size is
  // not drawn on iOS. One holding anything else, an opted-out role included, stays judged.
  const represented = n => {
    if (projected.has(n.id)) return true;
    const kids = children(n).filter(c => !layer(c)), all = kids.map(represented);
    if (kids.length && all.every(Boolean)) projected.add(n.id);
    return projected.has(n.id);
  };
  headers.forEach(represented);
  return projected;
}

/** The navigation root's tab bar, which UIKit's tab bar controller draws in the web boxes' place (NavigationTabs.swift):
 * the root's own `tablist` with a tab naming one of the root's `tabpanel`s, and what it holds but those panels. UITabBar keeps its items at
 * one size at every text size and offers the large content viewer instead (Apple HIG, "Tab bars"), so the web's bar,
 * which grows with the root font, does not measure it. A tablist with `appearance="none"` is the app's own bar (LLP
 * 1075.003 §9.10) and stays judged. Read with the same root rule as `iosNavigationContent`. */
export function iosTabBar(nodes) {
  const children = n => (n?.children ?? []).map(id => nodes.get(id)).filter(Boolean);
  const parents = new Map([...nodes.values()].flatMap(n => children(n).map(c => [c.id, n])));
  const root = [...nodes.values()].find(n => !parents.has(n.id)), bar = new Set();
  if (root?.props?.navigationKey == null || root.props.navigationBack == null) return bar;
  const role = n => n.props?.accessibilityRole;
  const owner = n => {
    let at = parents.get(n.id);
    while (at && at.props?.navigationKey == null) at = parents.get(at.id);
    return at;
  };
  const panels = new Set([...nodes.values()].filter(n => role(n) === 'tabpanel' && n.props?.id != null && owner(n) === root).map(n => n.props.id));
  const list = [...nodes.values()].find(n => role(n) === 'tablist' && owner(n) === root && children(n).some(t => role(t) === 'tab' && panels.has(t.props?.accessibilityControls)));
  if (!list || (list.style?.appearance?.value === 'none' && ['authored', 'dynamic'].includes(list.style.appearance.source))) return bar;
  // A tablist that holds the root's panels keeps them, and their routes, in the page, and so the boxes around them
  // (NavigationCoversIOS.swift `covered`): only what holds no panel is the bar's.
  const kept = new Set([...nodes.values()].filter(n => role(n) === 'tabpanel' && panels.has(n.props?.id)).map(n => n.id));
  const holds = n => kept.has(n.id) || children(n).some(holds);
  const add = n => { bar.add(n.id); children(n).forEach(add); };
  const cover = n => { if (!holds(n)) add(n); else children(n).filter(c => !kept.has(c.id)).forEach(cover); };
  cover(list);
  return bar;
}

const WRAP = 'wrap the row: flex-wrap="wrap" with a flex-basis in rem on its items (docs/recipes/responsive-columns.md)';
/** The remedy for a finding at the largest size: never a smaller or fixed font. A text in a list's row (`inRow`) is cut by
 * the row, which wrapping the row does not change: at the accessibility sizes UIKit's cells wrap a label and grow taller,
 * never truncating it, so a `line-clamp` there reads as a tell (round 22's book-club and contacts took one from this
 * remedy). A grouped list's standard cell already hyphenates as its label does. Elsewhere a UILabel wraps at words and breaks
 * between syllables only a word longer than its line (iOS 27), so `hyphens="auto"` is the remedy for a text of one word;
 * on a heading or sentence it hyphenates words a UILabel keeps whole, which round 26's graders named a tell. */
export function largeRemedy(f, { inRow = false } = {}) {
  const wide = (f.detail?.need ?? f.detail?.cut ?? [0])[0] > 0;
  if (f.kind === 'clipped' && inRow && !f.detail?.edge) {
    return `${wide ? 'give the text more of the row (no px width beside it) and ' : ''}let the row grow taller with its text, as a cell does at this size (no px height on the row or what it holds; no \`line-clamp\`, which cuts what a cell shows whole)`;
  }
  const oneWord = typeof f.text === 'string' && f.text.trim() !== '' && !/\s/.test(f.text.trim());
  if (f.kind === 'clipped') return f.detail?.edge ? 'let the page scroll there (a scroll with flex=1 min-height=0)'
    : wide && f.detail?.need ? `give it room (no px width on its box), or ${WRAP}; ${oneWord ? 'a word longer than any line breaks between syllables with `hyphens="auto"`, as a UILabel breaks it' : 'not `hyphens="auto"`, which breaks words a UILabel wraps whole'}`
    : f.detail?.need ? 'let its box grow with its text: no px height on it (a rem size, or none)' : `give the box that cuts it room (no px width or height on it), or ${WRAP}`;
  if (f.kind === 'squeezed') return `let it keep its words (no min-width: 0 or px width beside it), or ${WRAP}`;
  // Overlapping cells of a row that stays one line (a week's days) wrap nowhere: UIKit's calendar caps its day numbers, which
  // grow to 31 points and no further (iOS 27), never shrinking them.
  if (f.kind === 'overlap') return `${WRAP}; for a row of equal cells that must stay one line (a week's days), cap the size its text grows to, as UIKit's calendar does: font-size="min(-exact-subheadline, 31px)"`;
  return WRAP;
}
