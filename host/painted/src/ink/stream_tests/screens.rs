//! App screens under the recorder: Caltrain's, a sheet over glass, and the heavy list.
//! @ref LLP 1172.023.006 D5.
use super::*;
use crate::presenter::PainterChoice;
use exact_runner::DataSource;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn root(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

/// One app on two presenters, the CPU painter's and the recording Ink's, driven alike.
/// Both draw the fixture fonts, and their clocks stand at zero.
pub(super) struct Pair<D: DataSource>([crate::Presenter<D>; 2]);

impl<D: DataSource + Default> Pair<D> {
    pub(super) fn boot(plan: &[u8], size: (f32, f32), assets: &str, ink: Ink) -> Self {
        let boot = || {
            let (p, error) = crate::Presenter::boot_with_fixture_fonts(
                plan,
                D::default(),
                size,
                1.,
                root(assets),
                PainterChoice::Cpu,
            )
            .unwrap();
            assert!(error.is_none(), "{error:?}");
            p
        };
        let mut pair = Self([boot(), boot()]);
        pair.0[1].reader_replaced(Box::new(ink));
        pair.each(|p| {
            p.wait_images(Duration::from_secs(10));
        });
        pair
    }

    fn each(&mut self, drive: impl Fn(&mut crate::Presenter<D>)) {
        for p in &mut self.0 {
            // Two sessions live on this thread, and `wait_images` binds no roles of its own.
            let _roles = p.host().runner().bind_roles();
            drive(p);
        }
    }

    /// Scroll `port`, or the page's one scroller, to `y`.
    pub(super) fn scroll(&mut self, port: Option<&str>, y: f32) {
        self.each(|p| {
            let port = match port {
                Some(port) => id(p, port),
                None => {
                    let ports: Vec<_> = p.boxes().iter().filter(|b| b.scroll.is_some()).collect();
                    assert_eq!(ports.len(), 1, "one scroller");
                    ports[0].id
                }
            };
            assert!(p.scroll_to(port, (0., y)).is_some(), "a scroller");
        });
    }

    /// Press `button` and run the commands its action issued.
    fn tap(&mut self, button: &str) {
        self.each(|p| {
            p.tap(id(p, button)).unwrap();
            p.run_commands(D::default);
        });
    }

    /// A frame of each. The pictures it asked for then load, so that the next frame
    /// shows them whatever the decoders' pace.
    pub(super) fn frame(&mut self, frame: &mut dyn FnMut(Pixmap, Pixmap)) {
        let [cpu, ink] = &mut self.0;
        frame((*cpu.frame()).clone(), (*ink.frame()).clone());
        self.each(|p| {
            p.wait_images(Duration::from_secs(10));
        });
    }
}

fn id<D: DataSource>(p: &crate::Presenter<D>, test_id: &str) -> u32 {
    let kernel = p.host().kernel();
    let key = kernel.find_by_test_id(test_id)[0];
    kernel.node_by_key(key).unwrap().id
}

/// Caltrain as the pinned suite boots it.
fn caltrain_app(ink: Ink) -> Pair<caltrain_data::Caltrain> {
    static PLAN: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    let plan = PLAN.get_or_init(|| caltrain::build().unwrap().encode());
    Pair::boot(plan, (390., 844.), "apps/caltrain", ink)
}

/// Caltrain's Home, three frames of its scroll, the station picker, another
/// station's Home, and that Home without its sky.
pub(super) fn caltrain(ink: Ink, frame: &mut dyn FnMut(Pixmap, Pixmap)) {
    let mut app = caltrain_app(ink);
    app.frame(frame);
    for y in [61.5, 148.25, 260.75] {
        app.scroll(None, y);
        app.frame(frame);
    }
    app.scroll(None, 0.);
    for button in ["change-station", "station-paloalto", "sky-toggle"] {
        app.tap(button);
        app.frame(frame);
    }
}

/// Caltrain's station picker alone: an app's screen small enough to commit.
pub(super) fn stations(ink: Ink, frame: &mut dyn FnMut(Pixmap, Pixmap)) {
    let mut app = caltrain_app(ink);
    app.tap("change-station");
    app.frame(frame);
}

/// A sheet over a page of rows under a glass bar. The sheet is a material with two
/// shadows and a rounded clip at half points, around a rounded scroller of clipped,
/// translucent rows. The page and the scroller move by fractions under them.
pub(super) fn sheet(ink: Ink, frame: &mut dyn FnMut(Pixmap, Pixmap)) {
    let rows: String = (0..14)
        .map(|n| {
            let hue = ["#e53935", "#1e88e5", "#43a047", "#fdd835"][n % 4];
            format!(
                "          row height=38 padding=10 gap=10 align-items=\"center\" background-color=\"#ffffff\" border-bottom=\"1px solid #d8dbe2\"
            box width=38 height=38 border-radius=19 background-image=\"linear-gradient(135deg, {hue}, #ffffff)\" box-shadow=\"0 1px 3px #00000055\"
            text \"Message {n}\" font-size=15
"
            )
        })
        .collect();
    let choices: String = (0..8)
        .map(|n| {
            let opacity = if n % 3 == 1 { 0.55 } else { 1. };
            format!(
                "            row height=28 padding=8 gap=8 align-items=\"center\" opacity={opacity} border-bottom=\"1px solid #00000022\"
              box width=28 height=28 border-radius=9 overflow=\"hidden\" opacity=0.8
                box width=40 height=40 background-image=\"conic-gradient(#e53935, #fdd835, #43a047, #1e88e5, #e53935)\"
              text \"Choice {n}\" font-size=14
"
            )
        })
        .collect();
    let plan = crate::test_plan(&format!(
        "component App
  view
    column position=\"relative\" width=\"100%\" height=\"100%\" background-color=\"#eef0f4\"
      scroll testId=\"page\" width=\"100%\" height=\"100%\"
        column padding-top=52
{rows}      row position=\"absolute\" left=0 top=0 width=\"100%\" height=52 box-sizing=\"border-box\" padding=14 align-items=\"center\" background-color=\"#ffffff99\" backdrop-filter=\"blur(20px) saturate(1.8)\" border-bottom=\"1px solid #00000022\"
        text \"Inbox\" font-size=17 font-weight=700
      box position=\"absolute\" left=0 top=0 width=\"100%\" height=\"100%\" background-color=\"#00000040\"
      column position=\"absolute\" left=24.5 top=90.5 width=271 border-radius=22 overflow=\"hidden\" background-color=\"#ffffff8c\" backdrop-filter=\"blur(24px) saturate(1.8)\" box-shadow=\"0 18px 40px #00000059, 0 2px 6px #00000033\"
        text \"Choose one\" font-size=16 font-weight=700 padding=14
        scroll testId=\"choices\" height=150.5 margin-left=12 margin-right=12 border-radius=14 background-color=\"#ffffffb3\"
          column
{choices}        row padding=12 gap=8 justify-content=\"flex-end\"
          box width=72 height=30 border-radius=15 background-color=\"#ffffff\" box-shadow=\"0 1px 4px #00000040, inset 0 0 0 1px #00000014\"
          box width=72 height=30 border-radius=15 background-image=\"linear-gradient(#42a5f5, #1e88e5)\" box-shadow=\"0 3px 8px #1e88e566\"
"
    ));
    let mut app = Pair::<()>::boot(&plan, (320., 440.), "", ink);
    for (page, choices) in [(0., 0.), (37.5, 23.25), (81.25, 61.75)] {
        app.scroll(Some("page"), page);
        app.scroll(Some("choices"), choices);
        app.frame(frame);
    }
}

/// The heavy list's feed, from the copy of the benchmark's messages that
/// `bench/heavy-list/prepare.sh` makes. Its data crate embeds that file, which a
/// checkout does not hold, so this restates the crate's `Message::parse`.
struct Feed(exact_plan::Value);

impl Feed {
    fn read() -> Self {
        use exact_plan::Value;
        let path = root("bench/heavy-list/exact-heavylist/data/messages.json");
        let json = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("{}: {e}; run bench/heavy-list/prepare.sh", path.display()));
        let json: serde_json::Value = serde_json::from_slice(&json).unwrap();
        let text = |v: &serde_json::Value| Value::str(v.as_str().unwrap_or_default());
        let number = |v: &serde_json::Value| Value::Number(v.as_f64().unwrap());
        let list = |v: &serde_json::Value| v.as_array().cloned().unwrap_or_default();
        let option = |v: Option<Value>| Value::Option(v.map(std::rc::Rc::new));
        let rows = list(&json["messages"])
            .iter()
            .map(|m| {
                let paragraphs = (list(&m["paragraphs"]).iter().enumerate())
                    .map(|(i, runs)| {
                        let runs = (list(runs).iter().enumerate())
                            .map(|(j, r)| {
                                let id = Value::str(&j.to_string());
                                Value::record(vec![id, text(&r["t"]), text(&r["s"])])
                            })
                            .collect();
                        Value::record(vec![Value::str(&i.to_string()), Value::list(runs)])
                    })
                    .collect();
                let photos = list(&m["photos"]);
                let photos = photos.first().map(|first| {
                    let src =
                        |at: usize| photos.get(at).map_or(Value::str(""), |p| text(&p["src"]));
                    let ratio = first["w"].as_f64().unwrap() / first["h"].as_f64().unwrap();
                    let count = Value::Number(photos.len() as f64);
                    Value::record(vec![
                        count,
                        src(0),
                        src(1),
                        src(2),
                        src(3),
                        Value::Number(ratio),
                    ])
                });
                let link = m.get("link").filter(|l| !l.is_null()).map(|l| {
                    let site = Value::str(&l["site"].as_str().unwrap().to_uppercase());
                    Value::record(vec![
                        text(&l["thumb"]),
                        text(&l["title"]),
                        text(&l["description"]),
                        site,
                    ])
                });
                let quote = (m.get("quote").filter(|q| !q.is_null()))
                    .map(|q| Value::record(vec![text(&q["author"]), text(&q["excerpt"])]));
                let reactions = (list(&m["reactions"]).iter())
                    .map(|r| Value::record(vec![text(&r["emoji"]), number(&r["count"])]))
                    .collect();
                Value::record(vec![
                    text(&m["id"]),
                    text(&m["author"]),
                    text(&m["avatar"]),
                    number(&m["minutesAgo"]),
                    Value::list(paragraphs),
                    option(photos),
                    option(link),
                    option(quote),
                    Value::list(reactions),
                ])
            })
            .collect();
        Self(Value::record(vec![Value::list(rows)]))
    }
}

impl DataSource for Feed {
    fn query(
        &mut self,
        source: &str,
        _: &[exact_plan::Value],
    ) -> std::result::Result<exact_plan::Value, exact_runner::DataError> {
        assert_eq!(
            source, "messages",
            "the clock stands, so nothing else is asked"
        );
        Ok(self.0.clone())
    }
}

/// The heavy list in the benchmark's Windows window, its feed baked as the
/// benchmark's builds bake it: the first screen, then 24 frames of a scroll, 23.375
/// points each. A picture that comes into view shows a frame after its row does.
pub(super) fn heavy(ink: Ink, frame: &mut dyn FnMut(Pixmap, Pixmap)) {
    let app = "bench/heavy-list/exact-heavylist";
    let plan = contract::compile_path(&root(app).join("app.contract")).unwrap();
    let plan = contract::bake(plan, Feed::read()).unwrap().encode();
    let mut app = Pair::<()>::boot(&plan, (800., 768.), app, ink);
    app.frame(frame);
    for at in 1..=24 {
        app.scroll(Some("messages"), at as f32 * 23.375);
        app.frame(frame);
    }
}
