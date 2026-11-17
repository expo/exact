import {createHash} from 'node:crypto';

const canonical = value => Array.isArray(value) ? value.map(canonical)
  : value && typeof value === 'object' ? Object.fromEntries(Object.keys(value).sort().map(k => [k, canonical(value[k])])) : value;
export const digest = value => createHash('sha256').update(JSON.stringify(canonical(value)) ?? 'undefined').digest('hex');
export const blind = (source, reason) => ({kind: 'blind-spot', source, reason});
export const verdict = findings => findings.some(f => f.kind === 'blind-spot') ? 'inconclusive' : findings.length ? 'disagreement' : 'agreement';

export function modelElements(tree) {
  return (tree.nodes ?? []).map(n => {
    const p = n.props ?? {};
    return {identifier: p.testId, label: p.accessibilityLabel ?? p.text ?? n.text,
      type: p.semanticTag ?? n.kind, node: n.id, unshown: n.unshown,
      enabled: ![true, 'true'].includes(p.disabled)};
  }).filter(n => n.identifier || n.label);
}

export function externalElements(tree) {
  return (tree.elements ?? []).map(n => ({...n,
    label: n.label || n.title || (/StaticText|AXStaticText/.test(n.type) ? n.value : undefined),
  })).filter(n => n.identifier || n.label);
}

export function compareTrees(inside, outside) {
  const findings = [], joins = [], used = new Set();
  for (const a of inside) {
    const exact = a.identifier ? outside.filter(b => b.identifier === a.identifier) : [];
    const byName = !a.identifier && a.label ? outside.filter(b => !b.identifier && b.label === a.label) : [];
    const choices = exact.length ? exact : byName;
    const ownCount = inside.filter(b => exact.length ? b.identifier === a.identifier : !b.identifier && b.label === a.label).length;
    if (choices.length > 1 || choices.length === 1 && ownCount > 1) {
      findings.push(blind('tree', `ambiguous ${exact.length ? 'identifier' : 'name'}: ${a.identifier ?? a.label}`));
      continue;
    }
    const b = choices[0];
    if (!b || used.has(b)) { findings.push({kind: 'inside-only', inside: a}); continue; }
    used.add(b);
    const via = exact.length ? 'identifier' : 'name-only';
    joins.push({inside: a, outside: b, via});
    for (const field of ['label', 'enabled']) {
      if (a[field] === undefined) continue;
      if (field === 'enabled' && b[field] === undefined) {
        findings.push(blind('tree', `enabled state was not exposed for ${a.identifier ?? a.label}`));
        continue;
      }
      const observed = b[field] ?? '';
      if (a[field] !== observed)
        findings.push({kind: 'tree-field', target: a.identifier ?? a.label, field, inside: a[field], outside: observed, via});
    }
  }
  for (const b of outside) if (!used.has(b)) findings.push({kind: 'outside-only', outside: b});
  return {findings, joins};
}

export function imageFacts(image) {
  const {width, height, data} = image;
  if (!width || !height || data.length !== width * height * 4) throw new Error('empty or malformed decoded image');
  let uniform = true;
  for (let p = 4; p < data.length; p += 4) {
    if ([0, 1, 2, 3].some(c => data[p + c] !== data[c])) { uniform = false; break; }
  }
  return {width, height, uniform, digest: createHash('sha256').update(data).digest('hex')};
}

export function comparePixels(a, b, source) {
  if (a.width !== b.width || a.height !== b.height) return [
    {kind: 'capture-extent', source, inside: [a.width, a.height], outside: [b.width, b.height]},
    blind(source, 'capture regions have different extents; no coordinate mapping was established'),
  ];
  let changed = 0, minX = a.width, minY = a.height, maxX = -1, maxY = -1;
  for (let p = 0; p < a.width * a.height; p++) {
    if (![0, 1, 2, 3].some(c => a.data[p * 4 + c] !== b.data[p * 4 + c])) continue;
    changed++;
    const x = p % a.width, y = Math.floor(p / a.width);
    minX = Math.min(minX, x); minY = Math.min(minY, y); maxX = Math.max(maxX, x); maxY = Math.max(maxY, y);
  }
  return changed ? [{kind: 'pixels', source, changed, total: a.width * a.height,
    bounds: [minX, minY, maxX - minX + 1, maxY - minY + 1]}] : [];
}

export function targetPoint(tree, target) {
  const found = externalElements(tree).filter(e => e.identifier === target);
  if (found.length !== 1) throw new Error(`outside target ${target}: ${found.length} matches`);
  const e = found[0], f = e.frame;
  if (!f || f.length !== 4 || !f.every(Number.isFinite) || f[2] <= 0 || f[3] <= 0 || e.enabled === false)
    throw new Error(`outside target ${target}: no enabled, nonempty frame`);
  return {point: [f[0] + f[2] / 2, f[1] + f[3] / 2], element: e, via: 'identifier'};
}

export function stableFindings(before, after, source) {
  return digest(before) === digest(after) ? [] : [blind(source, 'observation changed across the checkpoint')];
}
