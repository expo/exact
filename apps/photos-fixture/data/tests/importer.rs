//! The photo index against a scripted library (LLP 1133.019.000 D1), its
//! SQLite file real: the first import, a walk resumed across a relaunch, a
//! replacement when history runs out, one replacement a trigger on a
//! rescan-only view, and a failed read that keeps the committed index. After
//! each sync the index is compared with the library's own list of the view.

use exact_data_host::Storage;
use exact_plan::Value;
use exact_runner::{Answer, DataSource, Dispatch, Outcome, Reply, Response, Store, Work};
use photos_fixture_data::Fixture;
use serde_json::{json, Value as Json};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A change made to the library while the app reads it.
type Change = Box<dyn FnOnce(&mut Library)>;

/// A library as `@exact/photos` answers it. Each change has a sequence
/// number; a history token is `at <n>`, a walk's continuation `walk <start>
/// <end> <after>`. A rescan-only library answers every token `expired`.
#[derive(Default)]
struct Library {
    /// Each photo's creation date and whether it is a screenshot.
    photos: BTreeMap<String, (f64, bool)>,
    /// Each change: its sequence number and the photo.
    changes: Vec<(u64, String)>,
    seq: u64,
    /// History before this is gone: an older token answers `expired`.
    floor: u64,
    rescan: bool,
    /// The next read of this source fails with this code.
    fail: Option<(&'static str, &'static str)>,
    /// Run before the answer to this source's read of this number, counted
    /// from 1 over the library's life.
    during: Option<(&'static str, usize, Change)>,
    /// The reads of each source.
    reads: BTreeMap<String, usize>,
    /// The module's pages answered.
    pages: usize,
}

impl Library {
    fn add(&mut self, id: &str, created: f64, screenshot: bool) {
        self.photos.insert(id.into(), (created, screenshot));
        self.touch(id);
    }

    fn remove(&mut self, id: &str) {
        self.photos.remove(id);
        self.touch(id);
    }

    fn touch(&mut self, id: &str) {
        self.seq += 1;
        self.changes.push((self.seq, id.into()));
    }

    fn view(&self, shots: bool) -> Vec<(&String, f64)> {
        let mut photos: Vec<_> = (self.photos.iter())
            .filter(|(_, (_, s))| !shots || *s)
            .map(|(id, (created, _))| (id, *created))
            .collect();
        photos.sort_by(|a, b| b.1.total_cmp(&a.1).then(b.0.cmp(a.0)));
        photos
    }

    fn asset(&self, id: &str) -> Json {
        let (created, _) = self.photos[id];
        json!({"id": id, "mediaType": "image", "width": 1, "height": 1, "creationDate": created,
            "favorite": false, "picture": format!("photos:{id}")})
    }

    fn answer(&mut self, source: &str, args: &Json) -> Json {
        self.pages += 1;
        let n = self.reads.entry(source.into()).or_default();
        *n += 1;
        let n = *n;
        if (self.during.as_ref()).is_some_and(|(s, at, _)| *s == source && *at == n) {
            let (_, _, change) = self.during.take().unwrap();
            change(self);
        }
        if let Some((failing, code)) = self.fail {
            if failing == source {
                self.fail = None;
                return json!({"error": code});
            }
        }
        let shots = args["subtypes"].as_array().is_some_and(|s| !s.is_empty());
        let limit = args["limit"].as_f64().unwrap_or(500.0) as usize;
        match source {
            "photoAssets" => self.list(shots, limit, args["cursor"].as_str()),
            _ => self.walk(shots, limit, args["since"].as_str()),
        }
    }

    fn list(&self, shots: bool, limit: usize, cursor: Option<&str>) -> Json {
        let view = self.view(shots);
        let start = match cursor {
            Some(after) => view
                .iter()
                .position(|(id, _)| *id == after)
                .map_or(view.len(), |i| i + 1),
            None => 0,
        };
        let page: Vec<_> = view.iter().skip(start).take(limit).collect();
        let cursor = (start + page.len() < view.len()).then(|| page.last().unwrap().0.clone());
        json!({"assets": page.iter().map(|(id, _)| self.asset(id)).collect::<Vec<_>>(), "cursor": cursor})
    }

