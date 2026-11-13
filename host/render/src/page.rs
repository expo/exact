//! A rendered document as a page (LLP 1048.000 D3, D6, D7): the one place a
//! page is composed, for the build's `dist/` and the server alike.
use crate::serve::deferred::{self, Checkpoints};
use crate::Rendered;

/// The capture script (LLP 1048.000 D6, 1048.001 D5): host code, inline in
/// every document's head, that records presses — a click, or Enter or Space
/// on an element that hears `press` and isn't a native control — against
/// their elements from first parse, until the boot glue takes them over and
/// replays them once after adoption. Once the page has painted (its first
/// paint entry), a page that doesn't activate on interaction hands its wasm's
/// download, which the head's preload began, to the glue (`exact.runtime`).
/// A navigation that leaves the document stops that download, and removes the
/// preload, which would otherwise keep it going: the next document needs the
/// link. It holds no app logic. The
/// server's CSP admits it by hash; `scripts/boot.mjs` pins that hash and its
/// size.
pub fn capture() -> &'static str {
    include_str!("../../web/capture.js").trim_end()
}

/// The built shell with the renderer's `<head>` and the capture script in
/// place of its title and viewport meta, the document in `#exact-root`, and
/// its checkpoint, with the document's digest, before the glue. An idle
/// page keeps the shell's wasm and `navigation.js` preloads, so its runtime
/// downloads with the document, where a CDN would send them as a 103 (D3, as
/// built); an interaction page's are removed: it fetches nothing before a
/// handler's intent. Refuses a shell that no longer has those places.
pub fn page(shell: &str, rendered: &Rendered) -> Result<String, String> {
    page_at(shell, rendered, None)
}

pub(crate) fn page_at(
    shell: &str,
    rendered: &Rendered,
    checkpoints: Option<&Checkpoints>,
) -> Result<String, String> {
    if is_js(shell) {
        return Ok(page_with(shell, &Js::of(shell)?, rendered, checkpoints));
    }
    let mut html = shell.to_string();
    // From `start` through the first `end` after it.
    let cut = |html: &mut String, start: &str, end: &str, with: &str| -> Result<(), String> {
        let lacks = || format!("the shell has no `{start}`…`{end}`");
        let at = html.find(start).ok_or_else(lacks)?;
        let stop = html[at..].find(end).ok_or_else(lacks)? + at + end.len();
        html.replace_range(at..stop, with);
        Ok(())
    };
    let lang = lang_attr(shell, &rendered.document.lang);
    cut(
        &mut html,
        "<html",
        ">",
        &format!(
            "<html lang=\"{lang}\" dir=\"{}\"{}>",
            rendered.document.dir,
            scroll_attr(rendered.document.scroll_document)
        ),
    )?;
    // The shell's viewport meta goes before the head, which has its own.
    cut(&mut html, "<meta name=\"viewport\"", ">\n", "")?;
    cut(
        &mut html,
        "<title>",
        "</title>\n",
        &format!("{}\n<script>{}</script>\n", rendered.head, capture()),
    )?;
    cut(&mut html, "<!-- Fetched in parallel", "-->\n", "")?;
    let interaction = rendered.activate == exact_plan::ActivatePolicy::Interaction;
    // The wasm's URL names its build (`./app.wasm?v=…`, build.mjs). An
    // interaction page drops the preload, so its checkpoint names the build
    // for document-glue.js.
    let navigation = "<link rel=\"modulepreload\" href=\"./navigation.js\">\n";
    let preload = "<link rel=\"preload\" href=\"";
    let wasm = &format!("{preload}./app.wasm");
    let named = html.find(wasm).and_then(|at| {
        let from = at + preload.len();
        Some(&html[from..from + html[from..].find('"')?])
    });
    let build = match named {
        Some(href) if interaction => format!(" data-wasm=\"{href}\""),
        _ => String::new(),
    };
    if interaction {
        cut(&mut html, navigation, navigation, "")?;
        cut(&mut html, wasm, ">\n", "")?;
    }
    let (open, close) =
        shell_root(&html).ok_or("the shell has no `<div id=\"exact-root\"></div>`")?;
    let document = format!("{}{}</div>", &html[open.clone()], rendered.document.root);
    html.replace_range(open.start..close, &document);
    let glue = "<script type=\"module\" src=\"./glue.js\"></script>";
    let entry = if interaction {
        "<script type=\"module\" src=\"./document-glue.js\"></script>"
    } else {
        glue
    };
    let (src, body) = deferred::checkpoint(&rendered.checkpoint, rendered.activate, checkpoints);
    let checkpoint = format!(
        "<script type=\"application/vnd.exact.checkpoint\" data-digest=\"{}\" data-activate=\"{}\"{build}{src}>{body}</script>\n{entry}",
        rendered.digest, rendered.activate.name()
    );
    cut(&mut html, glue, glue, &checkpoint)?;
    // Counted outside script elements ([`blanked`]) and before the shell's
    // sheet, the first after the render's head (which may hold a sheet of
    // its own): after it, `host.web.head`'s entries are the app's.
    let seen = blanked(&html);
    let head_end = seen
        .find(&rendered.head)
        .map_or(0, |at| at + rendered.head.len());
    let own = &seen[..seen[head_end..]
        .find("<style>")
        .map_or(seen.len(), |at| at + head_end)];
    let viewports = own.matches("<meta name=\"viewport\"").count();
    let preloads = ["app.wasm", "navigation.js", "glue.js"]
        .iter()
        .filter(|file| own.contains(&format!("preload\" href=\"./{file}")))
        .count();
    let expected = if interaction { 0 } else { 2 };
    if viewports != 1 || preloads != expected || !html.contains(&rendered.head) {
        return Err("the shell no longer has the places a document goes".into());
    }
    Ok(html)
}

/// The JavaScript runtime's entry in its shell (`host/web-js/build.mjs`).
const JS_ENTRY: &str = "<script type=\"module\" src=\"./app.js\"></script>";

