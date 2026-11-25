// The root's tab bar (LLP 1115.002 item 5): wide, a sidebar; along the window's bottom, a bar whose tabs keep inside
// the safe area while the box that ends at its top has no bottom inset (LLP 1123 D9; the rules are neutral.css's).
import { viewChildren, viewParent } from "./lists.js";

export const inFlow = style => style.display !== "none" && !/^(absolute|fixed)$/.test(style.position);
const flow = style => /^(static|relative)$/.test(style.position), takesRoom = el => inFlow(getComputedStyle(el));
const column = style => /flex/.test(style.display) && style.flexDirection.startsWith("column");
const placed = new WeakMap();

// Only the nav containing the root tabs and their footer belongs to the sidebar. Another nav, or a footer inside a
// route, keeps its place in the document.
export function placeTabBar(root, list, wide, moved = false) {
  const parent = viewParent(list);
  const children = parent?.localName === "nav" && viewParent(parent) === root ? viewChildren(parent) : [];
  const footer = children.length === 2 ? children.find(el => el !== list && el.localName === "footer") : null;
  const wrapper = footer ? parent : null, orientation = list?.getAttribute("aria-orientation");
  const siblings = viewChildren(root), placement = [wide, parent === root, orientation, ...[root, wrapper, list, footer, ...siblings].map(el => el && [
    el.className, el.style.display, el.style.flexDirection, el.style.position, el.style.appearance, el.style.order, el.style.paddingBottom
  ].join("|"))].join("/");
  const last = placed.get(root);
  if (!moved && last?.placement === placement && last.list === list && last.wrapper === wrapper && last.siblings.length === siblings.length && last.siblings.every((el, i) => el === siblings[i])) return;

  // Read authored placement without our own CSS. An unrelated commit must not restyle the page.
  for (const [el, mark] of [[root, "data-exact-sidebar"], [last?.list, "data-exact-sidebar-tabs"], [last?.list, "data-exact-tab-bar"], [last?.above, "data-exact-above-tab-bar"], [last?.wrapper, "data-exact-tab-navigation"]]) el?.removeAttribute(mark);
  const entry = { placement, list, wrapper, siblings, wide, watch: last?.watch, edges: last?.edges };
  placed.set(root, entry);
  if (list) follow(root, entry);
  else entry.watch?.disconnect();
  place(root, list, wrapper, footer, parent, orientation, siblings, wide, entry);
}

function place(root, list, wrapper, footer, parent, orientation, siblings, wide, entry) {
  if ((!wrapper && parent !== root) || orientation === "vertical") return;
  const bar = getComputedStyle(list), box = wrapper ?? list, edge = box.getBoundingClientRect(), ends = siblings.map(el => el.getBoundingClientRect().bottom);
  if (bar.appearance === "none" || !bar.flexDirection?.startsWith("row") || !column(getComputedStyle(root))) return;
  // The root's shown bar that ends at the window's bottom, narrow or kept a bar by its authored position, keeps its
  // tabs in the safe area, and the box that ends at its top has no bottom inset
  const along = inFlow(bar) && (!wide || !flow(bar)) && takesRoom(box) && edge.top < innerHeight - 1 && Math.abs(edge.bottom - innerHeight) < 1;
  if (list.toggleAttribute("data-exact-tab-bar", along)) (entry.above = siblings.findLast((el, i) => el !== box && takesRoom(el) && Math.abs(ends[i] - edge.top) < 1))?.setAttribute("data-exact-above-tab-bar", "");
  if (!flow(bar) || (!wide && !wrapper)) return;
  if (wrapper) {
    const style = getComputedStyle(wrapper);
    if (!flow(style) || !(style.display === "block" || style.display === "contents" || column(style)) || !flow(getComputedStyle(footer))) return;
    wrapper.setAttribute("data-exact-tab-navigation", bar.display === "none" ? "hidden" : wide ? "sidebar" : "bar");
  }
  root.toggleAttribute("data-exact-sidebar", wide);
  list.toggleAttribute("data-exact-sidebar-tabs", wide);
}

/** The edges the marks are decided by, to the whole pixel: the window's bottom, the bar's top and bottom, and each of
 * the root's children's bottoms. */
function edges(box, siblings) {
  // The server renderer's DOM lays nothing out
  const bar = box?.getBoundingClientRect?.();
  return [globalThis.innerHeight, bar?.top, bar?.bottom, ...siblings.map(el => el.getBoundingClientRect?.().bottom)].map(v => Math.round(v ?? 0)).join(",");
}

/** Places the bar again where the root or one of its children changes size and the edges its marks read move, as a box
 * after the bar that grows lifts it off the window's bottom with no commit. Read after layout, the edges cost none. */
function follow(root, entry) {
  if (!globalThis.ResizeObserver) return;
  entry.watch ??= new ResizeObserver(() => {
    const now = placed.get(root), seen = now && root.isConnected && edges(now.wrapper ?? now.list, now.siblings);
    if (!seen || seen === now.edges) return;
    now.edges = seen;
    placeTabBar(root, now.list, now.wide, true);
  });
  entry.watch.disconnect();
  for (const el of [root, ...entry.siblings]) entry.watch.observe(el);
}