    fn walk(&self, shots: bool, limit: usize, since: Option<&str>) -> Json {
        let expired = || json!({"added": [], "updated": [], "deleted": [], "token": self.fresh(), "more": false, "expired": true});
        let Some(since) = since.filter(|s| !s.is_empty()) else {
            return json!({"added": [], "updated": [], "deleted": [], "token": self.fresh(), "more": false, "expired": false});
        };
        let words: Vec<&str> = since.split(' ').collect();
        let (start, end, after) = match words[..] {
            ["rescan"] => return expired(),
            ["at", n] => (n.parse::<u64>().unwrap(), self.seq, None),
            ["walk", s, e, a] => (s.parse().unwrap(), e.parse().unwrap(), Some(a)),
            _ => return json!({"error": "SyntaxError"}),
        };
        if self.rescan || start < self.floor {
            return expired();
        }
        let mut candidates: Vec<&String> = (self.changes.iter())
            .filter(|(n, _)| *n > start && *n <= end)
            .map(|(_, id)| id)
            .filter(|id| after.is_none_or(|a| id.as_str() > a))
            .collect();
        candidates.sort();
        candidates.dedup();
        let page: Vec<_> = candidates.iter().take(limit).collect();
        let more = candidates.len() > page.len();
        let inserted = |id: &str| !(self.changes.iter()).any(|(n, c)| *n <= start && c == id);
        let (mut added, mut updated, mut deleted) = (vec![], vec![], vec![]);
        for id in &page {
            match self.photos.get(id.as_str()) {
                Some((_, s)) if !shots || *s => match inserted(id) {
                    true => added.push(self.asset(id)),
                    false => updated.push(self.asset(id)),
                },
                _ => deleted.push(json!(id)),
            }
        }
        let token = match more {
            true => format!("walk {start} {end} {}", page.last().unwrap()),
            false => format!("at {end}"),
        };
        json!({"added": added, "updated": updated, "deleted": deleted, "token": token, "more": more, "expired": false})
    }