/// The JavaScript runtime's capture script (LLP 1048.001 D5, as
/// `capture()`): presses and edits before activation, replayed after
/// adoption. It also starts the runtime, by the page's policy
/// ([`activate_js`]): an `eager` page's after its first paint, once the
/// document is parsed; an `idle` page's when the browser is idle after
/// `load`; an `interaction` page's at the first press or edit, as every
/// page's at a press that comes first.
pub fn capture_js() -> &'static str {
    include_str!("../../web-js/capture.js").trim_end()
}

/// `<html data-scrolldocument>` when an element is the page's scroller
/// (LLP 1048.003 D4): the shell's rule reads the root, not a `:has()` over
/// the whole tree at every style recalculation.
fn scroll_attr(on: bool) -> &'static str {
    if on {
        " data-scrolldocument"
    } else {
        ""
    }
}

/// The mark a streamed page sets once its render finds the page's scroller,
/// its `<html>` having gone before (the only other inline script; the CSP
/// admits its bytes).
pub fn scroll_document_js() -> &'static str {
    "document.documentElement.setAttribute(\"data-scrolldocument\",\"\")"
}

/// A route's activation on the JavaScript runtime (LLP 1071 D6): what it
/// declared, with an undeclared one `eager`, the default (preloaded from
/// the head, run after first paint).
pub(crate) fn activate_js(activate: exact_plan::ActivatePolicy) -> &'static str {
    match activate {
        exact_plan::ActivatePolicy::Inferred => "eager",
        declared => declared.name(),
    }
}

/// Whether a page at `location` names the entry's preloads in the head a
/// server sends before its render: an `interaction` route fetches nothing
/// before intent, and an undeclared one whose page may wait does not know
/// yet, so it names them after its head once its render chose to run at
/// once (`body_open_js`'s `late`, LLP 1048.007 D1).
pub(crate) fn preloads_early(plan: &exact_plan::Plan, location: &str) -> bool {
    match exact_web::document::route_at(plan, location).map(|route| route.activate) {
        Some(exact_plan::ActivatePolicy::Interaction) => false,
        None | Some(exact_plan::ActivatePolicy::Inferred) => crate::inference(plan).never_waits(),
        Some(_) => true,
    }
}

/// [`page`] over a shell already read, with the build's analyzed sheet
/// when [`Js::with_rules`] gave it.
pub(crate) fn page_with(
    shell: &str,
    js: &Js,
    rendered: &Rendered,
    checkpoints: Option<&Checkpoints>,
) -> String {
    let preload = activate_js(rendered.activate) != "interaction";
    let document = &rendered.document;
    let scroll = document.scroll_document;
    head_js(shell, js, &document.lang, &document.dir, preload, scroll)
        + &body_js(shell, js, rendered, false, scroll, checkpoints)
}

/// [`page`] with the build's analyzed sheet (`.gen/page-style.json`), so
/// a JavaScript page carries only the rules its document can match (LLP
/// 1048.007 D4).
pub fn page_with_rules(shell: &str, rules: &str, rendered: &Rendered) -> Result<String, String> {
    if !is_js(shell) {
        return page(shell, rendered);
    }
    Ok(page_with(
        shell,
        &Js::of(shell)?.with_rules(rules),
        rendered,
        None,
    ))
}

/// What a JavaScript shell's pages are composed from, read once: a server
/// keeps it for every page it sends ([`crate::Server`]).
pub(crate) struct Js {
    at: Places,
    preloads: Vec<(usize, usize)>,
    classes: Classes,
    fonts: bool,
    /// The shell's `<style>…</style>` line, between the viewport meta and
    /// the root.
    sheet: Option<(usize, usize)>,
    /// The sheet as the build analyzed it ([`Js::with_rules`]): a page then
    /// carries its own rules after its head ([`body_open_js`]), and the head
    /// sent before the render carries none.
    rules: Option<Vec<crate::page_style::Rule>>,
}

impl Js {
    /// `shell`'s places, or why it lacks them.
    pub(crate) fn of(shell: &str) -> Result<Js, String> {
        let at = places(shell)?;
        let sheet = shell[at.title.1..at.root].find("<style>").and_then(|s| {
            let start = at.title.1 + s;
            let end = shell[start..at.root].find("</style>")? + start + "</style>".len();
            Some((start, end + usize::from(shell[end..].starts_with('\n'))))
        });
        Ok(Js {
            preloads: preloads(shell, &at),
            classes: classes(shell),
            fonts: shell.contains("as=\"font\""),
            sheet,
            rules: None,
            at,
        })
    }

    /// With the build's analyzed sheet (`.gen/page-style.json`); unchanged
    /// when it does not parse or the shell has no sheet.
    pub(crate) fn with_rules(mut self, json: &str) -> Js {
        self.rules = self.sheet.and(crate::page_style::parse(json));
        self
    }
}

/// Whether `shell` is the JavaScript runtime's, whose pages a server can
/// send in two parts ([`head_js`], [`body_js`]).
pub(crate) fn is_js(shell: &str) -> bool {
    js_entry(shell).is_some()
}

/// The JavaScript runtime's entry after the shell's root, outside script
/// text ([`blanked`]): [`JS_ENTRY`], or the `<script type="module">` an
/// `--inline` shell holds the bundle in (`host/web-js/build.mjs`). A page
/// puts its checkpoint in that place, and its capture script loads
/// `./app.js`.
fn js_entry(shell: &str) -> Option<std::ops::Range<usize>> {
    const INLINE: &str = "<script type=\"module\">";
    let (_, root_end) = shell_root(shell)?;
    let seen = &blanked(shell)[root_end..];
    let (at, len) = match seen.find(JS_ENTRY) {
        Some(at) => (at, JS_ENTRY.len()),
        None => {
            let at = seen.find(INLINE)?;
            let text = seen[at + INLINE.len()..].find("</script>")?;
            if !seen[at + INLINE.len()..][..text].trim().is_empty() {
                return None;
            }
            (at, INLINE.len() + text + "</script>".len())
        }
    };
    Some(root_end + at..root_end + at + len)
}

