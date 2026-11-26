// Where the device keeps content clear (LLP 1188 D4): a control, or a text, that lies outside its owner's safe area where no
// scroller can bring it in (`edge`), or across an active division, the band UIKit keeps clear along the hinge while the phone
// is half open, where no scroller holds it (`hinge`). The iOS host says, under UIKit's own bars, whose safe area an item is
// in (`owner`, a place in `obs.owners`: the owner's box and its `safe` area, in the window), whether a scroller holds it
// (`scrolls`) and how far one can move it from each edge (`room`), what its clips leave of it (`clip`), and the window's
// active divisions, with the margins UIKit keeps clear (`obs.divisions`); this file judges.

const EDGES = {
  top: { axis: 'y', at: (r) => r.y, inward: 1 },
  bottom: { axis: 'y', at: (r) => r.y + r.h, inward: -1 },
  left: { axis: 'x', at: (r) => r.x, inward: 1 },
  right: { axis: 'x', at: (r) => r.x + r.w, inward: -1 },
};
const middle = (r, axis) => (axis === 'y' ? r.y + r.h / 2 : r.x + r.w / 2);
/** A box as large as its owner's safe area is the screen's surface (a map, a tap anywhere), not something on it. */
const isSurface = (r, owner) => r.w >= owner.safe.w - 0.5 && r.h >= owner.safe.h - 0.5;

/** How far `r` lies past the owner's safe area at `edge` and how deep the area there is, or null: none of it does, or it is
 *  drawn through to the owner's edge with its middle clear, as a bar's fill is. */
function past(r, owner, edge) {
  const { axis, at, inward } = EDGES[edge], safe = at(owner.safe), end = at(owner);
  const depth = (safe - end) * inward, by = Math.min((safe - at(r)) * inward, depth);
  if (by < 2) return null;
  const reaches = (at(r) - end) * inward <= 0.5, clear = (middle(r, axis) - safe) * inward >= 0;
  return reaches && clear ? null : { edge, by, depth };
}

/** Where two rectangles meet; a size of nothing or less where they do not. */
export const meet = (a, b) => { const x = Math.max(a.x, b.x), y = Math.max(a.y, b.y); return { x, y, w: Math.min(a.x + a.w, b.x + b.w) - x, h: Math.min(a.y + a.h, b.y + b.h) - y }; };

/** The `edge` and `hinge` findings of one observation: `{ it, kind, detail }`, at most one of each kind an item. */
export function regionFindings(obs) {
  const found = [];
  for (const it of obs.items ?? []) {
    const owner = obs.owners?.[it.owner];
    // A control's own label goes with it
    if (!owner || !(it.control || it.text) || (it.text && it.of != null)) continue;
    if (it.cover?.kind === 'alert' || it.cover?.kind === 'sheet') continue; // behind what is presented
    // What its clips leave of a control's box, or of where a text is drawn
    const rects = (it.control ? [it] : it.lines ?? [it.ink ?? it]).map((r) => (it.clip ? meet(r, it.clip) : r)).filter((r) => r.w > 0 && r.h > 0 && !isSurface(r, owner));
    // What lies over an item at an edge is `covered`, and the keys cover the band below them
    const covered = it.cover && !it.disabled, keys = obs.keyboard != null && obs.keyboard < owner.safe.y + owner.safe.h;
    const edges = covered ? [] : Object.keys(EDGES).filter((edge) => !(edge === 'bottom' && keys))
      .flatMap((edge) => rects.map((r) => past(r, owner, edge)).filter((p) => p && (it.room?.[edge] ?? 0) < p.by));
    const worst = edges.sort((a, b) => b.by - a.by)[0];
    if (worst) found.push({ it, kind: 'edge', detail: worst });
    // What a scroller holds runs across the hinge, as UIKit's own lists do: all of it where it has room to move, and in one that
    // fits its rows, which cross the hinge and are at least half as long as the safe area is
    const runs = (r, d) => (d.h >= d.w ? r.x < d.x && r.x + r.w > d.x + d.w && r.w * 2 >= owner.safe.w : r.y < d.y && r.y + r.h > d.y + d.h && r.h * 2 >= owner.safe.h);
    const division = it.room ? null : (obs.divisions ?? []).find((d) => rects.some((r) => { const m = meet(r, d); return m.w > 0.5 && m.h > 0.5 && !(it.scrolls && runs(r, d)); }));
    if (division) found.push({ it, kind: 'hinge', detail: division.h >= division.w ? { axis: 'x', from: division.x, to: division.x + division.w } : { axis: 'y', from: division.y, to: division.y + division.h } });
  }
  return found;
}

/** Whether a scroller can bring `b` out from under `a`, which it does not move, and leave it in its owner's safe area. */
export function clears(obs, b, a) {
  const safe = obs.owners?.[b.owner]?.safe;
  if (!safe || !b.room || a.room) return false;
  const to = (edge, dx, dy) => {
    const x = b.x + dx, y = b.y + dy;
    return (b.room[edge] ?? 0) >= Math.abs(dx + dy) && x >= safe.x && y >= safe.y && x + b.w <= safe.x + safe.w && y + b.h <= safe.y + safe.h;
  };
  return to('bottom', 0, a.y - b.y - b.h) || to('top', 0, a.y + a.h - b.y) || to('right', a.x - b.x - b.w, 0) || to('left', a.x + a.w - b.x, 0);
}

/** A finding's own words, and what to do. */
export const regionLine = (kind, d, unit) => kind === 'hinge'
  ? `the hinge runs through it (${d.axis} ${Math.round(d.from)} to ${Math.round(d.to)} ${unit})`
  : `${Math.round(d.by)} ${unit} of it lie outside the safe area, which ends ${Math.round(d.depth)} ${unit} from the ${d.edge}`;
export const regionHint = (kind) => (kind === 'hinge' ? 'move it to one side of the hinge' : 'place it in the safe area') + ' (contract vocab edges)';
