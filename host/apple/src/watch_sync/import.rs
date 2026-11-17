//! The watch's import of a phone snapshot, and the phone's revisions, as pure
//! decisions. `decide` and `retire` answer the steps of one import in order;
//! `watch_sync.rs` performs them and stops at the first that fails, so every
//! prefix is a state the next attempt, in this process or another, starts from.
//!
//! Values live in the Keychain and in preferences, with no transaction across
//! them. The journal brackets the writes instead: a reader refuses the store
//! while one is recorded, and the generation tells a reader that an import
//! finished inside its read (`coherent`).
use serde_json::{json, Value};

/// How many replaced phones' identities are remembered.
pub(crate) const RETIRED: usize = 32;

/// The last finished import.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct Record {
    pub sender: String,
    pub revision: u64,
    /// The names it wrote, sorted: what a later import or retirement may delete.
    pub names: Vec<String>,
    /// Replaced phones, oldest first. Their snapshots are never imported.
    pub retired: Vec<String>,
}

/// An import that began. A retirement names no sender.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct Journal {
    pub sender: Option<String>,
    pub revision: u64,
    /// Every name the import writes or deletes, sorted.
    pub names: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct Meta {
    pub received: Option<Record>,
    pub journal: Option<Journal>,
}

/// A validated snapshot's identity.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Incoming {
    pub sender: String,
    pub revision: u64,
    /// The names it carries that this binary selects, sorted.
    pub names: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Step {
    Journal(Journal),
    /// Expire what other processes cached from the store (complication timelines).
    Invalidate,
    /// Write the snapshot's value for this name; an absent value deletes it.
    Write(String),
    /// Delete a name an earlier import wrote and this one does not carry.
    Delete(String),
    Record(Record),
    Generation,
    Clear,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    /// Nothing to write, and no import is unfinished.
    Current,
    /// An unfinished import stays owed, and this snapshot cannot finish it.
    Blocked,
    Import(Vec<Step>),
}

/// The steps of a journalled change. Caches expire before the first value
/// changes, so a reader in another process cannot serve the old account once any
/// of it is gone, and again before the journal clears, so nothing cached during
/// the import outlives it.
fn steps(journal: Journal, values: Vec<Step>, record: Option<Record>) -> Vec<Step> {
    let mut steps = vec![Step::Journal(journal), Step::Invalidate];
    steps.extend(values);
    steps.extend(record.map(Step::Record));
    steps.extend([Step::Generation, Step::Invalidate, Step::Clear]);
    steps
}

fn merged<'a>(lists: impl IntoIterator<Item = &'a Vec<String>>) -> Vec<String> {
    let mut names: Vec<String> = lists.into_iter().flatten().cloned().collect();
    names.sort();
    names.dedup();
    names
}

/// What to do with a snapshot. Only a finished store answers `Current`: while a
/// journal is recorded the answer is an import that finishes it, or `Blocked`.
pub(crate) fn decide(meta: &Meta, incoming: &Incoming) -> Decision {
    let old = meta.received.as_ref();
    let journal = meta.journal.as_ref();
    let unfinished = if journal.is_some() {
        Decision::Blocked
    } else {
        Decision::Current
    };
    if journal.is_some_and(|j| {
        j.sender.as_deref() == Some(&incoming.sender) && j.revision > incoming.revision
    }) {
        return Decision::Blocked;
    }
    if old.is_some_and(|r| r.retired.contains(&incoming.sender)) {
        return unfinished;
    }
    if let Some(r) = old.filter(|r| r.sender == incoming.sender) {
        // The same revision is imported again when this binary selects other
        // names of it than the last import wrote, or to finish a journal.
        if r.revision > incoming.revision
            || (journal.is_none() && r.revision == incoming.revision && r.names == incoming.names)
        {
            return unfinished;
        }
    }
    let known = merged(
        old.map(|r| &r.names)
            .into_iter()
            .chain(journal.map(|j| &j.names)),
    );
    let mut values: Vec<Step> = incoming.names.iter().cloned().map(Step::Write).collect();
    values.extend(
        known
            .iter()
            .filter(|n| !incoming.names.contains(n))
            .cloned()
            .map(Step::Delete),
    );
    let mut retired = old.map(|r| r.retired.clone()).unwrap_or_default();
    if let Some(previous) = old.filter(|r| r.sender != incoming.sender) {
        if retired.len() >= RETIRED {
            retired.remove(0);
        }
        retired.push(previous.sender.clone());
    }
    Decision::Import(steps(
        Journal {
            sender: Some(incoming.sender.clone()),
            revision: incoming.revision,
            names: merged([&incoming.names, &known]),
        },
        values,
        Some(Record {
            sender: incoming.sender.clone(),
            revision: incoming.revision,
            names: incoming.names.clone(),
            retired,
        }),
    ))
}

