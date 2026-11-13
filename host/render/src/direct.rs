//! A JavaScript page written from its runner's instance tree, with no
//! kernel, and sent as it is written (LLP 1048.004 D4, Q3).
//!
//! The server's navigation path flushes a page's head before its render
//! (LLP 1071 D6). Here the rest follows in the order the page holds it: the
//! head's fields and what else goes before the document — known from the
//! settled tree before any element is written — then the root, handed on
//! every [`exact_web::document::STREAM_CHUNK`] as the walk writes it in the
//! runtime's form, then the checkpoint. The bytes are the page
//! [`crate::page::body_js`] composes from a kernel render, but for the
//! digest, which is over the runtime's form of the root (the JavaScript
//! runtime never reads it).

use crate::page::{body_close_js, body_open_js, preload_image, runtime_style_of, Js, Sheet};
use crate::{activation, direct_for, encoded, retire, settle_at, Ids, Projection, Rendered};
use exact_plan::{ActivatePolicy, PaintPolicy, Plan};
use exact_runner::DataSource;
use exact_web::document::{
    before_root, digest, project_tree, read_checkpoint, route_at, Document, Site, Writing,
};
use std::time::Duration;

pub(crate) struct Streamed {
    pub rendered: Rendered,
    /// The settled page alone, as a cache keeps it (LLP 1048.005 D5).
    pub body: String,
    pub boot: Option<Boot>,
}

