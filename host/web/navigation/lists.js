export const listRow = (el) => [...el.children].find((c) => !c.hasAttribute("data-exiting"));
export function viewChildren(el) {
  return [...el?.children ?? []].flatMap((c) => c.hasAttribute("data-exact-list-item") ? [listRow(c)].filter(Boolean) : [c]);
}
// Semantic list items keep the row's own element and contribute no layout box.
export function viewParent(el) {
  const p = el?.parentElement;
  return p?.hasAttribute("data-exact-list-item") ? p.parentElement : p;
}
export const listOwner = (p) => p?.$listOwner ?? (p?.hasAttribute?.("data-exact-list") ? p : null);
const concreteRole = /^(?:alert|alertdialog|application|article|banner|blockquote|button|caption|cell|checkbox|code|columnheader|combobox|comment|complementary|contentinfo|definition|deletion|dialog|directory|doc-abstract|doc-acknowledgments|doc-afterword|doc-appendix|doc-backlink|doc-biblioentry|doc-bibliography|doc-biblioref|doc-chapter|doc-colophon|doc-conclusion|doc-cover|doc-credit|doc-credits|doc-dedication|doc-endnote|doc-endnotes|doc-epigraph|doc-epilogue|doc-errata|doc-example|doc-footnote|doc-foreword|doc-glossary|doc-glossref|doc-index|doc-introduction|doc-noteref|doc-notice|doc-pagebreak|doc-pagefooter|doc-pageheader|doc-pagelist|doc-part|doc-preface|doc-prologue|doc-pullquote|doc-qna|doc-subtitle|doc-tip|doc-toc|document|emphasis|feed|figure|form|generic|graphics-document|graphics-object|graphics-symbol|grid|gridcell|group|heading|image|img|insertion|link|list|listbox|listitem|log|main|mark|marquee|math|menu|menubar|menuitem|menuitemcheckbox|menuitemradio|meter|navigation|none|note|option|paragraph|presentation|progressbar|radio|radiogroup|region|row|rowgroup|rowheader|scrollbar|search|searchbox|sectionfooter|sectionheader|separator|slider|spinbutton|status|strong|subscript|suggestion|superscript|switch|tab|table|tablist|tabpanel|term|textbox|time|timer|toolbar|tooltip|tree|treegrid|treeitem)$/i, listRole = (el) => {
  const r = el.getAttribute("role");
  return r === "list" || r === "listitem" || r === "directory" ? r : r?.split(/[\t\n\f\r ]+/).find((r) => concreteRole.test(r))?.toLowerCase();
};
// Concrete roles from kernel/tables/schema.json; aria-roles.test.mjs holds them equal.
export function listItem(el, owner) {
  const p = el.parentElement;
  owner ??= listOwner(p?.hasAttribute("data-exact-list-item") ? p.parentNode : p);
  const marker = el.localName === "dialog" || el.hasAttribute("popover") ? "layer" : "", ownerRole = owner && listRole(owner), role = (!owner || ownerRole === "list" || ownerRole === "directory") && listRole(el) !== "listitem" ? "listitem" : "presentation";
  if (p?.hasAttribute("data-exact-list-item")) {
    if (p.getAttribute("data-exact-list-item") !== marker) {
      p.setAttribute("data-exact-list-item", marker);
    }
    if (p.getAttribute("role") !== role) {
      p.setAttribute("role", role);
    }
    return p;
  }
  const w = document.createElement("div");
  w.setAttribute("role", role);
  w.setAttribute("data-exact-list-item", marker);
  w.style.setProperty("display", "contents");
  w.append(el);
  return w;
}
// The marker keeps top-layer rows out of the modal card’s body count.
export function syncListItemRoles(el) {
  if (el.parentElement?.hasAttribute("data-exact-list-item")) {
    listItem(el);
  }
  if (el.hasAttribute("data-exact-list")) {
    for (const w of el.children) {
      if (w.hasAttribute("data-exact-list-item")) {
        listItem(listRow(w), el);
      }
    }
  }
}
export function unwrapListItem(el) {
  if (!el?.hasAttribute?.("data-exact-list-item")) {
    return el;
  }
  const child = listRow(el);
  el.before(...el.childNodes);
  el.remove();
  return child;
}
