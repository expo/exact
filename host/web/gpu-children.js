// Canvas child placement and CSS composition (LLP 1014 D5, LLP 1133.039 D8).
// A surface's fade of a placed child rides a custom property no author writes,
// and one rule multiplies it into the child's picture without taking over the
// author's `opacity`, `mask` or animations. The rule outranks the shell's
// (`all: unset` / `all: revert`) and yields to the author's (`#exact-root#exact-root`
// or inline), so an author's own `filter` wins: the web's known difference.
export const PLACEMENT_OPACITY = "--exact-placement-opacity";
export const placementStyle = `#exact-root#exact-root :where([style*="${PLACEMENT_OPACITY}"]){filter:opacity(var(${PLACEMENT_OPACITY}))}`;

// A homography maps child-local points to canvas points. CSS adds the child's
// kernel offset after its transform, so subtract that offset in homogeneous space.
function childFrames(entry) {
  const children = [...(entry.host.children ?? [])].filter(el => !el.hasAttribute("data-surface"));
  return children.map(el => ({ el, name: el.getAttribute("data-testid") ?? "", frame: [el.offsetLeft, el.offsetTop, el.offsetWidth, el.offsetHeight] }));
}

function placementOpacity(el, factor) {
  const value = factor === 1 ? "" : String(factor);
  if (el.style.getPropertyValue(PLACEMENT_OPACITY) !== value) el.style.setProperty(PLACEMENT_OPACITY, value);
}

export function restoreChild(row) {
  if (!row.original) return;
  placementOpacity(row.el, 1);
  Object.assign(row.el.style, row.original.style);
  row.el.inert = row.original.inert;
  delete row.el.exactPlaced;
  delete row.original;
}

export function supplyChildren(entry, module, staging = false, force = staging) {
  if (module.gpu_children_mode?.(entry.id) !== 3) {
    if (!staging) for (const row of entry.children ?? []) restoreChild(row);
    if (entry.children) {
      module.gpu_children_count(entry.id, 0);
      if (!staging) entry.el.style.zIndex = "-1";
    }
    entry.children = undefined;
    return;
  }
  const previous = entry.children ?? [];
  const rows = childFrames(entry);
  if (!staging) for (const row of previous) if (!rows.some(next => next.el === row.el)) restoreChild(row);
  for (const [i, row] of rows.entries()) {
    const old = previous.find(old => old.el === row.el);
    row.original = old?.original;
    row.hidden = old?.hidden;
    if (force || previous[i]?.el !== row.el || previous[i]?.name !== row.name || row.frame.some((n, j) => n !== previous[i].frame[j]))
      module.gpu_child_view(entry.id, i, row.name, ...row.frame);
  }
  if (force || !entry.children || previous.length !== rows.length) module.gpu_children_count(entry.id, rows.length);
  entry.children = rows;
}

// The child's box, in its own points, cut by a placement's two clip rows
// (each ax + by + c >= 0), as Linux clips its composite: "" when they cut nothing.
function clipPath(h, width, height) {
  let points = [[0, 0], [width, 0], [width, height], [0, height]], cut = false;
  for (const [a, b, c] of [h.subarray(10, 13), h.subarray(13, 16)]) {
    const d = ([x, y]) => a * x + b * y + c, next = [];
    points.forEach((q, i) => {
      const p = points.at(i - 1), dp = d(p), dq = d(q);
      if ((dp >= 0) !== (dq >= 0)) next.push([p[0] + (q[0] - p[0]) * dp / (dp - dq), p[1] + (q[1] - p[1]) * dp / (dp - dq)]);
      if (dq >= 0) next.push(q); else cut = true;
    });
    points = next;
  }
  return cut ? `polygon(${(points.length ? points : [[0, 0]]).map(([x, y]) => `${x}px ${y}px`).join(",")})` : "";
}

export function placeChildren(entry, module) {
  if (!entry.children) return;
  const h = new Float32Array(16), placed = [];
  for (const [i, row] of (entry.children ?? []).entries()) {
    // Rows that cut nothing, for a module that writes only the first ten floats.
    h.set([0, 0, 1, 0, 0, 1], 10);
    const outcome = module.gpu_placement(entry.id, i, h), el = row.el;
    row.hidden = outcome === 2;
    if (outcome === 0) { restoreChild(row); continue; }
    if (!row.original) {
      row.original = {
        style: Object.fromEntries(["transform", "transformOrigin", "zIndex", "visibility", "position"].map(k => [k, el.style[k]])),
        inert: el.inert,
        staticPosition: getComputedStyle(el).position === "static",
      };
      el.exactPlaced = true;
    }
    el.style.visibility = outcome === 2 ? "hidden" : row.original.style.visibility;
    el.inert = outcome === 2 || row.original.inert;
    // Read only for a child the surface places and shows; a module without the export fades nothing.
    const opacity = outcome === 2 ? 1 : module.gpu_placement_opacity?.(entry.id, i);
    placementOpacity(el, Number.isFinite(opacity) ? Math.max(0, Math.min(1, opacity)) : 1);
    if (outcome === 2) continue;
    const [x, y] = row.frame;
    el.style.transformOrigin = "0 0";
    // Relative positioning makes z-index apply to ordinary block children too.
    if (row.original.staticPosition) el.style.position = "relative";
    el.style.transform = `matrix3d(${[h[0] - x * h[6], h[3] - y * h[6], 0, h[6], h[1] - x * h[7], h[4] - y * h[7], 0, h[7], 0, 0, 1, 0, h[2] - x * h[8], h[5] - y * h[8], 0, h[8]].join(",")})`;
    // The authored clip-path is kept, and restored, once a placement has cut the child.
    const clip = clipPath(h, row.frame[2], row.frame[3]);
    if (clip) row.original.style.clipPath ??= el.style.clipPath ?? "";
    if ("clipPath" in row.original.style) el.style.clipPath = clip || row.original.style.clipPath;
    placed.push({ el, depth: h[9], index: i });
  }
  // Integer CSS ranks preserve the full float ordering. At equal depth the later
  // Contract child draws/hits last, on native and web alike.
  placed.sort((a, b) => a.depth - b.depth || a.index - b.index);
  entry.el.style.zIndex = String(-placed.length - 1);
  placed.forEach((row, i) => { row.el.style.zIndex = String(i - placed.length); });
}