/// Render `location` and write its page after the head a server flushed
/// (`body_js`'s bytes), handing each run to `send` as it is written.
/// `late`: whether the page, by the activation its render decides, needs
/// the entry's preloads its early head left out. `Ok(None)` when the page
/// has no direct render (the plan); nothing was sent. A tree the fold
/// cannot cover is rendered with a kernel here, keeping any boot document's
/// head and swap.
/// `limit` bounds what is sent: a page past it is the error, with what went
/// before it sent.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_js<D: DataSource + 'static, F: Fn() -> D>(
    plan: &Plan,
    data: &F,
    viewport: exact_runner::Viewport,
    location: &str,
    site: &Site,
    deadline: Duration,
    delivery: &exact_runner::delivery::Delivery,
    shell: &str,
    js: &Js,
    late: impl Fn(ActivatePolicy) -> bool,
    limit: usize,
    checkpoints: Option<&crate::serve::deferred::Checkpoints>,
    send: &mut dyn FnMut(&[u8]),
) -> Result<Option<Streamed>, String> {
    if !direct_for(plan, Ids::Any, Projection::Auto)? {
        return Ok(None);
    }
    // A route that paints its boot document first (`paint=boot`, LLP
    // 1048.005): the page with every answer still to come, its placeholders
    // showing, goes before the render waits on anything — after the head's
    // fields, which are the boot document's — in an `#exact-root` marked
    // `data-boot`. The settled page that follows hides it (a style, for a
    // reader without JavaScript) and removes it (a hashed inline script)
    // before the runtime adopts. A page with nothing to wait for has no
    // boot document: it settles as it boots.
    let paints_boot = route_at(plan, location).is_some_and(|r| r.paint == PaintPolicy::Boot);
    let mut boot: Option<Boot> = None;
    let mut page = {
        let mut on_boot = |runner: &mut exact_runner::Runner<crate::Anonymous<D>>| {
            if runner.pending().is_empty() {
                return;
            }
            let Ok(tree) = runner.document_tree() else {
                return;
            };
            let rewrite = runtime_style_of(js);
            let writing = Writing {
                style: Some(&rewrite),
                sink: None,
                prioritize_image: true,
            };
            let Ok((document, _)) = project_tree(&tree, runner, writing) else {
                return;
            };
            let Ok(head) = document.page_head(plan, site, location) else {
                return;
            };
            let (keyframes, scroll) = (document.keyframes.clone(), document.scroll_document);
            let mut lead = body_open_js(shell, js, &head, scroll, false, false, Sheet::Whole);
            preload_image(&mut lead, shell, document.hero_image.as_ref());
            let open = crate::page::root_open(shell);
            let lead = lead.strip_suffix(open).unwrap_or(&lead);
            // The shell's root with its attributes (LLP 1096 D7), marked.
            let bytes = format!(
                "{lead}{} data-boot=\"\">{}</div>",
                &open[..open.len() - 1],
                document.root
            );
            send(bytes.as_bytes());
            boot = Some(Boot {
                sent: bytes.len(),
                keyframes,
                scroll,
                controls: document.controls,
            });
        };
        settle_at(
            plan,
            data,
            viewport,
            location,
            deadline,
            true,
            crate::render_time(),
            delivery,
            paints_boot.then_some(&mut on_boot as &mut dyn FnMut(&mut _)),
        )?
    };
    let tree = match page.runner.document_tree() {
        Ok(tree) => tree,
        Err(why) => {
            crate::log(format_args!("render {location}: with a kernel ({why})"));
            retire(page, data);
            // The kernel's render is another runner's: no view of it is a
            // boot control's, and the swap drops every queued event.
            if let Some(boot) = &mut boot {
                boot.controls.clear();
            }
            let rendered = crate::render_with_at(
                plan,
                data,
                viewport,
                location,
                site,
                deadline,
                Ids::Any,
                Projection::Kernel,
                crate::render_time(),
                delivery,
            )?;
            let late = late(rendered.activate);
            let body = crate::page::body_js(shell, js, &rendered, late, false, checkpoints);
            let after_boot = boot.as_ref().map(|boot| {
                crate::page::body_after_boot_js(
                    shell,
                    js,
                    &rendered,
                    late,
                    false,
                    Some(boot),
                    checkpoints,
                )
            });
            let sent = after_boot.as_deref().unwrap_or(&body);
            let size = boot.as_ref().map_or(0, |boot| boot.sent) + sent.len();
            if size > limit {
                return Err(format!("the page is {size} bytes"));
            }
            send(sent.as_bytes());
            return Ok(Some(Streamed {
                rendered,
                body,
                boot,
            }));
        }
    };
    let state = read_checkpoint(&page.checkpoint).map_err(|e| format!("the checkpoint: {e}"))?;
    let handlers = tree.handlers(plan);
    let (activate, wake) = activation(plan, location, &state, &handlers, page.clocked, &viewport);
    // The head's fields, before any element: the tree's head, its first
    // root's viewport policies, and the `@keyframes` its elements name.
    let (keyframes, scroll, hero_image) = before_root(&tree, plan);
    let runner = &page.runner;
    let first = |id| {
        tree.roots()
            .first()
            .and_then(|root| tree.node(*root))
            .and_then(|n| n.props.str(id).map(str::to_owned))
    };
    let ahead = Document {
        lang: runner.resolved_locale().into(),
        dir: runner.direction().into(),
        root: String::new(),
        viewport_fit: first(exact_kernel::PropId::ViewportFit),
        interactive_widget: first(exact_kernel::PropId::InteractiveWidget),
        head: tree.head(),
        route_title: exact_web::document::route_title(&tree, tree.roots()),
        keyframes,
        scroll_document: scroll,
        hero_image,
        controls: Vec::new(),
    };
    let head = ahead
        .page_head(plan, site, location)
        .map_err(|e| e.to_string())?;
    let mut sent = boot.as_ref().map_or(0, |b| b.sent);
    let mut over = false;
    let mut hand = |bytes: &[u8]| {
        sent += bytes.len();
        if sent > limit {
            over = true;
        } else {
            send(bytes);
        }
    };
    let late = late(activate);
    // The page as a cache keeps it: the settled head, no boot document.
    let mut open = body_open_js(shell, js, &head, scroll, late, false, Sheet::Whole);
    preload_image(&mut open, shell, ahead.hero_image.as_ref());
    match &boot {
        Some(boot) => {
            let mut more = boot.open(shell, js, &ahead, late);
            preload_image(&mut more, shell, ahead.hero_image.as_ref());
            hand(more.as_bytes());
        }
        None => hand(open.as_bytes()),
    }
    let rewrite = runtime_style_of(js);
    let written = {
        let mut sink = |chunk: &str| hand(chunk.as_bytes());
        project_tree(
            &tree,
            runner,
            Writing {
                style: Some(&rewrite),
                sink: Some(&mut sink),
                prioritize_image: true,
            },
        )
    };
    let (document, rest) = match written {
        Ok(done) => done,
        Err(e) => {
            retire(page, data);
            return Err(e.to_string());
        }
    };
    hand(rest.as_bytes());
    let checkpoint = page.checkpoint.clone();
    let digest = digest(&encoded(plan), location, &checkpoint, &document.root);
    let close = body_close_js(
        shell,
        js,
        &digest,
        activate,
        &wake,
        &checkpoint,
        checkpoints,
    );
    let to = boot.as_ref().map(|boot| boot.to(&document.controls));
    hand(close_after_boot(close.clone(), to).as_bytes());
    let settled = page.settled;
    retire(page, data);
    if over {
        return Err(format!("the page is {sent} bytes"));
    }
    let body = open + &document.root + &close;
    Ok(Some(Streamed {
        rendered: Rendered {
            document,
            head,
            checkpoint,
            digest,
            state,
            settled,
            activate,
            wake,
            runtime_form: true,
        },
        body,
        boot,
    }))
}