/// Delete the replicas this binary no longer selects. With a selection left,
/// an unfinished import is the next snapshot's to finish.
pub(crate) fn retire(meta: &Meta, allowed: &[String]) -> Decision {
    let known = merged(
        meta.received
            .as_ref()
            .map(|r| &r.names)
            .into_iter()
            .chain(meta.journal.as_ref().map(|j| &j.names)),
    );
    let removed: Vec<String> = known
        .iter()
        .filter(|n| !allowed.contains(n))
        .cloned()
        .collect();
    if meta.journal.is_some() && !allowed.is_empty() {
        return Decision::Blocked;
    }
    if removed.is_empty() && meta.journal.is_none() {
        return Decision::Current;
    }
    let record = meta.received.clone().map(|mut r| {
        r.names.retain(|n| allowed.contains(n));
        r
    });
    Decision::Import(steps(
        Journal {
            sender: None,
            revision: 0,
            names: removed.clone(),
        },
        removed.into_iter().map(Step::Delete).collect(),
        record,
    ))
}

/// What a reader samples before its first read and after its last.
#[cfg(any(target_os = "watchos", test))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Mark {
    pub journal: bool,
    pub generation: u64,
}

/// Whether values read between two marks are one finished import's. An import
/// that starts and finishes between two reads leaves both journals clear, so the
/// generation is compared too.
#[cfg(any(target_os = "watchos", test))]
pub(crate) fn coherent(before: Option<&Mark>, after: Option<&Mark>) -> bool {
    matches!((before, after), (Some(a), Some(b)) if !a.journal && !b.journal && a.generation == b.generation)
}

/// The phone's identity and the last revision it spent.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Sent {
    pub sender: String,
    pub revision: u64,
}

/// The identity and revision the next snapshot carries. Nothing is spent yet.
pub(crate) fn candidate(sent: Option<&Sent>, proposed: &str) -> Option<Sent> {
    Some(Sent {
        sender: sent.map_or(proposed, |s| &s.sender).to_owned(),
        revision: sent.map_or(0, |s| s.revision).checked_add(1)?,
    })
}

/// Spend `next` when it is still the candidate: the snapshot it numbers fits
/// the transport and is about to be handed over.
pub(crate) fn reserve(sent: Option<&Sent>, next: &Sent) -> Option<Sent> {
    (candidate(sent, &next.sender).as_ref() == Some(next)).then(|| next.clone())
}

/// Give `revision` back when the transport refused its snapshot, so no watch has
/// seen it, and it is the last one spent.
pub(crate) fn release(sent: &Sent, revision: u64) -> Option<Sent> {
    (sent.revision == revision && revision > 0).then(|| Sent {
        sender: sent.sender.clone(),
        revision: revision - 1,
    })
}

fn strings(value: &Value) -> Vec<String> {
    let mut names: Vec<String> = value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    names.sort();
    names.dedup();
    names
}

impl Record {
    pub(crate) fn read(value: &Value) -> Option<Record> {
        value.is_object().then(|| Record {
            sender: value["sender"].as_str().unwrap_or("").to_owned(),
            revision: value["revision"].as_u64().unwrap_or(0),
            names: strings(&value["names"]),
            // Order is the eviction order.
            retired: value["retired"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect(),
        })
    }
    pub(crate) fn json(&self) -> Value {
        json!({"sender": self.sender, "revision": self.revision, "names": self.names, "retired": self.retired})
    }
}

impl Journal {
    pub(crate) fn read(value: &Value) -> Option<Journal> {
        value.is_object().then(|| Journal {
            sender: value["sender"].as_str().map(str::to_owned),
            revision: value["revision"].as_u64().unwrap_or(0),
            names: strings(&value["names"]),
        })
    }
    pub(crate) fn json(&self) -> Value {
        json!({"sender": self.sender, "revision": self.revision, "names": self.names})
    }
}

impl Sent {
    pub(crate) fn read(value: &Value) -> Option<Sent> {
        Some(Sent {
            sender: value["sender"].as_str()?.to_owned(),
            revision: value["revision"].as_u64()?,
        })
    }
    pub(crate) fn json(&self) -> Value {
        json!({"sender": self.sender, "revision": self.revision})
    }
}
