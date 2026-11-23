//! The app's own index of the photo library, kept in step with it as LLP
//! 1133.019.000 D1 says an app keeps one: the importer of expo/exact#714.
//! `photoIndex(screenshots, limit, budget, access)` syncs the index of one
//! view (all photos, or the screenshots alone) and answers what it holds;
//! `photoListing(screenshots)` lists the same view whole, for the drives to
//! compare with.
//!
//! The index is a SQLite file: each photo's id and creation date by view, and
//! each view's checkpoint (a `photoChanges` token). A sync:
//!
//! 1. With a checkpoint, walks `photoChanges` from it, at most one walk,
//!    `limit` changes a page, and commits each page's upserts, removals and
//!    next token in one transaction, so a relaunch mid-walk resumes from the
//!    last page committed.
//! 2. With none, or when the walk answers `expired` or refuses the stored
//!    token (`SyntaxError`), replaces the index: a checkpoint taken first (the
//!    `expired` answer's fresh token, else `photoChanges` without `since`),
//!    then every page of `photoAssets`, 500 a page, each written to a staging
//!    table.
//! 3. Walks once from that checkpoint, keeping its changes. Should a page
//!    answer `expired` (always, on a rescan-only view), they are dropped and
//!    the list stands with the checkpoint taken before it. The staged list,
//!    the changes and the token replace the index in one transaction.
//!
//! Each trigger runs one sync and never loops: on a rescan-only view, one
//! replacement. A failed read keeps the committed index, answered with the
//! error's code. The runner serialises the syncs: an announcement while one
//! runs asks the resource again once it answers (LLP 1016.002 D4).
//!
//! `budget`, when not zero, stops a sync after that many pages of the module,
//! as a relaunch would: a walk keeps the pages it committed, and a replacement
//! is discarded. `access`, the status's, is an argument only so that a change
//! of it supersedes the sync in flight, whose staged read D1 discards. A
//! superseded sync's write already sent still lands, and before the next
//! sync reads (storage requests share one ordered lane): a walk's page the
//! next walk replays, or a whole replacement.

use crate::modules::photos;
use exact_data::modules::ModuleError;
use exact_data::storage;
use exact_plan::Value;
use exact_runner::{Answer, Outcome, Request};
use serde_json::{json, Value as Json};
use std::collections::BTreeMap;

const DB: &str = "app:/data/photos.db";
/// A page of the module at most, and the list's page (LLP 1133.019.000 D1).
const PAGE: f64 = 500.0;
/// Rows a SQL statement writes at most: three parameters each, within
/// SQLite's limit of 32,766.
const ROWS: usize = 300;
/// Ids a read of the index answers at most, within the SQLite adapter's
/// 100,000 rows a query.
const READ: usize = 10_000;

const SCHEMA: [&str; 3] = [
    "CREATE TABLE IF NOT EXISTS photo (view TEXT NOT NULL, id TEXT NOT NULL, created REAL, PRIMARY KEY (view, id)) WITHOUT ROWID",
    "CREATE TABLE IF NOT EXISTS staged (view TEXT NOT NULL, id TEXT NOT NULL, created REAL, PRIMARY KEY (view, id)) WITHOUT ROWID",
    "CREATE TABLE IF NOT EXISTS checkpoint (view TEXT PRIMARY KEY, token TEXT NOT NULL)",
];

/// Changes to apply: each upserted id's creation date, or `None` for a
/// removal.
type Changes = BTreeMap<String, Option<Option<f64>>>;

/// Where a sync is: the request it waits on.
enum Step {
    /// The stored checkpoint.
    Load,
    /// A walk's page from the index's checkpoint.
    Walk,
    /// That page's transaction; the next page's token when there is one.
    Commit(Option<String>),
    /// The checkpoint a replacement starts from.
    Checkpoint,
    /// A page of the list.
    List,
    /// That page's rows staged; the next page's cursor when there is one.
    Stage(Option<String>),
    /// A page of the walk from the replacement's checkpoint.
    CatchUp,
    /// The replacement's transaction.
    Publish,
    /// A page of the committed index, read to answer.
    Summary,
}