    fn fresh(&self) -> String {
        match self.rescan {
            true => "rescan".into(),
            false => format!("at {}", self.seq),
        }
    }
}

struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        ibex2_sqlite::link();
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        Self(std::env::temp_dir().join(format!(
            "exact-photos-index-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// One launch of the app: its data source over the root's storage.
struct App {
    data: Storage<Fixture>,
    store: Store,
}

/// What `photoIndex` answered.
#[derive(Debug, PartialEq)]
struct Index {
    count: usize,
    digest: String,
    how: String,
    pages: usize,
    more: bool,
    error: String,
}

impl App {
    fn launch(root: &Root) -> Self {
        let mut data = Storage::new(Fixture::default());
        data.configure_storage(
            root.0.join("data"),
            root.0.join("cache"),
            root.0.join("tmp"),
        )
        .unwrap();
        data.activate().unwrap();
        let store = Store::new(data.grants(), Vec::<(String, Vec<u8>)>::new());
        Self { data, store }
    }

    fn run(&mut self, library: &mut Library, source: &str, args: &[Value]) -> Vec<Value> {
        let mut answer = self.data.answer(&mut self.store, source, args).unwrap();
        for _ in 0..10_000 {
            let request = match answer {
                Answer::Now(Value::Record(fields)) => return fields.to_vec(),
                Answer::Now(other) => panic!("{source}: {other:?}"),
                Answer::Later(request) => request,
            };
            let outcome = match request.continuation {
                Some(token) => match self.data.dispatch(token, &self.store) {
                    Dispatch::Run(Work::Now(work)) => work(),
                    Dispatch::Run(Work::Later(hand)) => {
                        let (tx, rx) = std::sync::mpsc::channel();
                        hand(Reply::new(move |o| {
                            let _ = tx.send(o);
                        }));
                        rx.recv().unwrap()
                    }
                    _ => panic!("expected storage work"),
                },
                None => {
                    let body: Json = serde_json::from_slice(&request.body).unwrap();
                    assert_eq!(body["extension"], "photos");
                    let reply = library.answer(body["source"].as_str().unwrap(), &body["args"]);
                    Outcome::Response(Response {
                        status: 200,
                        headers: vec![],
                        body: reply.to_string().into_bytes(),
                    })
                }
            };
            answer = self
                .data
                .parse(&mut self.store, source, args, outcome)
                .unwrap();
        }
        panic!("{source} did not settle");
    }

    fn sync(&mut self, library: &mut Library, shots: bool, limit: f64, budget: f64) -> Index {
        let args = [
            Value::Bool(shots),
            Value::Number(limit),
            Value::Number(budget),
            Value::str("full"),
        ];
        let pages = library.pages;
        let f = self.run(library, "photoIndex", &args);
        let pages_seen = library.pages - pages;
        let index = Index {
            count: f[0].as_number().unwrap() as usize,
            digest: f[1].as_str().unwrap().into(),
            how: f[2].as_str().unwrap().into(),
            pages: f[3].as_number().unwrap() as usize,
            more: f[4] == Value::Bool(true),
            error: f[6].as_str().unwrap().into(),
        };
        assert_eq!(index.pages, pages_seen, "the pages the sync says it read");
        index
    }

    /// The view listed whole: its count and digest.
    fn listing(&mut self, library: &mut Library, shots: bool) -> (usize, String) {
        let f = self.run(library, "photoListing", &[Value::Bool(shots)]);
        assert_eq!(f[2].as_str(), Some(""));
        (
            f[0].as_number().unwrap() as usize,
            f[1].as_str().unwrap().into(),
        )
    }

    fn in_step(&mut self, library: &mut Library, shots: bool, index: &Index) -> bool {
        let (pages, reads) = (library.pages, library.reads.clone());
        let listed = self.listing(library, shots);
        (library.pages, library.reads) = (pages, reads);
        listed == (index.count, index.digest.clone())
    }
}

fn filled(n: usize) -> Library {
    let mut library = Library::default();
    for i in 0..n {
        library.add(&format!("p{i:04}"), (i % 7) as f64, i % 3 == 0);
    }
    library
}

#[test]
fn the_first_import_lists_every_page_and_later_syncs_walk() {
    let root = Root::new();
    let mut library = filled(1203);
    let mut app = App::launch(&root);
    let first = app.sync(&mut library, false, 500.0, 0.0);
    // A checkpoint, three pages of the list and one walk from the checkpoint.
    assert_eq!(
        (
            first.count,
            first.how.as_str(),
            first.pages,
            first.error.as_str()
        ),
        (1203, "replaced", 5, "")
    );
    assert!(app.in_step(&mut library, false, &first));

    library.add("new", 99.0, false);
    library.remove("p0005");
    let next = app.sync(&mut library, false, 500.0, 0.0);
    assert_eq!(
        (next.count, next.how.as_str(), next.pages),
        (1203, "walked", 1)
    );
    assert!(app.in_step(&mut library, false, &next));

    // The screenshots alone are an index of their own.
    let shots = app.sync(&mut library, true, 500.0, 0.0);
    assert_eq!((shots.count, shots.how.as_str()), (401, "replaced"));
    assert!(app.in_step(&mut library, true, &shots));
}

#[test]
fn a_walk_stopped_short_goes_on_from_its_last_page_after_a_relaunch() {
    let root = Root::new();
    let mut library = filled(5);
    App::launch(&root).sync(&mut library, false, 2.0, 0.0);
    for i in 0..7 {
        library.add(&format!("q{i}"), 50.0, false);
    }
    library.remove("p0001");
    // Walks of two, two pages this launch: four of the eight changes committed.
    let stopped = App::launch(&root).sync(&mut library, false, 2.0, 2.0);
    assert_eq!(
        (
            stopped.count,
            stopped.how.as_str(),
            stopped.pages,
            stopped.more
        ),
        (7, "walked", 2, true)
    );
    let mut app = App::launch(&root);
    let rest = app.sync(&mut library, false, 2.0, 0.0);
    assert_eq!(
        (rest.count, rest.how.as_str(), rest.pages, rest.more),
        (11, "walked", 2, false)
    );
    assert!(app.in_step(&mut library, false, &rest));
}

#[test]
fn a_replacement_stopped_or_failed_mid_list_keeps_the_committed_index() {
    let root = Root::new();
    let mut library = filled(1009);
    let first = App::launch(&root).sync(&mut library, false, 500.0, 0.0);
    library.floor = library.seq + 1;
    library.add("late", 9.0, false);
    // History gone: the walk answers expired, and the replacement is stopped
    // after the first page of its list.
    let stopped = App::launch(&root).sync(&mut library, false, 500.0, 2.0);
    assert_eq!(
        (stopped.how.as_str(), stopped.count, &stopped.digest),
        ("interrupted", 1009, &first.digest)
    );
    // Then refused on the second page of its list.
    let mut app = App::launch(&root);
    let second = library.reads["photoAssets"] + 2;
    library.fail = None;
    library.during = Some((
        "photoAssets",
        second,
        Box::new(|l: &mut Library| l.fail = Some(("photoAssets", "NotAllowedError"))),
    ));
    let failed = app.sync(&mut library, false, 500.0, 0.0);
    assert_eq!(
        (
            failed.how.as_str(),
            failed.error.as_str(),
            failed.count,
            &failed.digest
        ),
        ("failed", "NotAllowedError", 1009, &first.digest)
    );
    // The next trigger replaces it whole.
    let replaced = app.sync(&mut library, false, 500.0, 0.0);
    assert_eq!(
        (
            replaced.how.as_str(),
            replaced.count,
            replaced.error.as_str()
        ),
        ("replaced", 1010, "")
    );
    assert!(app.in_step(&mut library, false, &replaced));
}

#[test]
fn a_change_during_the_list_is_caught_up_by_the_walk_from_its_checkpoint() {
    let root = Root::new();
    let mut library = filled(503);
    let mut app = App::launch(&root);
    // After the list's first page: the newest photo, listed, goes, and a newer
    // one, which the list has passed, comes.
    library.during = Some((
        "photoAssets",
        2,
        Box::new(|l: &mut Library| {
            l.remove("p0496");
            l.add("new", 99.0, false);
        }),
    ));
    let first = app.sync(&mut library, false, 500.0, 0.0);
    assert_eq!((first.how.as_str(), first.count), ("replaced", 503));
    assert!(app.in_step(&mut library, false, &first));
}

#[test]
fn a_catch_up_that_expires_after_a_page_drops_its_changes_and_keeps_the_checkpoint() {
    let root = Root::new();
    let mut library = filled(503);
    let mut app = App::launch(&root);
    // Before the list's second page, three changes, which the catch-up walks
    // two a page; its history goes after its first page.
    library.during = Some((
        "photoAssets",
        2,
        Box::new(|l: &mut Library| {
            l.remove("p0496");
            l.add("new", 99.0, false);
            l.add("newer", 100.0, false);
            l.during = Some((
                "photoChanges",
                3,
                Box::new(|l: &mut Library| l.floor = l.seq + 1),
            ));
        }),
    ));
    let first = app.sync(&mut library, false, 2.0, 0.0);
    // The list as read, with the photo it listed before its deletion.
    assert_eq!(
        (first.how.as_str(), first.count, first.pages),
        ("replaced", 503, 5)
    );
    assert!(!app.in_step(&mut library, false, &first));
    // The checkpoint kept is the one taken before the list: with its history
    // back, the next walk catches up from it.
    library.floor = 0;
    let next = app.sync(&mut library, false, 2.0, 0.0);
    assert_eq!((next.how.as_str(), next.count), ("walked", 504));
    assert!(app.in_step(&mut library, false, &next));
}

#[test]
fn a_catch_up_that_fails_after_a_page_keeps_the_committed_index() {
    let root = Root::new();
    let mut library = filled(503);
    let mut app = App::launch(&root);
    let first = app.sync(&mut library, false, 2.0, 0.0);
    // History gone: the stored checkpoint expires. Before the list's second
    // page, three changes; the catch-up's second page is refused.
    library.floor = library.seq + 1;
    library.add("late", 1.0, false);
    library.reads.clear();
    library.during = Some((
        "photoAssets",
        2,
        Box::new(|l: &mut Library| {
            for id in ["a", "b", "c"] {
                l.add(id, 1.0, false);
            }
            l.during = Some((
                "photoChanges",
                3,
                Box::new(|l: &mut Library| l.fail = Some(("photoChanges", "UnknownError"))),
            ));
        }),
    ));
    let failed = app.sync(&mut library, false, 2.0, 0.0);
    assert_eq!(
        (
            failed.how.as_str(),
            failed.error.as_str(),
            failed.count,
            &failed.digest
        ),
        ("failed", "UnknownError", 503, &first.digest)
    );
    // The stored checkpoint stands: with its history back, one walk takes in
    // all four.
    library.floor = 0;
    let next = app.sync(&mut library, false, 500.0, 0.0);
    assert_eq!((next.how.as_str(), next.count), ("walked", 507));
    assert!(app.in_step(&mut library, false, &next));
}

#[test]
fn a_rescan_only_view_replaces_once_a_trigger() {
    let root = Root::new();
    let mut library = filled(4);
    library.rescan = true;
    let mut app = App::launch(&root);
    // No checkpoint: a list, and the catch-up walk from its token expires, so the list stands.
    let first = app.sync(&mut library, false, 500.0, 0.0);
    assert_eq!(
        (first.how.as_str(), first.pages, first.count),
        ("replaced", 3, 4)
    );
    library.add("shot", 9.0, true);
    // The stored token expires: one list, one catch-up that expires, and no more.
    let next = app.sync(&mut library, false, 500.0, 0.0);
    assert_eq!(
        (next.how.as_str(), next.pages, next.count),
        ("replaced", 3, 5)
    );
    assert!(app.in_step(&mut library, false, &next));
}

#[test]
fn a_token_the_module_refuses_is_replaced_and_other_failures_keep_the_index() {
    let root = Root::new();
    let mut library = filled(3);
    let mut app = App::launch(&root);
    app.sync(&mut library, false, 500.0, 0.0);
    library.add("x", 1.0, false);
    library.fail = Some(("photoChanges", "SyntaxError"));
    let refused = app.sync(&mut library, false, 500.0, 0.0);
    assert_eq!(
        (refused.how.as_str(), refused.count, refused.error.as_str()),
        ("replaced", 4, "")
    );
    library.add("y", 1.0, false);
    library.fail = Some(("photoChanges", "NotAllowedError"));
    let denied = app.sync(&mut library, false, 500.0, 0.0);
    assert_eq!(
        (denied.how.as_str(), denied.count, denied.error.as_str()),
        ("failed", 4, "NotAllowedError")
    );
    let caught = app.sync(&mut library, false, 500.0, 0.0);
    assert_eq!((caught.how.as_str(), caught.count), ("walked", 5));
    assert!(app.in_step(&mut library, false, &caught));
}