/// A boot document sent ahead of its settled page: its length, and what of
/// the head's fields the settled page need not repeat.
pub(crate) struct Boot {
    pub sent: usize,
    keyframes: String,
    scroll: bool,
    /// [`Document::controls`] of the boot document.
    controls: Vec<exact_kernel::ViewId>,
}

impl Boot {
    /// The boot head stays in place; only new keyframes and scroll policy follow it.
    pub(crate) fn open(&self, shell: &str, js: &Js, document: &Document, late: bool) -> String {
        let mut more = String::from(SHELL_HIDDEN);
        if document.keyframes != self.keyframes && !document.keyframes.is_empty() {
            more.push_str(&format!("<style>{}</style>", document.keyframes));
        }
        more.push_str(&body_open_js(
            shell,
            js,
            "",
            document.scroll_document && !self.scroll,
            late,
            false,
            Sheet::Sent,
        ));
        more
    }

    /// Each boot control's place among the settled page's `controls`, `-`
    /// where its view is gone: the swap's `data-to`. A view is one instance
    /// of the runner's tree, so the same view is the same logical control.
    pub(crate) fn to(&self, settled: &[exact_kernel::ViewId]) -> String {
        let at: std::collections::HashMap<_, _> =
            settled.iter().enumerate().map(|(i, id)| (*id, i)).collect();
        let places: Vec<String> = self
            .controls
            .iter()
            .map(|id| at.get(id).map_or_else(|| "-".into(), |i| i.to_string()))
            .collect();
        places.join(" ")
    }
}

/// `to`: [`Boot::to`], when a boot document went first.
pub(crate) fn close_after_boot(mut close: String, to: Option<String>) -> String {
    if let Some(to) = to {
        // Before the checkpoint lets the runtime adopt the settled root.
        close.insert_str(
            "</div>".len(),
            &format!("<script data-to=\"{to}\">{}</script>", boot_swap_js()),
        );
    }
    close
}

/// What hides a boot document once the settled page arrives, JavaScript or
/// none.
const SHELL_HIDDEN: &str = "<style>#exact-root[data-boot]{display:none}</style>";