/// One sync of one view's index.
struct Sync {
    view: &'static str,
    subtypes: Vec<String>,
    limit: f64,
    budget: u32,
    step: Step,
    /// Pages of the module read in this sync.
    pages: u32,
    /// `walked`, `replaced`, `interrupted` (a replacement the budget
    /// stopped) or `failed`.
    how: &'static str,
    /// A walk the budget stopped: the next sync goes on from its last page.
    more: bool,
    /// The code of the read that failed; the committed index stands.
    error: String,
    /// The replacement's checkpoint, whether its list has staged a page, and
    /// the changes of the walk from the checkpoint.
    fresh: String,
    staging: bool,
    caught: Changes,
    /// The index as read so far for the answer.
    read: Digest,
}

/// The sync and the listing in flight, each by its resource's arguments, and
/// how many syncs began. The app has one index resource and one listing, so
/// a new one supersedes any other: the runner drops a superseded request.
#[derive(Default)]
pub(crate) struct Index {
    sync: Option<(String, Sync)>,
    began: u32,
    listing: Option<(String, Vec<String>)>,
}

fn key(source: &str, args: &[Value]) -> String {
    format!("{source}{args:?}")
}

fn view_of(args: &[Value]) -> (&'static str, Vec<String>) {
    match args.first() {
        Some(Value::Bool(true)) => ("screenshot", vec!["screenshot".into()]),
        _ => ("all", Vec::new()),
    }
}

fn sql(commands: Json) -> Answer {
    Answer::Later(storage::request(
        "sqlite",
        json!({ "path": DB, "commands": commands }),
    ))
}

fn transaction(commands: Vec<Json>) -> Answer {
    Answer::Later(storage::request(
        "sqlite.transaction",
        json!({ "path": DB, "commands": commands }),
    ))
}

fn execute(sql: String, params: Vec<Json>) -> Json {
    json!({ "kind": "execute", "sql": sql, "params": params })
}

/// Upserts of `photos` into `table`'s `view`, a statement per [`ROWS`].
fn upserts<'a>(
    table: &str,
    view: &str,
    photos: impl Iterator<Item = (&'a String, &'a Option<f64>)>,
) -> Vec<Json> {
    let rows: Vec<_> = photos.collect();
    rows.chunks(ROWS)
        .map(|chunk| {
            let values = vec!["(?, ?, ?)"; chunk.len()].join(", ");
            let params = chunk
                .iter()
                .flat_map(|(id, created)| [json!(view), json!(id), json!(created)])
                .collect();
            execute(
                format!("INSERT INTO {table} (view, id, created) VALUES {values} ON CONFLICT (view, id) DO UPDATE SET created = excluded.created"),
                params,
            )
        })
        .collect()
}

/// Removals of `ids` from `view`, a statement per [`ROWS`].
fn removals<'a>(view: &str, ids: impl Iterator<Item = &'a String>) -> Vec<Json> {
    let ids: Vec<_> = ids.collect();
    ids.chunks(ROWS)
        .map(|chunk| {
            let marks = vec!["?"; chunk.len()].join(", ");
            let params = std::iter::once(json!(view)).chain(chunk.iter().map(|id| json!(id)));
            execute(
                format!("DELETE FROM photo WHERE view = ? AND id IN ({marks})"),
                params.collect(),
            )
        })
        .collect()
}

fn checkpoint(view: &str, token: &str) -> Json {
    execute(
        "INSERT INTO checkpoint (view, token) VALUES (?, ?) ON CONFLICT (view) DO UPDATE SET token = excluded.token".into(),
        vec![json!(view), json!(token)],
    )
}

/// How many ids, and a digest of them that is the same for the index and the
/// listing when they hold the same photos: FNV-1a over the ids in order, each
/// ended by a newline.
#[derive(Clone, Copy)]
struct Digest {
    count: usize,
    hash: u64,
}

impl Default for Digest {
    fn default() -> Self {
        Self {
            count: 0,
            hash: 0xcbf2_9ce4_8422_2325,
        }
    }
}

