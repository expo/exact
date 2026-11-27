// @ref LLP 1123 D7 — a popover route as the web's own popover: the route its presentation shows is the browser's
// popover, beside the element its `navigationPopoverAnchor` names (`showPopover({ source })`, placed by CSS anchor
// positioning), nonmodal, and closed by the browser's light dismiss and Escape, which go Back through the stack's way
// back. Where the browser cannot (no Popover API or anchor positioning) or the anchor is not there to stand beside (gone,
// hidden, or inside the presentation), and while another presentation covers it, the route is the web's sheet, as
// `routes.js` draws every presentation it does not present here.

// The popover each navigation root shows: its route's element and the presentation it is
const shown = new WeakMap();
// Routes a close left shown, which a Back did not take away: the sheet from then on, never opened again
const declined = new WeakSet();
const supported = () => typeof HTMLElement === "function" && typeof HTMLElement.prototype.showPopover === "function" && globalThis.CSS?.supports?.("position-area: bottom") === true;

/** Show the presentation at `presented` in the stack `nav` shows as a popover where it is a popover route that can be
 * one, its content the route at `selected` (a route pushed inside it among them).
 * `back(route)` takes the presentation away as Escape does a sheet, `project()` projects the root again and `find(id)` is
 * the live element an `id` names. Returns
 * whether the presentation is a popover now, so the projection leaves the page under it live. */
export function routePopover(nav, routes, presented, selected, { back, project, find }) {
  const base = routes[presented], content = routes[selected], was = shown.get(nav);
  // A close's Back is still to come: the route stays as the close left it until it does
  if (was?.closing && was.content === content) {
    return true;
  }
  const anchor = content && !declined.has(content) && base?.getAttribute("navigationPresentation") === "popover" && supported()
    ? standing(find(base.getAttribute("data-navigationpopoveranchor") ?? ""), routes.slice(presented, selected + 1)) : null;
  if (was && (was.content !== content || was.anchor !== anchor)) {
    end(nav, was);
  }
  if (!anchor) {
    return false;
  }
  return shown.get(nav)?.content === content || open(nav, base, content, anchor, back, project);
}

/** `el`, where a popover can stand beside it: in the document, drawn, and not inside the presentation's own routes. */
const standing = (el, own) => el?.isConnected && el.getClientRects().length && !own.some((route) => route.contains(el)) ? el : null;

function open(nav, base, content, anchor, back, project) {
  const record = { content, anchor, closing: false, ended: false };
  const [area, ...fallbacks] = sides(base.getAttribute("data-navigationpopoverarrows"));
  content.setAttribute("data-exact-route-popover", "");
  content.style.setProperty("--exact-popover-area", area);
  content.style.setProperty("--exact-popover-fallbacks", fallbacks.join(", ") || "none");
  // `closedby="none"`: only the app takes it away, so the browser's light dismiss and Escape do not
  content.popover = base.getAttribute("closedby") === "none" ? "manual" : "auto";
  record.toggled = (e) => {
    if (e.newState !== "closed" || record.ended) {
      return;
    }
    // The browser closed it (a press outside, Escape): Back once, in a task after the press that closed it has run,
    // and only where that press left this presentation shown; a Back that leaves it shown makes it the sheet
    record.closing = true;
    setTimeout(() => {
      if (shown.get(nav) === record && content.isConnected) {
        back(base);
      }
      setTimeout(() => {
        record.closing = false;
        if (shown.get(nav) === record) {
          declined.add(content);
          end(nav, record);
          project();
        }
      });
    });
  };
  content.addEventListener("toggle", record.toggled);
  try {
    content.showPopover({ source: anchor });
  } catch {
    clear(content, record);
    return false;
  }
  shown.set(nav, record);
  return true;
}

/** The presentation is no popover now: the host's own close, which asks no Back. */
function end(nav, record) {
  record.ended = true;
  if (shown.get(nav) === record) {
    shown.delete(nav);
  }
  if (record.content.matches(":popover-open")) {
    record.content.hidePopover();
  }
  clear(record.content, record);
}

function clear(content, record) {
  content.removeEventListener("toggle", record.toggled);
  content.removeAttribute("popover");
  content.removeAttribute("data-exact-route-popover");
  content.style.removeProperty("--exact-popover-area");
  content.style.removeProperty("--exact-popover-fallbacks");
}

// The side of its anchor a popover stands on, opposite each direction its arrow may point (`navigationPopoverArrows`):
// the first its `position-area`, the rest, in order, its fallbacks; any direction, below first, as UIKit and AppKit
// place one
const SIDE = { up: "bottom", down: "top", left: "right", right: "left" };
function sides(arrows) {
  const words = (arrows ?? "").split(/\s+/).filter((w) => w in SIDE);
  return [...new Set(words.length && !/\bany\b/.test(arrows) ? words.map((w) => SIDE[w]) : ["bottom", "top", "right", "left"])];
}