/// The shell's places, in its order: `<html…>`, `<title>` through the
/// viewport meta's line, the root, the entry.
struct Places {
    html: (usize, usize),
    title: (usize, usize),
    /// The root's opening tag, and the end of its `</div>`.
    root: usize,
    root_open: usize,
    root_end: usize,
    entry: std::ops::Range<usize>,
}

fn places(shell: &str) -> Result<Places, String> {
    let lacks = |what: &str| format!("the JavaScript shell has no `{what}`");
    let open = shell.find("<html").ok_or_else(|| lacks("<html"))?;
    let end = shell[open..].find('>').ok_or_else(|| lacks("<html>"))? + open + 1;
    let title = shell.find("<title>").ok_or_else(|| lacks("<title>"))?;
    let meta = shell[title..]
        .find("<meta name=\"viewport\"")
        .ok_or_else(|| lacks("viewport"))?
        + title;
    let stop = shell[meta..].find(">\n").ok_or_else(|| lacks("viewport"))? + meta + 2;
    let (root, root_end) =
        shell_root(shell).ok_or_else(|| lacks("<div id=\"exact-root\"></div>"))?;
    let entry = js_entry(shell).ok_or_else(|| lacks(JS_ENTRY))?;
    if !(end <= title && stop <= root.start && root_end <= entry.start) {
        return Err("the JavaScript shell's places are out of order".into());
    }
    Ok(Places {
        html: (open, end),
        title: (title, stop),
        root: root.start,
        root_open: root.end,
        root_end,
        entry,
    })
}

/// The shell's `modulepreload` lines between the viewport meta and its
/// sheet. After the sheet, `host.web.head`'s entries are the app's.
fn preloads(shell: &str, places: &Places) -> Vec<(usize, usize)> {
    let shell = &blanked(shell);
    let mut found = Vec::new();
    let mut from = places.title.1;
    let end = shell[from..places.root]
        .find("<style>")
        .map_or(places.root, |at| at + from);
    while let Some(at) = shell[from..end].find(MODULE_PRELOAD) {
        let at = at + from;
        let Some(stop) = shell[at..end].find(">\n") else {
            break;
        };
        found.push((at, at + stop + 2));
        from = at + stop + 2;
    }
    found
}

/// A rendered page's `<html lang>` value, escaped: the language of the
/// strings table the render resolved, or, for a plan with no table (`lang`
/// is empty), the one the shell carries, which the build wrote from the
/// manifest's `lang` (LLP 1115.000 D4.6).
fn lang_attr(shell: &str, lang: &str) -> String {
    if !lang.is_empty() {
        return lang
            .replace('&', "&amp;")
            .replace('"', "&quot;")
            .replace('<', "&lt;");
    }
    // As the build wrote it, escaped already.
    let tag = shell
        .find("<html")
        .and_then(|at| shell[at..].find('>').map(|end| &shell[at..at + end]));
    tag.and_then(|tag| tag.split(" lang=\"").nth(1))
        .and_then(|value| value.split('"').next())
        .unwrap_or("")
        .to_string()
}

/// One of the shell's `modulepreload` lines (`<link …>\n`), marked
/// `fetchpriority="low"` (LLP 1153.001 D8): a rendered page paints without
/// the runtime, which only adopts it (LLP 1048.001), so the runtime must not
/// compete with the page for the link. The shell a `client` route is sent
/// keeps it at the default priority: that page cannot paint until it runs.
fn push_low(out: &mut String, line: &str) {
    let tag = line.trim_end_matches('\n').trim_end_matches('>');
    out.push_str(tag);
    out.push_str(" fetchpriority=\"low\">\n");
}

/// What of a JavaScript page comes before anything its render decides: the
/// doctype and `<html lang dir>`, the charset and base, the capture script,
/// the entry's `modulepreload`s at low priority when `preload` (an
/// `interaction` page has none: it fetches nothing before intent) and the
/// stylesheet. A server sends it as a request arrives (LLP 1071 D6's early
/// flush), while the page's data is still being asked; the head stays open,
/// so what the render decides — the title, the metas, a late preload — still
/// lands in `<head>`.
pub(crate) fn head_js(
    shell: &str,
    js: &Js,
    lang: &str,
    dir: &str,
    preload: bool,
    scroll: bool,
) -> String {
    let at = &js.at;
    let lang = lang_attr(shell, lang);
    let mut out = String::with_capacity(at.root + 1024);
    out.push_str(&shell[..at.html.0]);
    let _ = std::fmt::Write::write_fmt(
        &mut out,
        format_args!(
            "<html lang=\"{lang}\" dir=\"{dir}\"{}>",
            scroll_attr(scroll)
        ),
    );
    out.push_str(&shell[at.html.1..at.title.0]);
    out.push_str("<script>");
    out.push_str(capture_js());
    out.push_str("</script>\n");
    let mut from = at.title.1;
    for &(start, stop) in &js.preloads {
        out.push_str(&shell[from..start]);
        if preload {
            push_low(&mut out, &shell[start..stop]);
        }
        from = stop;
    }
    match js
        .sheet
        .filter(|(start, _)| js.rules.is_some() && *start >= from)
    {
        Some((start, stop)) => {
            out.push_str(&shell[from..start]);
            out.push_str(&shell[stop..at.root]);
        }
        None => out.push_str(&shell[from..at.root]),
    }
    out
}

/// The rest of a JavaScript page, once its render is done: the head's
/// fields, the entry's `modulepreload`s when `late` (a page sent before its
/// render turned an `interaction` route `eager`), the page's own sheet, the
/// document in `#exact-root`, and its checkpoint in the entry's place,
/// which the capture script finds. The document goes as the runtime adopts
/// it ([`for_runtime`]).
pub(crate) fn body_js(
    shell: &str,
    js: &Js,
    rendered: &Rendered,
    late: bool,
    marked: bool,
    checkpoints: Option<&Checkpoints>,
) -> String {
    body_after_boot_js(shell, js, rendered, late, marked, None, checkpoints)
}