impl Digest {
    fn add(&mut self, id: &str) {
        self.count += 1;
        for byte in id.bytes().chain(std::iter::once(b'\n')) {
            self.hash = (self.hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }

    fn text(&self) -> String {
        format!("{:016x}", self.hash)
    }
}

impl Sync {
    fn changes(&mut self, since: Option<String>) -> Answer {
        self.pages += 1;
        let options = photos::PhotoChangesOptions {
            since,
            limit: Some(self.limit),
            subtypes: Some(self.subtypes.clone()),
        };
        module(photos::photo_changes(&options))
    }

    /// Whether the budget allows another page.
    fn spent(&self) -> bool {
        self.budget != 0 && self.pages >= self.budget
    }

    /// A page of the committed index after `after`, read to answer: after an
    /// error, the code stays.
    fn summary(&mut self, after: &str) -> Answer {
        self.step = Step::Summary;
        sql(json!([{
            "kind": "query",
            "sql": "SELECT id FROM photo WHERE view = ? AND id > ? ORDER BY id LIMIT ?",
            "params": [self.view, after, READ],
        }]))
    }

    fn fail(&mut self, code: &str) -> Answer {
        self.how = "failed";
        self.error = code.into();
        self.summary("")
    }

    /// A walk's page from `since`, or the end of the sync if the budget is spent.
    fn walk(&mut self, since: String) -> Answer {
        if self.spent() {
            self.more = true;
            return self.summary("");
        }
        self.step = Step::Walk;
        self.changes(Some(since))
    }

    /// Step 2: a replacement, from `fresh`, the `expired` answer's token, or
    /// from a checkpoint taken now.
    fn replace(&mut self, fresh: Option<String>) -> Answer {
        self.how = "replaced";
        match fresh {
            Some(token) => {
                self.fresh = token;
                self.list(None)
            }
            None if self.spent() => self.interrupted(),
            None => {
                self.step = Step::Checkpoint;
                self.changes(None)
            }
        }
    }

    fn list(&mut self, cursor: Option<String>) -> Answer {
        if self.spent() {
            return self.interrupted();
        }
        self.step = Step::List;
        self.pages += 1;
        module(photos::photo_assets(&photos::PhotoAssetsOptions {
            cursor,
            limit: Some(PAGE),
            subtypes: Some(self.subtypes.clone()),
        }))
    }

    /// A replacement the budget stopped: discarded, the index as it was. Its
    /// staged rows are cleared when the next replacement stages its first page.
    fn interrupted(&mut self) -> Answer {
        self.how = "interrupted";
        self.summary("")
    }

    /// Step 3's page: the walk from the replacement's checkpoint.
    fn catch_up(&mut self, since: String) -> Answer {
        if self.spent() {
            return self.interrupted();
        }
        self.step = Step::CatchUp;
        self.changes(Some(since))
    }

    /// The staged list, the walk's changes and `token` in place of the index,
    /// in one transaction.
    fn publish(&mut self, token: String) -> Answer {
        self.step = Step::Publish;
        let view = json!(self.view);
        let mut commands = vec![
            execute("DELETE FROM photo WHERE view = ?".into(), vec![view.clone()]),
            execute(
                "INSERT INTO photo (view, id, created) SELECT view, id, created FROM staged WHERE view = ?".into(),
                vec![view.clone()],
            ),
        ];
        let caught = std::mem::take(&mut self.caught);
        let upserted = caught
            .iter()
            .filter_map(|(id, c)| c.as_ref().map(|c| (id, c)));
        commands.extend(upserts("photo", self.view, upserted));
        let removed = caught.iter().filter(|(_, c)| c.is_none()).map(|(id, _)| id);
        commands.extend(removals(self.view, removed));
        commands.push(execute(
            "DELETE FROM staged WHERE view = ?".into(),
            vec![view],
        ));
        commands.push(checkpoint(self.view, &token));
        transaction(commands)
    }

    /// The next request, or the answer, after `outcome`.
    fn step(&mut self, outcome: Outcome) -> Answer {
        match std::mem::replace(&mut self.step, Step::Load) {
            Step::Load => match stored(outcome) {
                Ok(Some(token)) => {
                    self.how = "walked";
                    self.walk(token)
                }
                Ok(None) => self.replace(None),
                Err(code) => self.fail(&code),
            },
            Step::Walk => match photos::photo_changes_reply(&outcome) {
                // A token the module never issued: the index is replaced.
                Err(e) if e.code == "SyntaxError" => self.replace(None),
                Err(e) => self.fail(&e.code),
                Ok(page) if page.expired => self.replace(Some(page.token)),
                Ok(page) => {
                    self.step = Step::Commit(page.more.then(|| page.token.clone()));
                    let changed = page.added.iter().chain(&page.updated);
                    let mut commands = upserts(
                        "photo",
                        self.view,
                        changed.map(|a| (&a.id, &a.creation_date)),
                    );
                    commands.extend(removals(self.view, page.deleted.iter()));
                    commands.push(checkpoint(self.view, &page.token));
                    transaction(commands)
                }
            },
            Step::Commit(next) => match (storage::response(outcome), next) {
                (Err(_), _) => self.fail("UnknownError"),
                (Ok(_), Some(token)) => self.walk(token),
                (Ok(_), None) => self.summary(""),
            },
            Step::Checkpoint => match photos::photo_changes_reply(&outcome) {
                Ok(page) => self.replace(Some(page.token)),
                Err(e) => self.fail(&e.code),
            },
            Step::List => match photos::photo_assets_reply(&outcome) {
                Ok(page) => {
                    self.step = Step::Stage(page.cursor);
                    let mut commands = Vec::new();
                    // The first page clears what an earlier replacement left.
                    if !std::mem::replace(&mut self.staging, true) {
                        commands.push(execute(
                            "DELETE FROM staged WHERE view = ?".into(),
                            vec![json!(self.view)],
                        ));
                    }
                    let rows = page.assets.iter().map(|a| (&a.id, &a.creation_date));
                    commands.extend(upserts("staged", self.view, rows));
                    transaction(commands)
                }
                Err(e) => self.fail(&e.code),
            },
            Step::Stage(cursor) => match (storage::response(outcome), cursor) {
                (Err(_), _) => self.fail("UnknownError"),
                (Ok(_), Some(cursor)) => self.list(Some(cursor)),
                (Ok(_), None) => {
                    let fresh = self.fresh.clone();
                    self.catch_up(fresh)
                }
            },
            Step::CatchUp => match photos::photo_changes_reply(&outcome) {
                // The checkpoint cannot supply a delta: the list stands, with it.
                Ok(page) if page.expired => {
                    self.caught.clear();
                    let fresh = std::mem::take(&mut self.fresh);
                    self.publish(fresh)
                }
                Ok(page) => {
                    for asset in page.added.iter().chain(&page.updated) {
                        self.caught
                            .insert(asset.id.clone(), Some(asset.creation_date));
                    }
                    for id in page.deleted {
                        self.caught.insert(id, None);
                    }
                    match page.more {
                        true => self.catch_up(page.token),
                        false => self.publish(page.token),
                    }
                }
                Err(e) => self.fail(&e.code),
            },
            Step::Publish => match storage::response(outcome) {
                Ok(_) => self.summary(""),
                Err(_) => self.fail("UnknownError"),
            },
            Step::Summary => unreachable!("the index's reader answers"),
        }
    }

    fn answer(&self, syncs: u32) -> Value {
        Value::record(vec![
            Value::Number(self.read.count as f64),
            Value::str(&self.read.text()),
            Value::str(self.how),
            Value::Number(self.pages.into()),
            Value::Bool(self.more),
            Value::Number(syncs.into()),
            Value::str(&self.error),
        ])
    }
}

/// A module request.
fn module(request: Result<Request, ModuleError>) -> Answer {
    // The options are the binding's own records, so encoding cannot fail.
    Answer::Later(request.expect("photos options encode"))
}

/// The stored checkpoint, after the schema: `None` for an index never filled.
fn stored(outcome: Outcome) -> Result<Option<String>, String> {
    let results = storage::response(outcome).map_err(|_| "UnknownError".to_string())?;
    let rows = &results[SCHEMA.len()]["rows"];
    Ok(rows[0][0].as_str().map(str::to_owned))
}

/// The ids a page of the index's read answered.
fn ids(outcome: Outcome) -> Result<Vec<String>, String> {
    let results = storage::response(outcome)?;
    let rows = results[0]["rows"].as_array().cloned().unwrap_or_default();
    Ok(rows
        .iter()
        .filter_map(|row| row[0].as_str().map(str::to_owned))
        .collect())
}

impl Index {
    /// `photoIndex(screenshots, limit, budget, access)`: a sync begins with
    /// the stored checkpoint.
    pub(crate) fn sync(&mut self, args: &[Value]) -> Answer {
        let (view, subtypes) = view_of(args);
        let number = |i: usize| args.get(i).and_then(Value::as_number).unwrap_or(0.0);
        let limit = match number(1) {
            n if n >= 1.0 => n.min(PAGE),
            _ => PAGE,
        };
        self.began += 1;
        let mut commands: Vec<Json> = SCHEMA
            .iter()
            .map(|s| execute((*s).into(), vec![]))
            .collect();
        commands.push(json!({
            "kind": "query",
            "sql": "SELECT token FROM checkpoint WHERE view = ?",
            "params": [view],
        }));
        let sync = Sync {
            view,
            subtypes,
            limit,
            budget: number(2).max(0.0) as u32,
            step: Step::Load,
            pages: 0,
            how: "walked",
            more: false,
            error: String::new(),
            fresh: String::new(),
            staging: false,
            caught: Changes::new(),
            read: Digest::default(),
        };
        self.sync = Some((key("photoIndex", args), sync));
        sql(Json::Array(commands))
    }

    /// A reply in a sync: the next request, or the index's answer.
    pub(crate) fn synced(&mut self, args: &[Value], outcome: Outcome) -> Answer {
        let sync = match &mut self.sync {
            Some((key, sync)) if *key == self::key("photoIndex", args) => sync,
            _ => return Answer::Now(empty_index("AbortError")),
        };
        if !matches!(sync.step, Step::Summary) {
            return sync.step(outcome);
        }
        match ids(outcome) {
            Ok(ids) => {
                ids.iter().for_each(|id| sync.read.add(id));
                if ids.len() == READ {
                    return sync.summary(ids.last().expect("a full page"));
                }
                let answer = sync.answer(self.began);
                self.sync = None;
                Answer::Now(answer)
            }
            Err(_) => {
                self.sync = None;
                Answer::Now(empty_index("UnknownError"))
            }
        }
    }

    /// `photoListing(screenshots)`: every page of `photoAssets`.
    pub(crate) fn listing(&mut self, args: &[Value]) -> Answer {
        self.listing = Some((key("photoListing", args), Vec::new()));
        list_page(&view_of(args).1, None)
    }

    /// A page of the listing: the next, or its answer.
    pub(crate) fn listed(&mut self, args: &[Value], outcome: Outcome) -> Answer {
        let ids = match &mut self.listing {
            Some((key, ids)) if *key == self::key("photoListing", args) => ids,
            _ => return Answer::Now(listing(&[], "AbortError")),
        };
        match photos::photo_assets_reply(&outcome) {
            Ok(page) => {
                ids.extend(page.assets.into_iter().map(|a| a.id));
                if let Some(cursor) = page.cursor {
                    return list_page(&view_of(args).1, Some(cursor));
                }
                let (_, mut ids) = self.listing.take().unwrap_or_default();
                ids.sort();
                Answer::Now(listing(&ids, ""))
            }
            Err(e) => {
                self.listing = None;
                Answer::Now(listing(&[], &e.code))
            }
        }
    }
}

fn list_page(subtypes: &[String], cursor: Option<String>) -> Answer {
    module(photos::photo_assets(&photos::PhotoAssetsOptions {
        cursor,
        limit: Some(PAGE),
        subtypes: Some(subtypes.to_vec()),
    }))
}

fn listing(ids: &[String], error: &str) -> Value {
    let mut read = Digest::default();
    ids.iter().for_each(|id| read.add(id));
    Value::record(vec![
        Value::Number(read.count as f64),
        Value::str(&read.text()),
        Value::str(error),
    ])
}

fn empty_index(error: &str) -> Value {
    Value::record(vec![
        Value::Number(0.0),
        Value::str(&Digest::default().text()),
        Value::str(""),
        Value::Number(0.0),
        Value::Bool(false),
        Value::Number(0.0),
        Value::str(error),
    ])
}
