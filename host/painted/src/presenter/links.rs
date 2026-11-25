//! A followed link (LLP 1038 §7; LLP 1045 D4): a press on a Markdown
//! reader's link run, an inline run's `href` or a `link href` follows it,
//! as the web's same-document link and Apple's `ExactSession.follow` do. A
//! path naming one of the app's routes is a location for the navigation
//! root's `navigate`; an `http`, `https`, `mailto` or `tel` target leaves
//! the app: the host's shell opens it (LLP 1115.001 D5), and a host with no
//! shell, which has no browser, logs it instead.
use super::*;

pub(super) type UrlOpener = Box<dyn Fn(&str) -> Result<(), String>>;

impl<D: DataSource> Presenter<D> {
    /// Use the window's default browser, mail app or dialer for external links.
    pub fn set_url_opener(&mut self, open: impl Fn(&str) -> Result<(), String> + 'static) {
        self.url_opener = Some(Box::new(open));
    }

    /// The target a press at `(x, y)` on `hit` follows: the link run under
    /// the point in a painted paragraph, else the nearest `href` at or above
    /// the hit node.
    pub(crate) fn link_at(&self, hit: ViewId, x: f32, y: f32) -> Option<String> {
        let kernel = self.host.kernel();
        let node = kernel.node(hit)?;
        if node.node_type == NodeType::Text {
            if let Some(href) = self.run_link(hit, x, y) {
                return Some(href);
            }
        }
        let mut at = Some(hit);
        while let Some(id) = at {
            let node = kernel.node(id)?;
            if let Some(href) = node.props.link() {
                return Some(href.to_string());
            }
            at = node.parent;
        }
        None
    }

    /// The glyph under a point in `id`'s painted paragraph, and its run's
    /// link: a Markdown run's own target, or an inline run's `href` (its
    /// leaf or an inline ancestor below the paragraph).
    pub(super) fn run_link(&self, id: ViewId, x: f32, y: f32) -> Option<String> {
        let run = self.inline_run_at(id, x, y)?;
        let kernel = self.host.kernel();
        let node = kernel.node(id)?;
        if node.props.str(PropId::Markup) == Some("markdown") {
            let href = &self.paragraph(id)?.runs().get(run)?.href;
            return (!href.is_empty()).then(|| href.clone());
        }
        let mut at = inline_sources(&node).get(run).copied();
        while let Some(leaf) = at.filter(|leaf| *leaf != id) {
            let leaf = kernel.node(leaf)?;
            if let Some(href) = leaf.props.link() {
                return Some(href.to_string());
            }
            at = leaf.parent;
        }
        None
    }

    fn inline_run_at(&self, id: ViewId, x: f32, y: f32) -> Option<usize> {
        let _roles = self.host.runner().bind_roles();
        let kernel = self.host.kernel();
        let node = kernel.node(id)?;
        let paragraph = self.paragraph(id)?;
        let b = self.boxes.iter().find(|b| b.id == id)?.for_hit();
        let content = exact_kernel::svg::scene::content_box(kernel, &node);
        let inverse = crate::placement::inverse(b.scroll_mapping()?)?;
        let (x, y) = crate::placement::map(&inverse, x, y);
        let (tx, ty) = b.text_origin?;
        let (mut px, mut py) = (x - tx, y - ty);
        let lines = if let Some(fragments) = kernel.fragments(node.key) {
            let rect = b.scroll_rect();
            let fragment = fragments
                .iter()
                .find(|g| g.contains(x - rect.0, y - rect.1))?;
            px -= fragment.dx;
            py -= fragment.dy;
            fragment.lines.0 as usize..fragment.lines.1 as usize
        } else {
            py -= kernel.text_line_offset(&node);
            0..usize::MAX
        };
        let left = kernel.text_item_box(&node).map_or(0.0, |item| item.x);
        let (px, py) = (px - content.0 - left, py - content.1);
        // The line whose box the point is in: from a little over the glyphs'
        // ascent above its baseline to their descent below it.
        Some(
            paragraph
                .layout_runs()
                .zip(paragraph.baselines())
                .enumerate()
                .filter(|(i, _)| lines.contains(i))
                .map(|(_, line)| line)
                .flat_map(|(line, baseline)| {
                    line.glyphs
                        .iter()
                        .map(move |g| (g, *baseline, line.run_base))
                })
                .find(|(g, baseline, _)| {
                    (g.x..g.x + g.w).contains(&px)
                        && (baseline - g.font_size * 1.05..baseline + g.font_size * 0.35)
                            .contains(&py)
                })
                .map(|(g, _, base)| g.run(base))?,
        )
    }

