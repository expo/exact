//! Heavy list data: the 10,000 messages of `data/messages.json` (the copy of
//! `bench/heavy-list/data/messages.json` that `../prepare.sh` writes; bundled
//! into the binary), the reaction counts the app has changed, and live mode
//! (`BENCH_LIVE=1` in the launch environment).
//!
//! - `messages()` answers the feed (baked into the plan at build);
//! - `react(id, emoji)` adds one to that message's reaction and answers the feed;
//! - `config()` answers `{ live, motion }` from the launch environment;
//! - `tick()` (live mode) inserts message `live-k` at the top — the content of
//!   message `(k*37) % 10000`, `minutesAgo` 0 — bumps one reaction on message
//!   `(k*101) % 10000` (the first at or after it that has reactions), and
//!   answers the feed.
//!
//! Record sharing (LLP 1053 §0, G8): every answer is a fresh list, but its
//! items are the previous answer's `Rc` records for every message that did
//! not change, so the runner's identity checks (`compare::same`) hold for
//! them. A tick builds exactly two records: the inserted `live-k` (which
//! shares its copied content's fields) and the bumped message (which shares
//! every field but its reactions).
#![forbid(unsafe_code)]



use exact_plan::Value;
use exact_runner::{DataError, DataSource};

/// The benchmark's data, as the build found it.
pub const JSON: &[u8] = include_bytes!("../messages.json");

/// One message's content, parsed once.
struct Message {
    id: String,
    author: String,
    avatar: String,
    minutes_ago: f64,
    paragraphs: Value,
    photos: Value,
    link: Value,
    quote: Value,
    reactions: Vec<(String, f64)>,
}

// The JSON parses straight into these typed rows on a background thread, so
// the source never holds a generic `serde_json::Value` tree (~86 MB here).
#[derive(serde::Deserialize)]
struct Doc {
    messages: Vec<Raw>,
}
#[derive(serde::Deserialize)]
struct Raw {
    id: String,
    author: String,
    avatar: String,
    #[serde(rename = "minutesAgo")]
    minutes_ago: f64,
    paragraphs: Vec<Vec<RawRun>>,
    #[serde(default)]
    photos: Vec<RawPhoto>,
    link: Option<RawLink>,
    quote: Option<RawQuote>,
    #[serde(default)]
    reactions: Vec<RawReaction>,
}
#[derive(serde::Deserialize)]
struct RawRun {
    t: String,
    #[serde(default)]
    s: String,
}
#[derive(serde::Deserialize)]
struct RawPhoto {
    src: String,
    w: f64,
    h: f64,
}
#[derive(serde::Deserialize)]
struct RawLink {
    thumb: String,
    title: String,
    description: String,
    site: String,
}
#[derive(serde::Deserialize)]
struct RawQuote {
    author: String,
    excerpt: String,
}
#[derive(serde::Deserialize)]
struct RawReaction {
    emoji: String,
    count: f64,
}

fn some(v: Value) -> Value {
    Value::Option(Some(std::rc::Rc::new(v)))
}

impl Message {
    fn parse(m: Raw) -> Message {
        // Field order is `shape Para` / `shape Run` in app.contract.
        let paragraphs = m
            .paragraphs
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let runs = p
                    .iter()
                    .enumerate()
                    .map(|(j, r)| Value::record(vec![Value::str(&j.to_string()), Value::str(&r.t), Value::str(&r.s)]))
                    .collect();
                Value::record(vec![Value::str(&i.to_string()), Value::list(runs)])
            })
            .collect();
        let photos = match m.photos.first() {
            Some(first) => {
                let at = |i: usize| m.photos.get(i).map(|p| p.src.clone()).unwrap_or_default();
                // `aspect-ratio` is width / height.
                some(Value::record(vec![
                    Value::Number(m.photos.len() as f64),
                    Value::str(&at(0)),
                    Value::str(&at(1)),
                    Value::str(&at(2)),
                    Value::str(&at(3)),
                    Value::Number(first.w / first.h),
                ]))
            }
            None => Value::Option(None),
        };
        // Contract has no `text-transform`; the site line is upper-cased here.
        let link = match &m.link {
            Some(l) => some(Value::record(vec![
                Value::str(&l.thumb),
                Value::str(&l.title),
                Value::str(&l.description),
                Value::str(&l.site.to_uppercase()),
            ])),
            None => Value::Option(None),
        };
        let quote = match &m.quote {
            Some(q) => some(Value::record(vec![Value::str(&q.author), Value::str(&q.excerpt)])),
            None => Value::Option(None),
        };
        let reactions = m.reactions.into_iter().map(|r| (r.emoji, r.count)).collect();
        Message {
            id: m.id,
            author: m.author,
            avatar: m.avatar,
            minutes_ago: m.minutes_ago,
            paragraphs: Value::list(paragraphs),
            photos,
            link,
            quote,
            reactions,
        }
    }

    /// One row. Field order is `shape Message` in app.contract.
    fn value(&self) -> Value {
        Value::record(vec![
            Value::str(&self.id),
            Value::str(&self.author),
            Value::str(&self.avatar),
            Value::Number(self.minutes_ago),
            self.paragraphs.clone(),
            self.photos.clone(),
            self.link.clone(),
            self.quote.clone(),
            self.reactions_value(),
        ])
    }

    fn reactions_value(&self) -> Value {
        Value::list(
            self.reactions
                .iter()
                .map(|(e, c)| Value::record(vec![Value::str(e), Value::Number(*c)]))
                .collect(),
        )
    }
}

