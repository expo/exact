// @ref LLP 1115.000 D4.8 — how the sheets of a stack stand to each other in a wide window, as UIKit's form sheets do
// on an iPad (iPadOS 27; the measurements are the schema's `_metrics`): which sheets are one size, the step back of
// a sheet under one its size, the lengths neutral.css makes the step from, and what a sheet of another size dims
// again. routes.js `sheetFrame` and `sheetLeaves` call it, for a plan that can present a sheet.
import { viewChildren, viewParent } from "./lists.js";

const mark = (route, name, value) => {
  if ((route.getAttribute(name) ?? null) !== value) {
    value === null ? route.removeAttribute(name) : route.setAttribute(name, value);
  }
};
// The values the stack gives a route, or the routes' parent (neutral.css), each in a rule of a style sheet the host
// owns: a route's inline style is its author's, and a host writes it whole. A rule goes with its last value, or once
// its element has left the document.
const sheetLengths = new Map();
let lengthSheet = null, lengthRules = 0;
function sheetLength(route, name, value) {
  let rule = sheetLengths.get(route);
  if (!rule) {
    const root = route.getRootNode();
    if (!value || !globalThis.CSSStyleSheet || !root.adoptedStyleSheets) {
      return;
    }
    lengthSheet ??= new CSSStyleSheet();
    if (!root.adoptedStyleSheets.includes(lengthSheet)) {
      root.adoptedStyleSheets = [...root.adoptedStyleSheets, lengthSheet];
    }
    route.setAttribute("data-exact-sheet-geometry", String(++lengthRules));
    rule = lengthSheet.cssRules[lengthSheet.insertRule(`[data-exact-sheet-geometry="${lengthRules}"]{}`, lengthSheet.cssRules.length)];
    sheetLengths.set(route, rule);
  }
  if (rule.style.getPropertyValue(name) !== value) {
    rule.style.setProperty(name, value);
  }
  if (!rule.style.length || !route.isConnected) {
    lengthSheet.deleteRule([...lengthSheet.cssRules].indexOf(rule));
    sheetLengths.delete(route);
    route.removeAttribute("data-exact-sheet-geometry");
  }
}
// Whether a route's transform is its author's, which the step must leave alone, read from what applies, however a
// target wrote it (inline, or as a class). The look gives a sheet no transform or rotation and its box's centre for
// an origin, so any other is the author's, and so is any animation or transition on the route that touches one of
// the five, whatever its first frame, but the look's own: a transition of the scale or the translation that ends at
// the step's or at none (the step, its return, a slide). The scale and the translation are the step's while the host
// steps the sheet and none otherwise. The step's two are registered here (`--exact-sheet-step-scale`, `-rise`,
// neutral.css), as a number and a length, so what they come to is read beside what applies: where the two differ on
// a stepped sheet, a declaration of the author's has them, an identity one included (`scale: 1`), which nothing
// shows on a sheet the host is not stepping. Nothing is remembered of it: each read gives such a sheet the step
// and takes it off again (`sheetStep`), so whose the scale is follows the rules that apply, a media query's or an
// ancestor's included. Where a browser registers no property, a stepped sheet's two are taken as the step's.
let stepRegistered = false;
function registerStep() {
  stepRegistered = true;
  for (const [name, syntax, initialValue] of [["--exact-sheet-step-scale", "<number>", "1"], ["--exact-sheet-step-rise", "<length>", "0px"]]) {
    try {
      globalThis.CSS?.registerProperty?.({ name, syntax, inherits: false, initialValue });
    } catch {
      // registered already, by a module this one replaced
    }
  }
}
const TRANSFORMS = ["transform", "rotate", "scale", "translate", "transformOrigin"], STEP = ["--exact-sheet-step-scale", "--exact-sheet-step-rise"];
// A scale (`x y z`, 1 for none) or a translation (0 for none) that is `x` and `y` and nothing more, within `by`.
const isPair = (text, x, y, rest, by) => {
  const [a, b, c = rest] = text === "none" ? [rest, rest] : text.split(" ").map(parseFloat);
  return Math.abs(a - x) < by && Math.abs((b ?? (rest === 1 ? a : rest)) - y) < by && Math.abs(c - rest) < by;
};
// The route's scale and translation and the step's two (`step`), each where a transition is taking it, and whether
// anything else animates one of the five. An author's `transition: all` takes the registered two as well where the
// browser transitions a registered property (WebKit), so those are read at their ends too.
function applied(route, css) {
  const now = { scale: css.scale, translate: css.translate, step: STEP.map((name) => css.getPropertyValue(name)), animated: false };
  for (const animation of route.getAnimations()) {
    const frames = animation.effect?.getKeyframes?.() ?? [], touched = TRANSFORMS.filter((name) => frames.some((frame) => name in frame)), step = STEP.indexOf(animation.transitionProperty);
    if (step >= 0) {
      now.step[step] = String(frames.at(-1)?.[STEP[step]] ?? now.step[step]);
    } else if (touched.length === 1 && touched[0] === animation.transitionProperty && touched[0] in now) {
      now[touched[0]] = String(frames.at(-1)[touched[0]]);
    } else if (touched.length) {
      now.animated = true;
    }
  }
  return now;
}
// Whether the step's scale and translation are what the stepped route has, or is going to.
function stepHolds(now) {
  const [scale, rise] = now.step.map(parseFloat);
  return Number.isNaN(scale) || Number.isNaN(rise) || isPair(now.scale, scale, scale, 1, 1e-4) && isPair(now.translate, 0, -rise, 0, .05);
}
function ownTransform(route, css, width, height) {
  const now = applied(route, css), [x, y, z = 0] = css.transformOrigin.split(" ").map(parseFloat);
  if (now.animated || css.transform !== "none" || css.rotate !== "none" || Math.abs(x - width / 2) >= .05 || Math.abs(y - height / 2) >= .05 || z !== 0) {
    return true;
  }
  return route.hasAttribute("data-exact-sheet-step") ? !stepHolds(now) : !(isPair(now.scale, 1, 1, 1, 1e-4) && isPair(now.translate, 0, 0, 0, .05));
}
// The step back is the host's scale and translation (`data-exact-sheet-step`, neutral.css) on a sheet under one of
// its own size whose transform is not its author's (`box.own`). When either stops being so the mark goes: each
// property goes back by the look's own transition, from where it was, or is the author's from then on. A step just
// given is read back at once, before a frame shows it: where it does not hold, the author wrote the scale or the
// translation, and the step is taken off again. That is one reading of the route's style, no layout, at each read
// of a stack for a sheet the step does not hold on, and none for one it does. The lengths the step is made of are
// a sheet's for as long as one of its own size is over it, stepped or not, so that trying the step writes one mark
// and no rule.
function sheetStep(route, box) {
  const under = route.getAttribute("data-exact-under-sheet") === "same" && !!box, steps = under && !box.own, fresh = steps && !route.hasAttribute("data-exact-sheet-step");
  sheetLength(route, "--exact-sheet-stack-width", under ? `${box.width}px` : "");
  sheetLength(route, "--exact-sheet-stack-height", under ? `${box.height}px` : "");
  mark(route, "data-exact-sheet-step", steps ? "" : null);
  if (fresh) {
    const css = getComputedStyle(route);
    if (!stepHolds(applied(route, css))) {
      route.removeAttribute("data-exact-sheet-step");
    }
  }
}
// A wide sheet's border box as layout used it, unrounded, from computed values, which no slide or step back moves
// (the look makes a sheet `border-box`), its height alone, and, for a route the stack may step (`judged`), whether
// its transform is its author's (`own`). A sheet over a sheet gives up twice the stack's offset of its height
// (`--exact-sheet-stack-offset`, neutral.css), as far as what its author wrote lets it. A card's `max-height` is
// the height the look gives one: it is that tall alone, or as tall as it is where that is more. A sheet as tall as
// its content (`--exact-sheet-stack-fits`, a dialog) gives the room up from the third row of its frame, so one
// whose frame is the route is that much taller alone where the row is the look's and no `min-height` of the route's
// holds it; any other is measured (`frames`, `sheetAlone`), as what its author wrote on a component's box may hold
// it too. Null where the look states no offset: the narrow form, which compares no sizes.
function sheetBox(route, judged) {
  const css = getComputedStyle(route), offset = parseFloat(css.getPropertyValue("--exact-sheet-stack-offset"));
  if (Number.isNaN(offset)) {
    return null;
  }
  const width = parseFloat(css.width), height = parseFloat(css.height), fits = offset > 0 ? css.getPropertyValue("--exact-sheet-stack-fits").trim() : "";
  let alone = height, frames = null;
  if (fits) {
    const rows = css.gridTemplateRows.split(" "), least = css.minHeight === "auto" ? 0 : css.minHeight.endsWith("px") ? parseFloat(css.minHeight) : NaN;
    alone += 2 * offset;
    if (fits !== "route" || rows.length !== 4 || parseFloat(rows[2]) !== 0 || !(height - least >= .05)) {
      frames = fits === "route" ? [route] : [route, viewChildren(route)[0]].filter(Boolean);
    }
  } else if (offset > 0) {
    alone = Math.max(height, parseFloat(css.maxHeight));
  }
  return { width, height, alone, frames, own: judged && ownTransform(route, css, width, height) };
}
// How tall a sheet that has given some of its height up is alone: laid out once with its room
// (`data-exact-sheet-alone`, neutral.css), which says it whatever its author wrote on it or in it. While the mark
// is on, a transition that would start takes no time and one under way runs on (the sheet's slide). It comes off in
// two steps, the second once the style is as it was, so that a transition of the author's starts on neither change.
function sheetAlone(route, frames) {
  for (const frame of frames) {
    frame.setAttribute("data-exact-sheet-alone", "");
  }
  const css = getComputedStyle(route), alone = parseFloat(css.height);
  for (const frame of frames) {
    frame.setAttribute("data-exact-sheet-alone", "given");
  }
  void css.maxHeight;
  for (const frame of frames) {
    frame.removeAttribute("data-exact-sheet-alone");
  }
  return alone;
}
// A sheet's size can change with no projection (an image loads in it, its window is resized, across the look's
// breakpoint or not), and so can whose its transform is (an animation of the author's starts or ends on it; a media
// query's rule comes or goes, for which `stackSheets` hears the window). So a stack's sheets are read again, as its
// last projection framed them, when a route of a sheet in the stack is resized or an animation starts or ends on it:
// every such route is watched, in either form of the look, until it leaves the stack. A transition's events read
// nothing: the step's own are transitions, and one a read starts would start a read. A route that projection no
// longer holds (one leaving, one removed) is let go.
const lastFramed = new WeakMap(), watched = new Set(), pending = new Set(), stacks = new Set();
let sizeWatch = null, stackFrame = 0;
// What left the document takes what the stack held for it: a route is watched no longer, an element's rule goes,
// and a parent's stack is forgotten, with the window's listener after the last. So is the stack of a parent that
// stays where every route its last projection framed has left it (a root whose routes gave way to a placeholder),
// and what it dimmed the page by goes with it. An observer hears a route removed (its box is gone), and every
// projection, read and resize of the window looks too.
function sweep() {
  for (const route of watched) {
    if (!route.isConnected) {
      watched.delete(route);
      sizeWatch?.unobserve(route);
    }
  }
  for (const element of sheetLengths.keys()) {
    element.isConnected || sheetLength(element, "", "");
  }
  for (const parent of stacks) {
    if (!parent.isConnected || !lastFramed.get(parent)?.[0].some((route) => viewParent(route) === parent)) {
      parent.isConnected && sheetLength(parent, "--exact-sheet-stack-page-dims", "");
      pending.delete(parent);
      lastFramed.delete(parent);
      unstack(parent);
    }
  }
}
function watchSizes() {
  if (sizeWatch === null && globalThis.ResizeObserver) {
    sizeWatch = new ResizeObserver((entries) => {
      const framed = new Set();
      sweep();
      for (const { target } of entries) {
        const stack = target.isConnected && lastFramed.get(viewParent(target));
        if (stack && stack[0].includes(target)) {
          framed.add(stack);
        } else if (watched.delete(target)) {
          sizeWatch.unobserve(target);
        }
      }
      for (const stack of framed) {
        sheetStack(...stack);
      }
    });
    for (const type of ["animationstart", "animationend", "animationcancel"]) {
      document.addEventListener(type, ({ target }) => {
        const stack = watched.has(target) && lastFramed.get(viewParent(target));
        stack && stack[0].includes(target) && sheetStack(...stack);
      });
    }
  }
  return sizeWatch;
}
// How the stack's sheets stand to each other, once each route has its frame. A sheet presented from a route in a
// sheet says whether it is the size of what it covers (`data-exact-sheet-stacked="same"`), and each route of the
// sheet under it says which the one over it is (`data-exact-under-sheet`). UIKit steps a form sheet back only under
// a sheet of its own width and height, each as it is alone, and half a point is another size (iPadOS 27); a sheet is
// as its top shown route stands. The step's geometry is each sheet's own, so every route of a sheet in a stack is
// read first, and then given how tall it is alone, what the stack took from its height and, for its step, its used
// size (neutral.css): how far a covered sheet rises and how low the one over it stands follow from how close to the
// window's margin a sheet that tall is.
// UIKit lays a form sheet's dim over all beneath it, and one of another size than the sheet it covers has the
// page's: so each presented route beneath a sheet such a sheet covers, and the parent for the page, is told how
// many of them dim it again (`--exact-sheet-stack-dims`, `-page-dims`).
function sheetStack(routes, shownIn, inUse, selected) {
  const stacked = [...inUse].filter((at) => at > 0 && shownIn[at - 1] >= 0), boxes = new Map(), same = new Map(), parent = viewParent(routes[0]);
  parent && lastFramed.set(parent, [routes, shownIn, inUse, selected]);
  stacked.length && !stepRegistered && registerStep();
  // The route after each route's sheet: a stacked sheet's first, where one was presented from it.
  const after = routes.map((_, index) => {
    let next = index + 1;
    while (shownIn[index] >= 0 && shownIn[next] === shownIn[index]) {
      next++;
    }
    return next;
  });
  for (let index = 0; stacked.length && index <= selected; index++) {
    inUse.has(shownIn[index]) && boxes.set(routes[index], sheetBox(routes[index], stacked.includes(after[index])));
  }
  for (const [route, box] of boxes) {
    if (box?.frames) {
      box.alone = sheetAlone(route, box.frames);
    }
  }
  for (const at of stacked) {
    let top = at;
    while (top < selected && shownIn[top + 1] === at) {
      top++;
    }
    const above = boxes.get(routes[top]), below = boxes.get(routes[at - 1]);
    same.set(at, above && below ? Math.abs(above.width - below.width) < .05 && Math.abs(above.alone - below.alone) < .05 : null);
  }
  // The first route of each sheet that a sheet of another size covers.
  const dimmed = stacked.filter((at) => same.get(at) === false).map((at) => shownIn[at - 1]);
  parent && sheetLength(parent, "--exact-sheet-stack-page-dims", dimmed.length ? String(dimmed.length) : "");
  for (const [index, route] of routes.entries()) {
    const at = shownIn[index], box = boxes.get(route), dims = dimmed.length && (at >= 0 || route.getAttribute("navigationPresentation") === "fullscreen") ? dimmed.filter((first) => index < first).length : 0;
    sheetLength(route, "--exact-sheet-stack-dims", dims ? String(dims) : "");
    if (same.has(at)) {
      mark(route, "data-exact-sheet-stacked", same.get(at) ? "same" : "");
    }
    const over = same.get(after[index]) ?? null;
    mark(route, "data-exact-under-sheet", over === null ? null : over ? "same" : "other");
    sheetLength(route, "--exact-sheet-stack-took", box && same.has(at) ? `${+(box.alone - box.height).toFixed(3)}px` : "");
    sheetLength(route, "--exact-sheet-stack-alone", box && (same.has(at) || over) ? `${+box.alone.toFixed(3)}px` : "");
    sheetStep(route, box);
    if (!boxes.has(route)) {
      watched.delete(route) && sizeWatch?.unobserve(route);
    } else if (!watched.has(route)) {
      watched.add(route);
      watchSizes()?.observe(route, { box: "border-box" });
    }
  }
}
// What a projection does for a stack of sheets, once each route has its frame. A stack is read in the next frame, as
// a clear bar is measured, so that the task that projects lays nothing out; the frame's first paint has what was
// read. Where no sheet is over a sheet nothing is read, and what a stack left on its routes goes at once. The
// window's size can change whose a transform is with no sheet resized (a media query's rule comes or goes), so
// while a stack exists the window's resize reads every stack again; with none, nothing listens.
function readStacks(parent) {
  pending.add(parent);
  stackFrame ||= requestAnimationFrame(() => {
    stackFrame = 0;
    sweep();
    const framed = [...pending].map((parent) => lastFramed.get(parent));
    pending.clear();
    for (const stack of framed) {
      stack?.[0][0].isConnected && sheetStack(...stack);
    }
  });
}
function resized() {
  sweep();
  for (const parent of stacks) {
    readStacks(parent);
  }
}
function unstack(parent) {
  stacks.delete(parent) && !stacks.size && removeEventListener("resize", resized);
}
export function stackSheets(routes, shownIn, inUse, selected) {
  const parent = viewParent(routes[0]);
  sweep();
  if (![...inUse].some((at) => at > 0 && shownIn[at - 1] >= 0)) {
    pending.delete(parent);
    unstack(parent);
    sheetStack(routes, shownIn, inUse, selected);
  } else if (parent) {
    lastFramed.set(parent, [routes, shownIn, inUse, selected]);
    stacks.size || addEventListener("resize", resized);
    stacks.add(parent);
    readStacks(parent);
  }
}
/** A route that left the document takes its lengths with it: routes.js `sheetLeaves`, once its slide has removed it,
 * after which no projection may come. One removed in an update goes at that update's read. */
export function sheetGone(route) {
  sheetLength(route, "", "");
}
/** A projection that finds no route to frame in a root reaches no stack: what the stacks hold for routes and roots
 * that are gone goes here. */
export const sheetsLeft = sweep;