    pub(super) fn inline_hit(&self, id: ViewId, x: f32, y: f32) -> Option<ViewId> {
        let kernel = self.host.kernel();
        let node = kernel.node(id)?;
        if node.node_type != NodeType::Text || node.props.str(PropId::Text).is_some() {
            return None;
        }
        let run = self.inline_run_at(id, x, y)?;
        let mut at = inline_sources(&node).get(run).copied();
        while let Some(leaf) = at.filter(|leaf| *leaf != id) {
            let leaf = kernel.node(leaf)?;
            if leaf.computed_row(exact_kernel::StyleId::PointerEvents, |s| s.pointer_events)
                != exact_kernel::PointerEvents::None
                && crate::paint::paints(kernel, leaf.id, None)
                && (leaf.props.link().is_some()
                    || self
                        .host
                        .runner()
                        .handlers_of(leaf.id)
                        .contains(&EventKind::Press))
            {
                return Some(leaf.id);
            }
            at = leaf.parent;
        }
        None
    }

    /// The paragraph whose paint box the inline node `id` shares.
    pub(super) fn inline_block(&self, id: ViewId) -> Option<ViewId> {
        let kernel = self.host.kernel();
        let mut parent = kernel.node(id)?.parent?;
        loop {
            let node = kernel.node(parent)?;
            if node.node_type != NodeType::Text {
                return None;
            }
            if self.boxes.iter().any(|b| b.id == parent) {
                return Some(parent);
            }
            parent = node.parent?;
        }
    }

    /// Inline nodes keep the paragraph's captured ownership and placement,
    /// including wrapping, fragmentation and its scroll mapping.
    pub(super) fn inline_box(&self, id: ViewId) -> Option<PaintedBox> {
        let kernel = self.host.kernel();
        let node = kernel.node(id)?;
        if node.node_type != NodeType::Text || !crate::paint::paints(kernel, id, None) {
            return None;
        }
        let parent = self.inline_block(id)?;
        let mut painted = self.boxes.iter().find(|b| b.id == parent)?.clone();
        let paragraph = self.brush.inline_text.get(&parent)?;
        painted.id = id;
        painted.with_scroll_rect(paragraph.local_frame(id)?)
    }

    /// Follow `href` from `from`: a path naming a declared route navigates
    /// the navigation root above `from`; anything else the reader keeps
    /// leaves the app, where the host's shell can open it.
    pub(crate) fn follow(&mut self, from: ViewId, href: &str, now_ms: f64) {
        // A link to a fragment (`href="#moves"`) is the visit shown, at that
        // fragment (LLP 1153 D5).
        let shown = (href.trim().starts_with('#'))
            .then(|| {
                self.host
                    .router_op()
                    .map(|op| format!("{}{}", op.url, href.trim()))
            })
            .flatten();
        let Some(target) = shown
            .as_deref()
            .or_else(|| crate::text::markup::target(href))
        else {
            self.host.log(format!(
                "link {href:?}: only an app path, http, https, mailto and tel are followed"
            ));
            return;
        };
        if target.starts_with('/') && self.host.runner().route_matches(target) {
            let kernel = self.host.kernel();
            let mut at = Some(from);
            while let Some(id) = at {
                let Some(node) = kernel.node(id) else { break };
                if node.props.str(PropId::NavigationBack).is_some()
                    && self
                        .host
                        .runner()
                        .handlers_of(id)
                        .contains(&EventKind::Navigate)
                {
                    let location = target.to_string();
                    if let Some(error) =
                        self.host.dispatch_at(id, Event::Navigate(location), now_ms)
                    {
                        self.host.log(error);
                    }
                    if let Some(error) = self.after_commit() {
                        self.host.log(error);
                    }
                    return;
                }
                at = node.parent;
            }
            self.host.log(format!(
                "link {target}: no navigation root takes `navigate` here"
            ));
            return;
        }
        // The shell's browser, mail app or dialer.
        if self.leave(target) {
            return;
        }
        // No browser, mail or phone app on the headless/DRM host.
        self.host.log(format!(
            "openURL {target}: the Linux host has no browser to open it in"
        ));
    }