pub(crate) fn body_after_boot_js(
    shell: &str,
    js: &Js,
    rendered: &Rendered,
    late: bool,
    marked: bool,
    boot: Option<&crate::direct::Boot>,
    checkpoints: Option<&Checkpoints>,
) -> String {
    let document = &rendered.document;
    let mut html = std::borrow::Cow::Borrowed(document.root.as_str());
    if let Some(at) = document
        .hero_image
        .as_ref()
        .and_then(|image| image.priority_at)
    {
        html.to_mut().insert_str(at, " fetchpriority=\"high\"");
    }
    let mut root = String::with_capacity(document.root.len());
    if rendered.runtime_form {
        root.push_str(&html);
    } else {
        for_runtime(&mut root, &html, &js.classes);
    }
    // The tokens `render.mjs` reads: the document, then the shell from its root.
    let style = js.rules.as_ref().map(|rules| {
        let have = crate::page_style::tokens(&(root.clone() + &shell[js.at.root..]));
        crate::page_style::select(rules, &have)
    });
    let mut out = boot.map_or_else(
        || {
            body_open_js(
                shell,
                js,
                &rendered.head,
                document.scroll_document,
                late,
                marked,
                style.as_deref().map_or(Sheet::Whole, Sheet::Own),
            )
        },
        |boot| boot.open(shell, js, document, late),
    );
    preload_image(&mut out, shell, document.hero_image.as_ref());
    out.reserve(root.len() + rendered.checkpoint.len() + shell.len() - js.at.root);
    out.push_str(&root);
    let close = body_close_js(
        shell,
        js,
        &rendered.digest,
        rendered.activate,
        &rendered.wake,
        &rendered.checkpoint,
        checkpoints,
    );
    out.push_str(&crate::direct::close_after_boot(
        close,
        boot.map(|boot| boot.to(&document.controls)),
    ));
    out
}

/// Insert the hero's preload before the root's opening tag.
pub(crate) fn preload_image(
    open: &mut String,
    shell: &str,
    hero: Option<&exact_web::document::HeroImage>,
) {
    if let Some(hero) = hero {
        let src = hero
            .src
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;");
        let link = format!("<link rel=\"preload\" as=\"image\" href=\"{src}\">");
        open.insert_str(open.len() - root_open(shell).len(), &link);
    }
}

/// Which sheet a page's rest carries when the build analyzed the shell's
/// (its head, sent before the render, carries none).
pub(crate) enum Sheet<'a> {
    /// The rules its document can match, which the runtime replaces with
    /// the whole sheet before its first render.
    Own(&'a str),
    /// The shell's whole sheet: a page sent before its document is written.
    Whole,
    /// None: an earlier part of the page carried it.
    Sent,
}

/// [`body_js`] through `<div id="exact-root">`: what goes before the
/// document, which a streamed page sends before its root is written
/// (LLP 1048.004 Q3), with the page's sheet when the build analyzed it
/// ([`Sheet`]).
pub(crate) fn body_open_js(
    shell: &str,
    js: &Js,
    head: &str,
    scroll: bool,
    late: bool,
    marked: bool,
    sheet: Sheet,
) -> String {
    let mut out = String::with_capacity(head.len() + 1024);
    if js.fonts {
        out.push_str(&without_fonts(head));
    } else {
        out.push_str(head);
    }
    out.push('\n');
    // A head sent before the render could not mark `<html>` (`scroll_attr`).
    if scroll && !marked {
        out.push_str("<script>");
        out.push_str(scroll_document_js());
        out.push_str("</script>\n");
    }
    if late {
        for &(start, stop) in &js.preloads {
            push_low(&mut out, &shell[start..stop]);
        }
    }
    match (sheet, js.sheet.filter(|_| js.rules.is_some())) {
        (Sheet::Own(style), _) => {
            out.push_str("<style data-exact-page>");
            out.push_str(style);
            out.push_str("</style>\n");
        }
        (Sheet::Whole, Some((start, stop))) => out.push_str(&shell[start..stop]),
        _ => {}
    }
    out.push_str(&shell[js.at.root..js.at.root_open]);
    out
}

/// [`body_js`] from the root's `</div>`: the rest of the shell, with the
/// checkpoint in the entry's place.
pub(crate) fn body_close_js(
    shell: &str,
    js: &Js,
    digest: &str,
    activate: exact_plan::ActivatePolicy,
    wake: &str,
    checkpoint: &str,
    checkpoints: Option<&Checkpoints>,
) -> String {
    let (src, checkpoint) = deferred::checkpoint(checkpoint, activate, checkpoints);
    let at = &js.at;
    let mut out = String::with_capacity(shell.len() - at.root + checkpoint.len() + 256);
    out.push_str("</div>");
    out.push_str(&shell[at.root_end..at.entry.start]);
    let wake = if wake.is_empty() {
        String::new()
    } else {
        // Escaped as `render.mjs` `esc` writes it.
        let escaped = wake
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;");
        format!(" data-wake=\"{escaped}\"")
    };
    let _ = std::fmt::Write::write_fmt(
        &mut out,
        format_args!(
            "<script type=\"application/vnd.exact.checkpoint\" data-digest=\"{}\" data-activate=\"{}\"{}{src}>{}</script>",
            digest,
            activate_js(activate),
            wake,
            checkpoint
        ),
    );
    out.push_str(&shell[at.entry.end..]);
    out
}

/// A JavaScript document's inline style as the runtime has it, for the
/// kernel-free writer ([`runtime_style`] over the shell's classes).
pub(crate) fn runtime_style_of(js: &Js) -> impl Fn(&str, &mut String) -> bool + '_ {
    move |css: &str, out: &mut String| runtime_style(&js.classes, css, out)
}

/// The head's fields without the plan's fonts, for a shell that declares
/// them (`host/web-js/build.mjs`: its stylesheet's faces, with their
/// `font-display`, and their preloads early in the head): a second rule for
/// a face would replace the shell's.
fn without_fonts(head: &str) -> String {
    const PRELOAD: &str = "<link rel=\"preload\" href=\"";
    let mut out = head.to_string();
    if let Some(at) = out.find("<style>@font-face") {
        if let Some(end) = out[at..].find("</style>") {
            out.replace_range(at..at + end + "</style>".len(), "");
        }
    }
    let mut from = 0;
    while let Some(at) = out[from..].find(PRELOAD).map(|at| at + from) {
        let Some(end) = out[at..].find('>').map(|end| at + end + 1) else {
            break;
        };
        if out[at..end].contains("as=\"font\"") {
            out.replace_range(at..end, "");
            from = at;
        } else {
            from = end;
        }
    }
    out
}

