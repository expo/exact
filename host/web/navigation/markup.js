import { navigableURL } from "./urls.js";
// LLP 1045 D4: a list marker ends at the paragraph indent, like an outside HTML list marker.
export function renderMarkup(el, json, adopting = false) {
  let pieces;
  try {
    pieces = JSON.parse(json);
  } catch {
    pieces = [];
  }
  // Atom pills need activation even when an empty label left no server placeholder.
  if (adopting && !pieces.some(piece => piece[6])) return false;
  el.replaceChildren();
  let box = el, start = true;
  const paragraph = (indent) => {
    if (start) {
      start = false;
      box = el;
      if (indent > 0) {
        box = document.createElement("span");
        box.style.display = "block";
        box.style.paddingLeft = `${indent}px`;
        el.appendChild(box);
      }
    }
    return box;
  };
  for (const [text, scale, weight, flags, href, indent = 0, atom] of pieces) {
    if (atom && globalThis.exact?.atoms?.readerAtom) {
      paragraph(indent).appendChild(globalThis.exact.atoms.readerAtom(el, text, atom, scale, weight, flags));
      continue;
    }
    // an inline atom's pill (LLP 1045.000 D6, atoms.js); until atoms.js is here, its label
    const destination = flags & 8 && href ? navigableURL(href) : null;
    const make = () => {
      const span = document.createElement(destination ? "a" : "span");
      if (scale !== 1) {
        span.style.fontSize = `${scale}em`;
      }
      if (weight) {
        span.style.fontWeight = weight;
      }
      if (flags & 1) {
        span.style.fontStyle = "italic";
      }
      if (flags & 2) {
        span.style.fontFamily = "ui-monospace, monospace";
      }
      if (flags & 4) {
        span.style.textDecoration = "line-through";
      }
      if (flags & 16) {
        span.style.opacity = "0.62";
      }
      if (destination) {
        span.href = href.trim();
      }
      if (destination && /^(https?:)?\/\//i.test(href.trim())) {
        span.target = "_blank";
        span.rel = "external noopener";
      }
      // it leaves the app, as natively (element.rs `leaves_app`)
      return paragraph(indent).appendChild(span);
    };
    if (flags & 32) {
      const marker = make();
      marker.style.cssText += "display:inline-flex;justify-content:flex-end;width:40px;white-space:pre;text-indent:0";
      marker.textContent = text;
      box.style.textIndent = "-40px";
      continue;
    }
    // Newlines are `<br>`s: the node's own white-space row still applies to the rest.
    let span = null;
    text.split("\n").forEach((line, i) => {
      if (i) {
        (span ??= make()).appendChild(document.createElement("br"));
        start = true;
        span = null;
      }
      if (line) {
        (span ??= make()).appendChild(document.createTextNode(line));
      }
    });
  }
  return true;
}