    /// A URL arriving from outside while the app runs (LLP 1133.023 D15
    /// item 3: Android's `onNewIntent`, a shortcut): its location, as a
    /// link's, goes to the navigation root's `navigate` when the root routes
    /// it, at its scheme's registered `protocol_handlers` template when there
    /// is one (`app::incoming_location`). The refusal, else `None`.
    pub fn open_location(&mut self, href: &str, now_ms: f64) -> Option<String> {
        let Some(location) = crate::app::incoming_location_at(href, &self.host.incoming_links)
        else {
            return Some(format!("open {href}: URL is outside this app"));
        };
        if !self.host.runner().route_accepts(&location) {
            return Some(format!("open {location}: no route matches it"));
        }
        let root = self.navigation_root().filter(|root| {
            self.host
                .runner()
                .handlers_of(*root)
                .contains(&EventKind::Navigate)
        });
        let Some(root) = root else {
            return Some(format!(
                "open {location}: no navigation root takes `navigate`"
            ));
        };
        self.host
            .dispatch_at(root, Event::Navigate(location), now_ms)
            .or(self.after_commit())
    }

    /// The launch URL of the first window to show a host that booted with
    /// nothing showing it (LLP 1133.017 D7: Android's boot for a wake), read
    /// as `EXACT_LAUNCH_URL` is at a boot. The refusal, else `None`.
    pub fn launch_location(&mut self, href: &str, now_ms: f64) -> Option<String> {
        let Some(location) = crate::app::incoming_location_at(href, &self.host.incoming_links)
        else {
            return Some(format!("launch {href}: URL is outside this app"));
        };
        self.launch_at(&location, now_ms)
    }

    /// The router as a boot at `location` makes it ([`Runner::host_launch`]).
    pub(crate) fn launch_at(&mut self, location: &str, now_ms: f64) -> Option<String> {
        match self.host.host_launch(location, now_ms) {
            Ok(true) => self.after_commit(),
            Ok(false) => Some(format!("launch {location}: no route matches it")),
            Err(error) => Some(error),
        }
    }
}