/// `html` with each script element's text blanked, byte for byte, so the
/// shell's places (its root, entry, preloads and viewport) are found only
/// in its own markup: a script's text, an `--inline` bundle's or a
/// `host.web.head` entry's, may spell any of them. A script element starts
/// where HTML's tokenizer starts one (HTML, 13.2.5 "Tokenization"): a
/// `<script` start tag in markup, not inside a comment, a quoted attribute
/// value, or the text of an element HTML reads as raw text or RCDATA
/// (`style`, `title`, `textarea`, …), which may hold that spelling. Its text
/// runs from the start tag's `>` to the first `</script` followed by
/// whitespace, `/` or `>` ("Script data state" and the end tag states after
/// it): an entry's text holds no `<!--` (`scripts/web-head.mjs`), so the
/// escaped states that could pass over that end tag are never entered. As
/// `render.mjs` `blanked`.
fn blanked(html: &str) -> String {
    const RAW_TEXT: [&str; 8] = [
        "style", "title", "textarea", "xmp", "iframe", "noembed", "noframes", "noscript",
    ];
    let lower = html.to_ascii_lowercase();
    let b = lower.as_bytes();
    let ends_name = |at: usize| {
        matches!(
            b.get(at),
            Some(b'\t' | b'\n' | 0x0c | b'\r' | b' ' | b'/' | b'>')
        )
    };
    let mut out = html.as_bytes().to_vec();
    let mut at = 0;
    while let Some(open) = lower[at..].find('<').map(|i| i + at) {
        if lower[open..].starts_with("<!--") {
            at = lower[open + 4..]
                .find("-->")
                .map_or(b.len(), |i| open + 4 + i + 3);
            continue;
        }
        let name_end = (open + 1..b.len())
            .find(|&i| ends_name(i))
            .unwrap_or(b.len());
        let name = &lower[open + 1..name_end];
        if !name.as_bytes().first().is_some_and(u8::is_ascii_lowercase) {
            at = open + 1;
            continue;
        }
        // The start tag's `>`, past its quoted attribute values.
        let mut end = name_end;
        let mut quote = None;
        while end < b.len() {
            match (quote, b[end]) {
                (None, b'>') => break,
                (None, q @ (b'"' | b'\'')) => quote = Some(q),
                (Some(q), c) if c == q => quote = None,
                _ => {}
            }
            end += 1;
        }
        at = (end + 1).min(b.len());
        if name != "script" && !RAW_TEXT.contains(&name) {
            continue;
        }
        // Its text, to the first end tag of its name followed by
        // whitespace, `/` or `>`.
        let tag = format!("</{name}");
        let mut from = at;
        let close = loop {
            match lower[from..].find(&tag).map(|i| i + from) {
                Some(close) if ends_name(close + tag.len()) => break Some(close),
                Some(close) => from = close + tag.len(),
                None => break None,
            }
        };
        let Some(close) = close else { break };
        if name == "script" {
            out[at..close].fill(b' ');
        }
        at = close;
    }
    // Whole characters were blanked: a script's text starts after an ASCII
    // `>` and ends before an ASCII `<`.
    String::from_utf8(out).expect("blanking keeps UTF-8")
}

/// The document's element in the shell, `<div id="exact-root"></div>`,
/// with what the build writes on it (`data-audio-session`, LLP 1096 D7):
/// its opening tag's range and where its `</div>` ends. The one outside
/// every script element ([`blanked`]).
fn shell_root(shell: &str) -> Option<(std::ops::Range<usize>, usize)> {
    const OPEN: &str = "<div id=\"exact-root\"";
    let shell = &blanked(shell);
    let at = shell.find(OPEN)?;
    let rest = &shell[at + OPEN.len()..];
    let attrs = match rest.strip_prefix(" data-audio-session=\"") {
        Some(value) => {
            let quote = value.find('"')?;
            if !value[..quote]
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b == b'-')
            {
                return None;
            }
            " data-audio-session=\"".len() + quote + 1
        }
        None => 0,
    };
    let open = at + OPEN.len() + attrs;
    shell[open..]
        .starts_with("></div>")
        .then(|| (at..open + 1, open + "></div>".len()))
}

/// The shell's root opening tag, attributes and all (`direct.rs` marks a
/// boot document's copy).
pub(crate) fn root_open(shell: &str) -> &str {
    shell_root(shell).map_or("<div id=\"exact-root\">", |(open, _)| &shell[open])
}

/// The shell stylesheet's static classes (`host/web-js/build.mjs`: `.c<n>{…}`
/// inside `#exact-root#exact-root{…}`): by their CSS text, and, for a class
/// no other rule of the shell names (no `:hover`, no media variant), its
/// declarations, which an element's inline style may contain with more.
pub(crate) struct Classes {
    exact: std::collections::HashMap<String, String>,
    plain: Vec<(String, Vec<String>)>,
}

fn classes(shell: &str) -> Classes {
    let mut found = Classes {
        exact: std::collections::HashMap::new(),
        plain: Vec::new(),
    };
    let Some(start) = shell.find("#exact-root#exact-root{") else {
        return found;
    };
    let mut rest = &shell[start + "#exact-root#exact-root{".len()..];
    while let Some(tail) = rest.strip_prefix(".c") {
        let Some(open) = tail.find('{') else { break };
        let Some(close) = tail[open..].find('}') else {
            break;
        };
        let (name, css) = (&tail[..open], &tail[open + 1..open + close]);
        if !name.bytes().all(|b| b.is_ascii_digit()) {
            break;
        }
        // `c<n>`, from the `.c<n>{` at the head of `rest`.
        let class = &rest[1..2 + open];
        found
            .exact
            .entry(css.to_string())
            .or_insert_with(|| class.to_string());
        let named = shell
            .match_indices(&rest[..2 + open])
            .filter(|(at, dot)| {
                !shell.as_bytes()[at + dot.len()..]
                    .first()
                    .is_some_and(u8::is_ascii_digit)
            })
            .count();
        if named == 1 {
            found.plain.push((
                class.to_string(),
                declarations(css).map(str::to_string).collect(),
            ));
        }
        rest = &tail[open + close + 1..];
    }
    // The class with the most declarations first: the fewest left inline.
    found.plain.sort_by_key(|(_, d)| std::cmp::Reverse(d.len()));
    found
}