/// The inline script that removes a boot document before the runtime adopts
/// the settled page: each press or edit the capture script queued on a boot
/// control moves to the same control in the settled page (an edit with the
/// control's value), and focus follows. Its `data-to` ([`Boot::to`]) names,
/// for the n-th `[data-exact-on]` element of the boot root, the place of the
/// same view among the settled root's; one whose view is gone, or which
/// hears other events now, takes its queued events with it. An element
/// inside a control is the one at the same place inside it. The server's
/// CSP admits the script by hash.
pub fn boot_swap_js() -> &'static str {
    concat!(
        "(()=>{let d=document,b=d.querySelector(\"#exact-root[data-boot]\"),",
        "s=d.querySelector(\"#exact-root:not([data-boot])\"),",
        "C=r=>[...r.querySelectorAll(\"[data-exact-on]\")],B=C(b),S=C(s),",
        "T=(d.currentScript?.dataset.to??\"\").split(\" \"),",
        "m=e=>{let c=e?.closest?.(\"[data-exact-on]\"),t=S[T[B.indexOf(c)]],p=[];",
        "if(!t||t.dataset.exactOn!=c.dataset.exactOn)return null;",
        "for(;e!=c;e=e.parentElement)p.unshift([...e.parentNode.children].indexOf(e));",
        "for(let n of p)t=t?.children[n];return t},",
        "f=m(d.activeElement),q=self.exact?.q??[],k=[];",
        "for(let i of q){if(!b.contains(i.target)){k.push(i);continue}let t=m(i.target);",
        "if(t){if(i.type!=\"click\"){t.value=i.target.value;t.checked=i.target.checked}i.target=t;k.push(i)}}",
        "q.splice(0,q.length,...k);b.remove();f?.focus()})()"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use exact_plan::Value;
    use exact_runner::{Answer, DataError, Outcome, Request, Response, Store};
    use std::sync::{mpsc, Arc, Mutex};

    const SHELL: &str = r#"<!doctype html>
<html lang="en">
<meta charset="utf-8">
<title>Cards</title>
<meta name="viewport" content="width=device-width, initial-scale=1">
<link rel="modulepreload" href="./app.js">
<style>p{margin:0}</style>
<div id="exact-root"></div>
<script type="module" src="./app.js"></script>
"#;
    const SOURCE: &str = r#"
routes nav
  home "/" render=cached paint=boot
  notfound render=build
component Cards
  resource cards = cards() as shape string[]
  view
    column
      head title="Cards"
      if cards.length == 0
        text "Loading cards"
      for card in cards key=card
        text card font-size="1.5rem"
"#;

    struct Cards(Arc<Mutex<Option<mpsc::Receiver<()>>>>);

    impl DataSource for Cards {
        fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
            Err(DataError::UnknownSource(source.into()))
        }

        fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
            Ok(Answer::Later(Request::continuation(1)))
        }

        fn continuation(&mut self, _: u64) -> Option<Box<dyn FnOnce() -> Outcome + Send>> {
            let hold = self.0.lock().unwrap().take();
            Some(Box::new(move || {
                if let Some(hold) = hold {
                    hold.recv_timeout(Duration::from_secs(30))
                        .expect("the boot document was not sent");
                }
                Outcome::Response(Response {
                    status: 200,
                    headers: vec![],
                    body: vec![],
                })
            }))
        }

        fn parse(
            &mut self,
            _: &mut Store,
            _: &str,
            _: &[Value],
            _: Outcome,
        ) -> Result<Answer, DataError> {
            Ok(Answer::Now(Value::list(vec![
                Value::str("first"),
                Value::str("second"),
            ])))
        }
    }

    #[test]
    fn a_boot_document_is_replaced_when_the_settled_page_needs_a_kernel() {
        let plan = contract::compile(SOURCE).unwrap();
        let (release, hold) = mpsc::channel();
        let hold = Arc::new(Mutex::new(Some(hold)));
        let mut release = Some(release);
        let mut sent = String::new();
        let streamed = render_js(
            &plan,
            &|| Cards(hold.clone()),
            Default::default(),
            "/",
            &Site::default(),
            Duration::from_secs(30),
            &Default::default(),
            SHELL,
            &Js::of(SHELL).unwrap(),
            |_| false,
            usize::MAX,
            None,
            &mut |bytes| {
                sent.push_str(std::str::from_utf8(bytes).unwrap());
                if sent.contains("Loading cards</div></div></div>") {
                    if let Some(release) = release.take() {
                        assert!(sent.contains("data-boot=\"\""));
                        assert!(!sent.contains(">first<"));
                        release.send(()).unwrap();
                    }
                }
            },
        )
        .unwrap()
        .expect("a streamed page");
        let cached = streamed.body;
        assert!(release.is_none(), "the boot document released the answer");
        // The kernel's views are not the boot document's: its swap moves nothing.
        let swap = format!("</div><script data-to=\"\">{}</script>", boot_swap_js());
        let mut rest = sent.as_str();
        for part in [
            "<title>Cards</title>",
            "<div id=\"exact-root\" data-boot=\"\">",
            ">Loading cards<",
            SHELL_HIDDEN,
            "<div id=\"exact-root\">",
            ">first<",
            ">second<",
            &swap,
            "application/vnd.exact.checkpoint",
        ] {
            rest = rest
                .split_once(part)
                .unwrap_or_else(|| panic!("missing {part}: {sent}"))
                .1;
        }
        assert_eq!(sent.matches("<title>").count(), 1);
        assert_eq!(sent.matches("<meta name=\"viewport\"").count(), 1);
        assert!(!cached.contains("data-boot"));
        assert!(!cached.contains(boot_swap_js()));
        assert!(!cached.contains("Loading cards"));
        assert!(cached.contains(">first<") && cached.contains(">second<"));
    }

    /// A boot control's queued events go to its own view in the settled
    /// page, or nowhere: a loading branch's button the answer replaced is not
    /// the button now at its place.
    #[test]
    fn a_boot_control_moves_only_to_its_own_view() {
        let source = r#"
routes nav
  home "/" render=cached paint=boot
  notfound render=build
component Cards
  state n = 0
  resource cards = cards() as shape string[]
  action bump
    n = n + 1
  view
    column
      if cards.length == 0
        button "Cancel" press=bump
      if cards.length > 0
        button "Delete" press=bump
      button "Keep" press=bump
"#;
        let plan = contract::compile(source).unwrap();
        let (release, hold) = mpsc::channel();
        let hold = Arc::new(Mutex::new(Some(hold)));
        let mut release = Some(release);
        let mut sent = String::new();
        render_js(
            &plan,
            &|| Cards(hold.clone()),
            Default::default(),
            "/",
            &Site::default(),
            Duration::from_secs(30),
            &Default::default(),
            SHELL,
            &Js::of(SHELL).unwrap(),
            |_| false,
            usize::MAX,
            None,
            &mut |bytes| {
                sent.push_str(std::str::from_utf8(bytes).unwrap());
                if sent.contains("data-boot=\"\">") {
                    if let Some(release) = release.take() {
                        release.send(()).unwrap();
                    }
                }
            },
        )
        .unwrap()
        .expect("a streamed page");
        let boot = sent.split_once(SHELL_HIDDEN).expect("a boot document").0;
        assert!(
            boot.contains(">Cancel<") && !boot.contains(">Delete<"),
            "{boot}"
        );
        // Cancel's view is gone; Keep is the settled page's second control.
        let swap = format!("<script data-to=\"- 1\">{}</script>", boot_swap_js());
        assert!(sent.contains(&swap), "{sent}");
        let result = crate::compare::location(
            &plan,
            &|| Cards(Arc::new(Mutex::new(None))),
            Default::default(),
            "/",
            &Site::default(),
            Duration::from_secs(30),
            Some(SHELL),
            &Default::default(),
        );
        assert!(result.contains("\"same\":true"), "{result}");
    }

    #[test]
    fn compare_accepts_an_asynchronous_boot_page() {
        let plan = contract::compile(&SOURCE.replace(" font-size=\"1.5rem\"", "")).unwrap();
        let result = crate::compare::location(
            &plan,
            &|| Cards(Arc::new(Mutex::new(None))),
            Default::default(),
            "/",
            &Site::default(),
            Duration::from_secs(30),
            Some(SHELL),
            &Default::default(),
        );
        assert!(result.contains("\"same\":true"), "{result}");
    }
}