fn inline_sources(node: &exact_kernel::NodeRef<'_>) -> Vec<ViewId> {
    fn collect(node: &exact_kernel::NodeRef<'_>, out: &mut Vec<ViewId>) {
        if node.props.str(PropId::Text).is_some() {
            out.push(node.id);
        } else {
            for child in node.child_nodes().filter(|n| n.node_type == NodeType::Text) {
                collect(&child, out);
            }
        }
    }
    let mut out = Vec::new();
    collect(node, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presenter::PainterChoice;

    const SRC: &str = r##"routes nav
  home "/"
  note "/note/:note"
  notfound

component App
  derive current = top(nav)
  action follow(url: string)
    nav = go(nav, url)
  view
    main navigationKey=`${current.id}` navigationBack="back" navigate=follow width="100%" height="100%" testId="root"
      for e in stack(nav) key=e.id
        column navigationKey=`${e.id}` testId=`route-${e.name}` position="absolute" inset=0 padding=10 background-color="#fff"
          text "- [Note](/note/3) and [out](https://example.com) and [inert](ftp://x)" markup="markdown" testId=`reader-${e.name}` font-size=16
"##;

    /// Where the run linking to `href` paints in `reader`, in window points.
    fn link(p: &mut Presenter<()>, reader: &str, href: &str) -> (f32, f32) {
        p.frame();
        let kernel = p.host.kernel();
        let id = kernel
            .node_by_key(kernel.find_by_test_id(reader)[0])
            .unwrap()
            .id;
        let content = exact_kernel::svg::scene::content_box(kernel, &kernel.node(id).unwrap());
        let rect = p.rect_of(id).unwrap();
        let paragraph = p.paragraph(id).unwrap();
        let (line, baseline) = paragraph
            .layout_runs()
            .zip(paragraph.baselines())
            .next()
            .unwrap();
        let g = line
            .glyphs
            .iter()
            .find(|g| paragraph.runs()[g.run(line.run_base)].href == href)
            .unwrap();
        (
            rect.0 + content.0 + g.x + g.w / 2.0,
            rect.1 + content.1 + baseline - 4.0,
        )
    }

    #[test]
    fn a_markdown_link_to_a_route_navigates_and_one_out_of_the_app_is_logged() {
        let (mut p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan(SRC),
            (),
            (400., 300.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        // An inert target paints as words and is no link (`markup::target`).
        assert!(p
            .paragraph_runs_with_href("reader-home")
            .iter()
            .all(|h| h != "ftp://x"));
        let (x, y) = link(&mut p, "reader-home", "/note/3");
        p.press_at(x, y, 10.);
        assert!(
            !p.host.kernel().find_by_test_id("route-note").is_empty(),
            "the route was pushed"
        );
        let (x, y) = link(&mut p, "reader-note", "https://example.com");
        p.press_at(x, y, 20.);
        assert!(
            p.host
                .runner()
                .journal()
                .any(|l| l.contains("openURL https://example.com")),
            "{:?}",
            p.host.runner().journal().collect::<Vec<_>>()
        );
    }

    #[test]
    fn adjacent_inline_links_are_hit_at_their_painted_boundary() {
        let (mut p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan(
                r#"component App
  view
    column padding-left=4
      text font-size=16 testId="line"
        text "A" href="/first" testId="first"
        text "B" href="/second" testId="second"
"#,
            ),
            (),
            (200., 100.),
            2.625,
            std::path::PathBuf::new(),
            PainterChoice::Canvas,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.frame();
        let k = p.host.kernel();
        let node = k.node_by_key(k.find_by_test_id("line")[0]).unwrap();
        assert_eq!(node.frame.x, 4.);
        let ids =
            ["first", "second"].map(|name| k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id);
        let paragraph = p.paragraph(node.id).unwrap();
        let line = paragraph.layout_runs().next().unwrap();
        let second = line
            .glyphs
            .iter()
            .find(|g| g.run(line.run_base) == 1)
            .unwrap();
        let x = node.frame.x + second.x;
        let y = paragraph.baselines()[0] - 4.;
        for (dx, id) in [-0.25, 0.25].into_iter().zip(ids) {
            assert_eq!(p.hit(x + dx / 2.625, y), Some(id));
        }
    }

    #[test]
    fn a_url_from_outside_goes_to_the_root_when_it_routes_it() {
        // The compiler supplies a fallback even when the source omits one.
        let mut plan = exact_plan::Plan::decode(&crate::test_plan(SRC)).unwrap();
        plan.routes.retain(|route| !route.notfound);
        let (mut p, error) = Presenter::boot_with_fixture_fonts(
            &plan.encode(),
            (),
            (400., 300.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        // A scheme's URL as Android's intent data carries it: its location, as a link's.
        assert_eq!(p.open_location("shelf://note/7", 10.), None);
        assert!(!p.host.kernel().find_by_test_id("route-note").is_empty());
        let notes = p.host.kernel().find_by_test_id("route-note").len();
        assert_eq!(p.open_location("/note/8?from=shortcut", 20.), None);
        assert_eq!(
            p.host.kernel().find_by_test_id("route-note").len(),
            notes + 1
        );
        // The agent's `type <root> <url>` delivers a URL as one from outside.
        let kernel = p.host.kernel();
        let root = kernel
            .node_by_key(kernel.find_by_test_id("root")[0])
            .unwrap()
            .id;
        p.type_text(root, "shelf://note/9").unwrap();
        assert_eq!(
            p.host.kernel().find_by_test_id("route-note").len(),
            notes + 2
        );
        // A path no route matches is refused, and nothing moves.
        assert_eq!(
            p.open_location("/elsewhere", 30.).as_deref(),
            Some("open /elsewhere: no route matches it")
        );
        assert_eq!(
            p.host.kernel().find_by_test_id("route-note").len(),
            notes + 2
        );
    }

    #[test]
    fn a_url_from_outside_uses_the_notfound_route() {
        let (mut p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan(SRC),
            (),
            (400., 300.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        assert!(!p.host.runner().route_matches("/nope-missing"));
        assert_eq!(
            p.open_location("shelf:///nope-missing?from=link#detail", 10.),
            None
        );
        let change = p.host.router_op().unwrap();
        assert_eq!(change.location(), "/nope-missing?from=link#detail");
        assert_eq!(change.visits.len(), 2);
        assert_eq!(p.host.kernel().find_by_test_id("route-notfound").len(), 1);
        assert_eq!(p.open_location("/note/7", 20.), None);
        assert_eq!(p.host.kernel().find_by_test_id("route-note").len(), 1);
    }

    impl Presenter<()> {
        fn paragraph_runs_with_href(&mut self, reader: &str) -> Vec<String> {
            self.frame();
            let kernel = self.host.kernel();
            let id = kernel
                .node_by_key(kernel.find_by_test_id(reader)[0])
                .unwrap()
                .id;
            let runs = self.paragraph(id).unwrap().runs();
            runs.iter()
                .map(|r| r.href.clone())
                .filter(|h| !h.is_empty())
                .collect()
        }
    }

    #[test]
    fn android_open_url_waits_for_the_intent_and_keeps_declarations_out_of_links() {
        let source = r#"component App
  state status = ""
  action hear(value: string)
    status = status + value
  view
    input id="result" change=hear
"#;
        let (mut p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan(source),
            (),
            (300., 500.),
            1.,
            Default::default(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none());
        p.compat = r#"{"inputs":{"capabilities":{"openUrlSchemes":["demo","unhandled","file","intent"]}}}"#.into();
        p.serve_requests();
        let status = |p: &Presenter<()>| {
            p.host
                .runner()
                .slot("status")
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned()
        };
        let open = |p: &mut Presenter<()>, url: &str| {
            p.open_url(&[
                exact_plan::Value::from(url),
                exact_plan::Value::from("result"),
            ]);
        };
        assert!(
            !p.leave("demo:///links"),
            "an inline link keeps its own policy"
        );
        p.take_requests();
        open(&mut p, "DeMo:///links?from=test");
        let words = p
            .take_requests()
            .expect("the declared scheme reaches Android");
        assert_eq!(words[0], 1);
        let accepted = words[1] as u32;
        open(&mut p, "unhandled://item");
        let missing = p.take_requests().unwrap()[1] as u32;
        assert_ne!(accepted, missing);
        p.requests_reset();
        assert_eq!(status(&p), "");
        assert!(p.answer_request(missing, Some("failed")));
        assert!(p.answer_request(accepted, Some("opened")));
        assert!(!p.answer_request(accepted, Some("opened")));
        assert_eq!(status(&p), "failedopened");
        p.take_requests();
        for url in [
            "unlisted:thing",
            "file:///path",
            "intent:thing",
            "demo:\0bad",
        ] {
            open(&mut p, url);
            assert!(p.take_requests().is_none());
        }
        assert_eq!(status(&p), "failedopenedfailedfailedfailedfailed");
        open(&mut p, "https://example.com");
        let old = p.take_requests().unwrap()[1] as u32;
        p.open_url(&[exact_plan::Value::from("https://example.com/queued")]);
        p.opens_replaced();
        assert!(!p.answer_request(old, Some("opened")));
        let queued = p
            .take_requests()
            .expect("an issued link still opens after reload");
        assert_eq!(queued[0], 1);
        open(&mut p, " app-settings: ");
        assert!(p.take_requests().is_none());

        p.agent = true;
        let before = status(&p);
        open(&mut p, "demo:///links");
        assert_eq!(status(&p), before + "opened");
        assert!(p.take_requests().is_none());
        assert!(p
            .host
            .runner()
            .journal()
            .any(|s| s.contains("openURL demo: journaled under the agent")));
    }

    #[test]
    fn desktop_links_and_open_url_commands_use_the_registered_handler() {
        use std::{
            cell::{Cell, RefCell},
            rc::Rc,
        };
        let source = r#"component App
  action launch
    openURL("https://example.com/command?x=1&y=2")
  view
    column width=400 height=300
      text "[Open](https://example.com/link)" markup="markdown" testId="reader" font-size=16
      button press=launch testId="launch" height=40
        text "Open a URL"
"#;
        let (mut p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan(source),
            (),
            (400., 300.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let opened = Rc::new(RefCell::new(Vec::new()));
        let fail = Rc::new(Cell::new(false));
        p.set_url_opener({
            let (opened, fail) = (opened.clone(), fail.clone());
            move |url| {
                opened.borrow_mut().push(url.to_owned());
                if fail.get() {
                    Err("no registered handler".into())
                } else {
                    Ok(())
                }
            }
        });
        let (x, y) = link(&mut p, "reader", "https://example.com/link");
        p.press_at(x, y, 10.);
        let launch = p.host.kernel().find_by_test_id("launch")[0];
        p.tap(p.host.kernel().node_by_key(launch).unwrap().id)
            .unwrap();
        p.run_commands(|| ());
        assert_eq!(
            &*opened.borrow(),
            &[
                "https://example.com/link",
                "https://example.com/command?x=1&y=2",
            ]
        );
        for url in [
            " HTTP://example.com/café ",
            "mailto:hello@example.com",
            "tel:+15555550123",
        ] {
            assert!(p.leave(url));
            assert_eq!(opened.borrow().last().unwrap(), url.trim());
        }
        let before = opened.borrow().len();
        for url in [
            "/route",
            "//example.com",
            "file:///test.exe",
            "javascript:alert(1)",
            "ftp://example.com",
            "https://example.com\0ignored",
        ] {
            assert!(!p.leave(url), "{url:?}");
        }
        p.open_url(&[exact_plan::Value::from("app-settings:")]);
        assert_eq!(opened.borrow().len(), before);
        assert!(p.take_requests().is_none());
        fail.set(true);
        assert!(p.leave("https://example.com/unavailable"));
        assert_eq!(opened.borrow().len(), before + 1);
        assert!(p
            .host
            .runner()
            .journal()
            .any(|line| line.contains("no registered handler")));
    }
}