/// A style's declarations, as the document and the stylesheet write them
/// (`name:value;` each).
fn declarations(css: &str) -> impl Iterator<Item = &str> {
    css.split(';').filter(|d| !d.is_empty())
}

/// An inline style as the class that carries most of it and the rest
/// (a dynamic style row's element: its static class and its live rows), or
/// `None` when no plain class's declarations are all in it.
fn split_style<'a>(classes: &'a Classes, css: &str) -> Option<(&'a str, String)> {
    let inline: Vec<&str> = declarations(css).collect();
    let (class, of) = classes.plain.iter().find(|(_, d)| {
        !d.is_empty() && d.len() < inline.len() && d.iter().all(|x| inline.contains(&x.as_str()))
    })?;
    let mut rest = String::new();
    for d in inline.iter().filter(|x| !of.iter().any(|o| o == *x)) {
        rest.push_str(d);
        rest.push(';');
    }
    Some((class, rest))
}

/// The render host's document as the JavaScript runtime adopts it: an
/// element whose inline style is one of the stylesheet's static classes
/// carries that class instead (the runtime gives it that class at
/// adoption, and drops the inline style), and no element carries the view
/// ids only the wasm runtime reads — a link keeps an empty `data-view`, as
/// the runtime's own links have, which the shell's link rules select. Adoption walks tags, so neither changes
/// what it claims; the page paints the same before and after (the class's
/// CSS is the style's, at a specificity above every rule of the shell's
/// but `!important` ones, as an inline style's is). On RealWorld's `/` the
/// two took about 1.4 KB off the page (brotli, as sent); it paints
/// pixel-for-pixel as before, with and without JavaScript.
fn for_runtime(out: &mut String, root: &str, classes: &Classes) {
    out.reserve(root.len());
    let mut rest = root;
    let mut attrs: Vec<(&str, Option<&str>)> = Vec::new();
    while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt]);
        rest = &rest[lt..];
        // A comment ends at `-->`; any other tag at its first `>` (the
        // document escapes `>` and `"` in attribute values).
        let end = if rest.starts_with("<!--") {
            rest.find("-->").map(|at| at + 3)
        } else {
            rest.find('>').map(|at| at + 1)
        };
        let Some(end) = end else { break };
        let tag = &rest[..end];
        rest = &rest[end..];
        if tag.starts_with("</") || tag.starts_with("<!") {
            out.push_str(tag);
            continue;
        }
        start_tag(out, tag, classes, &mut attrs);
    }
    out.push_str(rest);
}

/// One start tag, as [`for_runtime`] writes it; `attrs` is scratch.
fn start_tag<'t>(
    out: &mut String,
    tag: &'t str,
    classes: &Classes,
    attrs: &mut Vec<(&'t str, Option<&'t str>)>,
) {
    let space = |b: u8| matches!(b, b' ' | b'\t' | b'\n');
    let body = tag.trim_start_matches('<').trim_end_matches('>');
    let (body, close) = match body.strip_suffix('/') {
        Some(body) => (body, "/"),
        None => (body, ""),
    };
    let name_end = body.bytes().position(space).unwrap_or(body.len());
    attrs.clear();
    let mut rest = &body[name_end..];
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        let stop = rest
            .bytes()
            .position(|b| b == b'=' || space(b))
            .unwrap_or(rest.len());
        let name = &rest[..stop];
        rest = &rest[stop..];
        if let Some(value) = rest.strip_prefix("=\"") {
            let Some(q) = value.find('"') else {
                return out.push_str(tag);
            };
            attrs.push((name, Some(&value[..q])));
            rest = &value[q + 1..];
        } else if rest.starts_with('=') {
            // An unquoted value: not what the document writes.
            return out.push_str(tag);
        } else {
            attrs.push((name, None));
        }
    }
    let has_class = attrs.iter().any(|(name, _)| *name == "class");
    out.push('<');
    out.push_str(&body[..name_end]);
    for &(name, value) in attrs.iter() {
        match (name, value) {
            // The shell styles a link by `a[data-view]` (the runtime's
            // links carry an empty one); no other element needs its id.
            ("data-view", _) if &body[..name_end] == "a" => {
                out.push_str(" data-view");
                continue;
            }
            ("data-view", _) => continue,
            ("style", Some(css)) if !has_class && runtime_style(classes, &unescape(css), out) => {
                continue;
            }
            _ => {}
        }
        out.push(' ');
        out.push_str(name);
        if let Some(value) = value {
            out.push_str("=\"");
            out.push_str(value);
            out.push('"');
        }
    }
    out.push_str(close);
    out.push('>');
}

/// An element's inline style (`css`, its text) as the runtime has it
/// (`for_runtime`): its static class when the style is one, else the
/// class carrying most of it and only the live rows inline (a dynamic style
/// row's element, as the runtime writes it after adoption), written to
/// `out` as the attributes; `false`, writing nothing, when no class
/// carries it. The kernel-free writer shares it (LLP 1048.004).
pub(crate) fn runtime_style(classes: &Classes, css: &str, out: &mut String) -> bool {
    if let Some(class) = classes.exact.get(css) {
        out.push_str(" class=\"");
        out.push_str(class);
        out.push('"');
        return true;
    }
    match split_style(classes, css) {
        Some((class, rest)) => {
            out.push_str(" class=\"");
            out.push_str(class);
            out.push_str("\" style=\"");
            out.push_str(&escape_attr(&rest));
            out.push('"');
            true
        }
        None => false,
    }
}

/// An attribute value as the document writes it (`unescape`'s inverse).
fn escape_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\r', "&#13;")
}