/// `row`'s fields (a `shape Message` record), shared.
fn fields(row: &Value) -> &[Value] {
    match row {
        Value::Record(f) => f,
        _ => unreachable!("rows are this source's own records"),
    }
}

/// `row` with field `at` replaced; every other field keeps its allocation.
fn with(row: &Value, changes: &[(usize, Value)]) -> Value {
    let mut f = fields(row).to_vec();
    for (at, v) in changes {
        f[*at] = v.clone();
    }
    Value::record(f)
}

/// `shape Message` field indices.
const ID: usize = 0;
const MINUTES_AGO: usize = 3;
const REACTIONS: usize = 8;

/// The feed: live messages (newest first) above the 10,000. `rows` are their
/// row values as the feed shows them, so an answer copies one slice into its
/// list (`Items::from`) with no vector of its own.
struct Feed {
    messages: Vec<Message>,
    rows: Vec<Value>,
    /// In the order they came: the `j`th is `rows[live.len() - 1 - j]`.
    live: Vec<Message>,
    ticks: usize,
}

impl Feed {
    fn load(doc: Doc) -> Feed {
        let messages: Vec<Message> = doc.messages.into_iter().map(Message::parse).collect();
        let rows = messages.iter().map(Message::value).collect();
        Feed { messages, rows, live: Vec::new(), ticks: 0 }
    }

    fn value(&self) -> Value {
        Value::record(vec![Value::List(exact_plan::Items::from(self.rows.as_slice()))])
    }

    fn react(&mut self, id: &str, emoji: &str) -> bool {
        let bump = |m: &mut Message| match m.reactions.iter_mut().find(|(e, _)| e == emoji) {
            Some((_, c)) => {
                *c += 1.0;
                true
            }
            None => false,
        };
        if let Some(i) = id.strip_prefix('m').and_then(|n| n.parse::<usize>().ok()).filter(|&i| i < self.messages.len()) {
            if self.messages[i].id == id && bump(&mut self.messages[i]) {
                let at = self.live.len() + i;
                self.rows[at] = with(&self.rows[at], &[(REACTIONS, self.messages[i].reactions_value())]);
                return true;
            }
            return false;
        }
        if let Some(j) = self.live.iter().position(|m| m.id == id) {
            if bump(&mut self.live[j]) {
                let at = self.live.len() - 1 - j;
                self.rows[at] = with(&self.rows[at], &[(REACTIONS, self.live[j].reactions_value())]);
                return true;
            }
        }
        false
    }

    fn tick(&mut self) {
        let k = self.ticks + 1;
        self.ticks = k;
        let n = self.messages.len();
        let from_at = (k * 37) % n;
        let from = &self.messages[from_at];
        let copy = Message {
            id: format!("live-{k}"),
            author: from.author.clone(),
            avatar: from.avatar.clone(),
            minutes_ago: 0.0,
            paragraphs: from.paragraphs.clone(),
            photos: from.photos.clone(),
            link: from.link.clone(),
            quote: from.quote.clone(),
            reactions: from.reactions.clone(),
        };
        // The copy shares its source row's fields but the id and the time.
        let source = &self.rows[self.live.len() + from_at];
        let v = with(source, &[(ID, Value::str(&copy.id)), (MINUTES_AGO, Value::Number(0.0))]);
        self.live.push(copy);
        self.rows.insert(0, v);
        let start = (k * 101) % n;
        if let Some(i) = (0..n).map(|d| (start + d) % n).find(|&i| !self.messages[i].reactions.is_empty()) {
            let m = &mut self.messages[i];
            let r = k % m.reactions.len();
            m.reactions[r].1 += 1.0;
            let at = self.live.len() + i;
            self.rows[at] = with(&self.rows[at], &[(REACTIONS, m.reactions_value())]);
        }
    }
}

/// The source. On a device the feed the list first shows is the plan's
/// baked value; the source's own copy (what a reaction or a live tick edits)
/// is parsed and turned into row values on first use. Only live mode, whose
/// first tick uses it, parses at launch, on a background thread. Otherwise
/// the first reaction parses it (~40 ms on a phone), since one parsed at
/// launch is a second copy of the feed (18 MB on Android) while the app rests.
pub struct Heavy {
    parsed: Option<std::thread::JoinHandle<Doc>>,
    feed: Option<Feed>,
}

impl Default for Heavy {
    fn default() -> Self {
        // wasm32 has no threads: the web module parses on first use instead.
        #[cfg(target_arch = "wasm32")]
        let parsed = None;
        #[cfg(not(target_arch = "wasm32"))]
        let parsed = live().then(|| std::thread::spawn(|| serde_json::from_slice::<Doc>(JSON).expect("messages.json")));
        Heavy { parsed, feed: None }
    }
}