/// An attribute value's text: the references the document writes, read.
fn unescape(value: &str) -> std::borrow::Cow<'_, str> {
    if !value.contains('&') {
        return value.into();
    }
    value
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&#13;", "\r")
        .replace("&amp;", "&")
        .into()
}

/// One script element among `host.web.head`'s entries (LLP 1048.000 D3):
/// its `src` and `type` attributes, read, and its text as the shell holds it.
pub(crate) struct HeadScript {
    pub src: Option<String>,
    pub kind: Option<String>,
    pub text: String,
}

/// The shell's `host.web.head` scripts: those between the end of its sheet,
/// after which the entries are the app's, and its root. Each start tag is as
/// `scripts/web-head.mjs` writes it: a bare attribute, or a value in double
/// quotes with `"`, `<`, `>` and `&` escaped.
pub(crate) fn head_scripts(shell: &str) -> Vec<HeadScript> {
    let seen = blanked(shell);
    let Some(from) = seen
        .find("<style>")
        .and_then(|at| seen[at..].find("</style>").map(|end| at + end))
    else {
        return Vec::new();
    };
    let to = seen[from..]
        .find("<div id=\"exact-root\"")
        .map_or(seen.len(), |at| at + from);
    let mut scripts = Vec::new();
    let mut at = from;
    while let Some(open) = seen[at..to].find("<script").map(|found| found + at) {
        let Some(close) = tag_end(&seen[open..to]).map(|found| found + open) else {
            break;
        };
        let Some(end) = seen[close..to].find("</script").map(|found| found + close) else {
            break;
        };
        let (mut src, mut kind) = (None, None);
        let mut rest = &shell[open + "<script".len()..close];
        while let Some(start) = rest.find(|c: char| !c.is_ascii_whitespace()) {
            rest = &rest[start..];
            let stop = rest
                .find(|c: char| c == '=' || c.is_ascii_whitespace())
                .unwrap_or(rest.len());
            let name = rest[..stop].to_ascii_lowercase();
            rest = &rest[stop..];
            let value = rest.strip_prefix("=\"").and_then(|value| {
                let q = value.find('"')?;
                rest = &value[q + 1..];
                Some(unescape(&value[..q]).into_owned())
            });
            match name.as_str() {
                "src" => src = value,
                "type" => kind = value,
                _ => {}
            }
        }
        scripts.push(HeadScript {
            src,
            kind,
            text: shell[close + 1..end].to_string(),
        });
        at = end;
    }
    scripts
}

/// Where a start tag ends: its first `>` outside a quoted attribute value.
fn tag_end(tag: &str) -> Option<usize> {
    let mut quote = None;
    tag.bytes().position(|b| match quote {
        Some(q) => {
            if b == q {
                quote = None;
            }
            false
        }
        None if b == b'"' || b == b'\'' => {
            quote = Some(b);
            false
        }
        None => b == b'>',
    })
}

/// A `modulepreload` in the JavaScript shell's head (`host/web-js/build.mjs`).
const MODULE_PRELOAD: &str = "<link rel=\"modulepreload\" href=\"";

#[cfg(test)]
mod tests {
    use super::*;

    /// A streamed page's head went before its render found the page's
    /// scroller: the rest marks `<html>` with the one script the CSP admits.
    #[test]
    fn a_streamed_page_marks_its_scroller_after_the_head() {
        let shell = "<!doctype html>\n<html lang=\"en\">\n<meta charset=\"utf-8\">\n<base href=\"/\">\n<title>Blog</title>\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<link rel=\"modulepreload\" href=\"./app.js\">\n<style>p{margin:0}</style>\n<div id=\"exact-root\"></div>\n<script type=\"module\" src=\"./app.js\"></script>\n";
        let mut rendered = Rendered {
            document: exact_web::document::Document {
                lang: "en".into(),
                dir: "ltr".into(),
                root: "<div></div>".into(),
                viewport_fit: None,
                interactive_widget: None,
                head: Default::default(),
                route_title: None,
                keyframes: String::new(),
                scroll_document: true,
                hero_image: None,
                controls: Vec::new(),
            },
            head: String::new(),
            checkpoint: "{}".into(),
            digest: "0".into(),
            state: Default::default(),
            settled: crate::Settled::Complete,
            activate: exact_plan::ActivatePolicy::Inferred,
            wake: String::new(),
            runtime_form: false,
        };
        let js = Js::of(shell).unwrap();
        let head = head_js(shell, &js, "en", "ltr", true, false);
        assert!(head.contains("<html lang=\"en\" dir=\"ltr\">"), "{head}");
        let mark = format!("<script>{}</script>", scroll_document_js());
        let rest = body_js(shell, &js, &rendered, false, false, None);
        assert!(rest.find(&mark).unwrap() < rest.find("<div id=\"exact-root\">").unwrap());
        assert!(!body_js(shell, &js, &rendered, false, true, None).contains(&mark));
        rendered.document.scroll_document = false;
        assert!(!body_js(shell, &js, &rendered, false, false, None).contains(&mark));
    }

    /// LLP 1048.000 D3: the app's `host.web.head`, which the build writes
    /// after the shell's sheet, is in every page's head once, whether the
    /// page carries the whole sheet or only its own rules, and whatever
    /// its entries' text says: a script's text that spells the root, the
    /// entry or a preload line is not taken for the shell's own.
    #[test]
    fn a_page_keeps_the_apps_head_entries_after_the_sheet() {
        let entries = concat!(
            "<script async src=\"https://a.example/t.js\"></script>\n<script>x('<style>p{}</style>')</script>\n",
            "<script>const r = '<div id=\"exact-root\"></div>', p = `<link rel=\"modulepreload\" href=\"./x.js\">\n`;</script>\n",
        );
        let shell = format!("<!doctype html>\n<html lang=\"en\">\n<meta charset=\"utf-8\">\n<base href=\"/\">\n<title>Blog</title>\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<link rel=\"modulepreload\" href=\"./app.js\">\n<style>p{{margin:0}}</style>\n{entries}<div id=\"exact-root\"></div>\n<script type=\"module\" src=\"./app.js\"></script>\n");
        for rules in [None, Some("[]")] {
            let js = Js::of(&shell).unwrap();
            let js = match rules {
                Some(rules) => js.with_rules(rules),
                None => js,
            };
            for preload in [true, false] {
                let head = head_js(&shell, &js, "en", "ltr", preload, false);
                assert_eq!(head.matches(entries).count(), 1, "{head}");
                assert_eq!(head.contains("p{margin:0}"), rules.is_none(), "{head}");
            }
        }
    }

    /// The shell's root, entry and preloads are the ones outside every
    /// script element: a script before the root (a `host.web.head` entry),
    /// one after it, an `--inline` shell's bundle in the entry's place, or
    /// all of them may spell the root, whole or only its opening tag, and
    /// the page's document and checkpoint still go in the shell's own.
    #[test]
    fn the_shells_places_are_outside_its_scripts() {
        let head = "<script>const r = '<div id=\"exact-root\"></div>';</script>\n";
        let after = "<script>const a = '<div id=\"exact-root\"';</script>\n";
        let external = "<script type=\"module\" src=\"./app.js\"></script>\n";
        let inline = "<script type=\"module\">const b = '<div id=\"exact-root\"></div>', c = '<div id=\"exact-root\"';</script>\n";
        for (head, after, entry) in [
            (head, "", external),
            ("", after, external),
            (head, after, external),
            ("", "", inline),
            (head, after, inline),
        ] {
            let shell = format!("<!doctype html>\n<html lang=\"en\">\n<meta charset=\"utf-8\">\n<base href=\"/\">\n<title>Blog</title>\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<link rel=\"modulepreload\" href=\"./app.js\">\n<style>p{{margin:0}}</style>\n{head}<div id=\"exact-root\"></div>\n{after}{entry}");
            assert!(is_js(&shell), "{shell}");
            let js = Js::of(&shell).unwrap();
            let rendered = Rendered {
                document: exact_web::document::Document {
                    lang: "en".into(),
                    dir: "ltr".into(),
                    root: "<p>PAGE</p>".into(),
                    viewport_fit: None,
                    interactive_widget: None,
                    head: Default::default(),
                    route_title: None,
                    keyframes: String::new(),
                    scroll_document: false,
                    hero_image: None,
                    controls: Vec::new(),
                },
                head: String::new(),
                checkpoint: "{}".into(),
                digest: "0".into(),
                state: Default::default(),
                settled: crate::Settled::Complete,
                activate: exact_plan::ActivatePolicy::Inferred,
                wake: String::new(),
                runtime_form: false,
            };
            let page = head_js(&shell, &js, "en", "ltr", false, false)
                + &body_js(&shell, &js, &rendered, false, false, None);
            for script in [head, after].into_iter().filter(|s| !s.is_empty()) {
                assert_eq!(page.matches(script).count(), 1, "{page}");
            }
            // The entry's place holds the checkpoint; the capture script
            // loads `./app.js`.
            assert!(!page.contains(entry), "{page}");
            let root = page
                .find("<div id=\"exact-root\"><p>PAGE</p></div>")
                .expect(&page);
            let checkpoint = "<script type=\"application/vnd.exact.checkpoint\"";
            assert_eq!(page.matches(checkpoint).count(), 1, "{page}");
            assert!(root < page.find(checkpoint).unwrap(), "{page}");
            assert!(head.is_empty() || page.find(head).unwrap() < root, "{page}");
        }
    }

    /// What only spells a script tag is not one: a title, a sheet, a
    /// comment or an attribute value holding `<script` (an app named
    /// "Learn <script> tags", a head entry's `meta`) leaves the shell's
    /// root and entry where they are, and the shell a JavaScript one.
    #[test]
    fn a_script_tag_in_a_title_sheet_comment_or_attribute_is_not_one() {
        for title in ["Learn <script> tags", "Learn &lt;script> tags"] {
            let shell = format!("<!doctype html>\n<html lang=\"en\">\n<meta charset=\"utf-8\">\n<base href=\"/\">\n<title>{title}</title>\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<!-- <script> -->\n<link rel=\"modulepreload\" href=\"./app.js\">\n<style>p::after{{content:\"<script>\"}}</style>\n<meta name=\"x\" content=\"<script>\" data-y='<script>'>\n<div id=\"exact-root\"></div>\n<script type=\"module\" src=\"./app.js\"></script>\n");
            assert!(is_js(&shell), "{shell}");
            let js = Js::of(&shell).unwrap();
            assert_eq!(
                &shell[js.at.root..js.at.root_end],
                "<div id=\"exact-root\"></div>"
            );
            assert_eq!(
                &shell[js.at.entry.clone()],
                "<script type=\"module\" src=\"./app.js\"></script>"
            );
            assert_eq!(preloads(&shell, &js.at).len(), 1, "{shell}");
        }
    }

    /// LLP 1115.000 D4.6: a rendered page keeps the manifest's `lang`, which
    /// the build wrote on the shell, when its plan has no strings table to
    /// name a language (`lang` empty); a resolved table's language still wins.
    #[test]
    fn a_rendered_page_keeps_the_shells_lang_when_the_plan_names_none() {
        let shell = "<!doctype html>\n<html lang=\"pt-BR\">\n<meta charset=\"utf-8\">\n<base href=\"/\">\n<title>Olá</title>\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<style></style>\n<div id=\"exact-root\"></div>\n<script type=\"module\" src=\"./app.js\"></script>\n";
        let js = Js::of(shell).unwrap();
        let head = head_js(shell, &js, "", "ltr", true, false);
        assert!(head.contains("<html lang=\"pt-BR\" dir=\"ltr\">"), "{head}");
        let head = head_js(shell, &js, "fr", "ltr", true, false);
        assert!(head.contains("<html lang=\"fr\" dir=\"ltr\">"), "{head}");
        assert_eq!(lang_attr("<html lang=\"pt-BR\">", ""), "pt-BR");
        assert_eq!(lang_attr("<html>", ""), "");
        assert_eq!(lang_attr("<html lang=\"en\">", "a\"b"), "a&quot;b");
    }
}