/// Live mode: `BENCH_LIVE=1` in the launch environment.
fn live() -> bool {
    std::env::var("BENCH_LIVE").is_ok_and(|v| v == "1")
}

impl Heavy {
    fn feed(&mut self) -> &mut Feed {
        if self.feed.is_none() {
            let doc = match self.parsed.take() {
                Some(thread) => thread.join().expect("parse thread"),
                None => serde_json::from_slice::<Doc>(JSON).expect("messages.json"),
            };
            self.feed = Some(Feed::load(doc));
        }
        self.feed.as_mut().expect("loaded")
    }
}

impl DataSource for Heavy {
    fn bind(&mut self, _plan: &exact_plan::Plan) {
        // This source uses baked values and has no binding work.
        #[cfg(target_os = "macos")]
        exact_ink::device::apple::trace_mark(c"boot data bound", || 0.);
    }

    fn app_id(&self) -> &str {
        "dev.exact.heavybench.exact"
    }

    fn query(&mut self, source: &str, args: &[Value]) -> Result<Value, DataError> {
        match (source, args) {
            ("messages", []) => Ok(self.feed().value()),
            ("react", [id, emoji]) => {
                let (Some(id), Some(emoji)) = (id.as_str(), emoji.as_str()) else {
                    return Err(DataError::BadArguments("react(id: string, emoji: string)".into()));
                };
                let feed = self.feed();
                feed.react(id, emoji);
                Ok(feed.value())
            }
            ("config", []) => {
                let live = live();
                let motion = std::env::var("BENCH_MOTION").unwrap_or_default();
                if !matches!(motion.as_str(), "" | "bar" | "caret") {
                    return Err(DataError::BadArguments("BENCH_MOTION must be bar or caret".into()));
                }
                if live {
                    // Parse now, not on the first live tick.
                    self.feed();
                }
                Ok(Value::record(vec![Value::Bool(live), Value::str(&motion)]))
            }
            ("tick", []) => {
                let feed = self.feed();
                feed.tick();
                Ok(feed.value())
            }
            ("messages" | "config" | "tick", _) => Err(DataError::BadArguments(format!("{source}()"))),
            ("react", _) => Err(DataError::BadArguments("react(id, emoji)".into())),
            _ => Err(DataError::UnknownSource(source.into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(v: &Value) -> &[Value] {
        let Value::List(list) = v else { panic!("expected a list") };
        list
    }

    fn rows(v: &Value) -> &[Value] {
        items(&fields(v)[0])
    }

    fn changed_ids<'a>(before: &[Value], after: &'a [Value]) -> Vec<&'a str> {
        assert_eq!(before.len(), after.len());
        before.iter().zip(after).filter(|(a, b)| !exact_runner::compare::same(a, b))
            .map(|(_, row)| fields(row)[ID].as_str().unwrap()).collect()
    }

    #[test]
    fn feed_live_and_reactions() {
        let mut h = Heavy::default();
        assert_eq!(h.parsed.is_some(), live(), "only live mode parses before the first use");
        let config = h.query("config", &[]).unwrap();
        assert_eq!(fields(&config)[0].as_bool(), Some(live()));
        assert_eq!(h.feed.is_some(), live(), "only live config loads the feed");
        let t = std::time::Instant::now();
        let v = h.query("messages", &[]).unwrap();
        eprintln!("load: {:?}", t.elapsed());
        assert_eq!(rows(&v).len(), 10_000);
        let t = std::time::Instant::now();
        let v = h.query("tick", &[]).unwrap();
        eprintln!("tick: {:?}", t.elapsed());
        assert_eq!(rows(&v).len(), 10_001);
        let f = h.feed.as_ref().unwrap();
        assert_eq!(f.live[0].id, "live-1");
        assert_eq!(f.live[0].author, f.messages[37].author);
        let mut before = h.query("tick", &[]).unwrap();
        let f = h.feed.as_ref().unwrap();
        let bumped = f.messages[2 * 101..].iter().find(|m| !m.reactions.is_empty()).unwrap();
        assert_eq!(changed_ids(rows(&v), &rows(&before)[1..]), [bumped.id.as_str()]);

        let original = f.messages.iter().find(|m| !m.reactions.is_empty()).unwrap();
        let taps = [original, &f.live[0]].map(|m| (m.id.clone(), m.reactions[0].clone()));
        for (id, (emoji, count)) in taps {
            let after = h.query("react", &[Value::str(&id), Value::str(&emoji)]).unwrap();
            assert_eq!(changed_ids(rows(&before), rows(&after)), [id.as_str()]);
            let shown = rows(&after).iter().find(|row| fields(row)[ID].as_str() == Some(&id)).unwrap();
            let reaction = &items(&fields(shown)[REACTIONS])[0];
            assert_eq!(fields(reaction)[0].as_str(), Some(emoji.as_str()));
            assert_eq!(fields(reaction)[1].as_number(), Some(count + 1.0));
            before = after;
        }
    }
}
