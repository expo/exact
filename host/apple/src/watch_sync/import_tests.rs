//! `import`'s decisions, searched over every order of snapshots, selections and
//! failures that a small world allows, and named cases for single orderings.
//!
//! The world is three phones, two revisions each, two selectable names, and a
//! failure after any step. The search visits every store those reach.
use super::import::*;
use std::collections::{BTreeMap, HashSet, VecDeque};

const NAMES: [&str; 2] = ["x", "y"];
const SENDERS: [&str; 3] = ["a", "b", "c"];

/// A snapshot a phone sealed. `carries` is a bit per name in `NAMES`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct Snapshot {
    sender: usize,
    revision: u64,
    carries: u8,
}

/// The watch's store, and what the search knows about it that the store does not.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
struct World {
    meta: Meta,
    /// Each stored name, and the snapshot whose value it holds.
    values: BTreeMap<&'static str, Snapshot>,
    /// The snapshot the record stands for.
    source: Option<Snapshot>,
    generation: u64,
    invalidations: u64,
}

fn selection(mask: u8) -> Vec<String> {
    NAMES
        .iter()
        .enumerate()
        .filter(|(i, _)| mask & (1 << i) != 0)
        .map(|(_, n)| (*n).to_owned())
        .collect()
}

fn incoming(snapshot: Snapshot, policy: u8) -> Incoming {
    Incoming {
        sender: SENDERS[snapshot.sender].into(),
        revision: snapshot.revision,
        names: selection(snapshot.carries & policy),
    }
}

fn name(n: &str) -> &'static str {
    NAMES.iter().find(|k| **k == n).expect("a known name")
}

/// Perform one step as `watch_sync::perform` does, on the model store.
fn perform(world: &mut World, step: &Step, snapshot: Option<Snapshot>) {
    match step {
        Step::Journal(journal) => world.meta.journal = Some(journal.clone()),
        Step::Invalidate => world.invalidations += 1,
        Step::Write(n) => {
            world
                .values
                .insert(name(n), snapshot.expect("a write has a snapshot"));
        }
        Step::Delete(n) => {
            world.values.remove(name(n));
        }
        Step::Record(record) => {
            if let (Some(old), Some(new)) = (&world.meta.received, snapshot) {
                assert!(
                    !old.retired.contains(&record.sender),
                    "a retired phone's snapshot was imported: {world:?}"
                );
                if old.sender == record.sender {
                    assert!(
                        old.revision <= new.revision,
                        "an older revision replaced a newer one: {world:?} {new:?}"
                    );
                }
            }
            assert!(record.retired.len() <= RETIRED);
            world.meta.received = Some(record.clone());
            world.source = snapshot.or(world.source);
        }
        Step::Generation => world.generation += 1,
        Step::Clear => world.meta.journal = None,
    }
}

/// A store with no journal holds exactly one import: each name the record
/// lists has that snapshot's value, and no other name is stored.
fn assert_whole(world: &World) {
    if world.meta.journal.is_some() {
        return;
    }
    let listed = world
        .meta
        .received
        .as_ref()
        .map(|r| r.names.clone())
        .unwrap_or_default();
    for n in NAMES {
        let held = world.values.get(n).copied();
        if listed.iter().any(|l| l == n) {
            assert_eq!(held, world.source, "{n} is not its record's: {world:?}");
        } else {
            assert_eq!(held, None, "{n} outlived its record: {world:?}");
        }
    }
}

/// The steps keep the order readers in other processes rely on.
fn assert_ordered(steps: &[Step]) {
    let at = |wanted: fn(&Step) -> bool| -> Vec<usize> {
        steps
            .iter()
            .enumerate()
            .filter(|(_, s)| wanted(s))
            .map(|(i, _)| i)
            .collect()
    };
    let journal = at(|s| matches!(s, Step::Journal(_)));
    let invalidate = at(|s| matches!(s, Step::Invalidate));
    let values = at(|s| matches!(s, Step::Write(_) | Step::Delete(_)));
    let record = at(|s| matches!(s, Step::Record(_)));
    let generation = at(|s| matches!(s, Step::Generation));
    let clear = at(|s| matches!(s, Step::Clear));
    assert_eq!(journal, [0], "{steps:?}");
    assert_eq!(clear, [steps.len() - 1], "{steps:?}");
    assert_eq!(invalidate.len(), 2, "{steps:?}");
    assert_eq!(generation.len(), 1, "{steps:?}");
    let last_change = values.iter().chain(&record).max().copied().unwrap_or(0);
    assert!(
        invalidate[0] < values.first().copied().unwrap_or(usize::MAX),
        "{steps:?}"
    );
    assert!(
        last_change < generation[0] && generation[0] < invalidate[1] && invalidate[1] < clear[0],
        "{steps:?}"
    );
}

#[derive(Clone, Copy, Debug)]
enum Move {
    /// The watch app applies `snapshot` under its binary's selection.
    Apply { snapshot: Snapshot, policy: u8 },
    /// A binary with this selection retires what it no longer selects.
    Retire { policy: u8 },
}

fn moves() -> Vec<Move> {
    let mut moves = Vec::new();
    for policy in [0b01, 0b11] {
        for sender in 0..SENDERS.len() {
            for revision in 1..=2 {
                for carries in [0b01, 0b11] {
                    let snapshot = Snapshot {
                        sender,
                        revision,
                        carries,
                    };
                    moves.push(Move::Apply { snapshot, policy });
                }
            }
        }
    }
    moves.extend([0b00, 0b01, 0b11].map(|policy| Move::Retire { policy }));
    moves
}

fn decision(world: &World, m: Move) -> (Decision, Option<Snapshot>) {
    match m {
        Move::Apply { snapshot, policy } => {
            let incoming = incoming(snapshot, policy);
            let decision = decide(&world.meta, &incoming);
            // An unfinished import is never answered as up to date.
            if world.meta.journal.is_some() {
                assert_ne!(decision, Decision::Current, "{world:?} {m:?}");
            }
            (decision, Some(snapshot))
        }
        Move::Retire { policy } => (retire(&world.meta, &selection(policy)), None),
    }
}

/// Every store one move reaches: the move finished, and the move failed after
/// each of its steps.
fn successors(world: &World, m: Move) -> Vec<World> {
    let (decision, snapshot) = decision(world, m);
    let Decision::Import(steps) = decision else {
        return Vec::new();
    };
    assert_ordered(&steps);
    let mut reached = Vec::new();
    let mut next = world.clone();
    for step in &steps {
        reached.push(next.clone());
        perform(&mut next, step, snapshot);
    }
    assert!(next.meta.journal.is_none());
    if let Move::Apply { snapshot, policy } = m {
        for n in selection(snapshot.carries & policy) {
            assert_eq!(next.values.get(name(&n)), Some(&snapshot));
        }
    }
    reached.push(next);
    reached
}

fn reachable() -> HashSet<World> {
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([World::default()]);
    let moves = moves();
    while let Some(world) = queue.pop_front() {
        // The counters only grow, and decide nothing.
        let key = World {
            generation: 0,
            invalidations: 0,
            ..world.clone()
        };
        if !seen.insert(key) {
            continue;
        }
        assert_whole(&world);
        recovers(&world);
        for m in &moves {
            queue.extend(successors(&world, *m));
        }
    }
    seen
}

/// An unfinished import is finished, in one application, by the snapshot that
/// began it and by any current snapshot of the phone already accepted.
fn recovers(world: &World) {
    let Some(journal) = &world.meta.journal else {
        return;
    };
    let mut finishers = Vec::new();
    if let Some(sender) = &journal.sender {
        let sender = SENDERS.iter().position(|s| s == sender).unwrap();
        finishers.push((sender, journal.revision));
    }
    if let Some(record) = &world.meta.received {
        let sender = SENDERS.iter().position(|s| *s == record.sender).unwrap();
        // A newer journal of the same phone is finished by that revision.
        let newer = journal
            .sender
            .as_ref()
            .filter(|s| **s == record.sender)
            .map_or(0, |_| journal.revision);
        finishers.push((sender, record.revision.max(newer)));
    }
    for (sender, revision) in finishers {
        for policy in [0b01, 0b11] {
            let snapshot = Snapshot {
                sender,
                revision,
                carries: 0b11,
            };
            let finished = successors(world, Move::Apply { snapshot, policy });
            let last = finished.last().unwrap_or_else(|| {
                panic!("{snapshot:?} cannot finish {world:?}");
            });
            assert!(last.meta.journal.is_none());
            assert_whole(last);
        }
    }
}

#[test]
fn every_reachable_store_is_one_import_or_refuses_readers() {
    let seen = reachable();
    // A search that stops early proves nothing: this world reaches 32,424.
    assert!(seen.len() > 30_000, "{} stores", seen.len());
}

/// The timeline of one or more moves: the store before each step and after the last.
fn timeline(world: &World, plan: &[(Move, Option<usize>)]) -> Vec<World> {
    let mut states = vec![world.clone()];
    let mut next = world.clone();
    for (m, stop) in plan {
        let (decision, snapshot) = decision(&next, *m);
        let Decision::Import(steps) = decision else {
            continue;
        };
        for step in steps.iter().take(stop.unwrap_or(steps.len())) {
            perform(&mut next, step, snapshot);
            states.push(next.clone());
        }
    }
    states
}

fn mark(world: &World) -> Mark {
    Mark {
        journal: world.meta.journal.is_some(),
        generation: world.generation,
    }
}

/// A reader in another process marks, reads `x`, reads `y`, and marks again,
/// with any step of an import between any two of those. A read it accepts is
/// one finished import's.
#[test]
fn a_read_an_import_overlaps_is_never_accepted() {
    let a = Snapshot {
        sender: 0,
        revision: 1,
        carries: 0b11,
    };
    let b = Snapshot {
        sender: 1,
        revision: 1,
        carries: 0b11,
    };
    let apply = |snapshot| Move::Apply {
        snapshot,
        policy: 0b11,
    };
    let start = timeline(&World::default(), &[(apply(a), None)])
        .pop()
        .unwrap();
    let mut accepted = 0;
    for first_stop in (0..8).map(Some).chain([None]) {
        // One import, then another: the reader may span both, or a failed
        // import and the retry that finishes it.
        let states = timeline(
            &start,
            &[
                (apply(b), first_stop),
                (apply(b), None),
                (apply(Snapshot { revision: 2, ..b }), None),
            ],
        );
        let last = states.len() - 1;
        for begin in 0..=last {
            for x in begin..=last {
                for y in x..=last {
                    for end in y..=last {
                        if !coherent(Some(&mark(&states[begin])), Some(&mark(&states[end]))) {
                            continue;
                        }
                        accepted += 1;
                        let read = (states[x].values.get("x"), states[y].values.get("y"));
                        assert_eq!(read.0, read.1, "a mixed read: {begin} {x} {y} {end}");
                        assert_eq!(read.0, states[begin].values.get("x"));
                        assert_whole(&states[begin]);
                    }
                }
            }
        }
    }
    assert!(accepted > 0, "no read was ever accepted");
    // A mark that cannot be read accepts nothing.
    let whole = mark(&start);
    assert!(!coherent(None, Some(&whole)) && !coherent(Some(&whole), None));
}

fn record(sender: &str, revision: u64, names: &[&str]) -> Record {
    Record {
        sender: sender.into(),
        revision,
        names: names.iter().map(|n| (*n).to_owned()).collect(),
        retired: Vec::new(),
    }
}

fn snapshot(sender: &str, revision: u64, names: &[&str]) -> Incoming {
    Incoming {
        sender: sender.into(),
        revision,
        names: names.iter().map(|n| (*n).to_owned()).collect(),
    }
}

fn deletes(decision: &Decision) -> Vec<&str> {
    let Decision::Import(steps) = decision else {
        panic!("{decision:?} is not an import");
    };
    steps
        .iter()
        .filter_map(|s| match s {
            Step::Delete(n) => Some(n.as_str()),
            _ => None,
        })
        .collect()
}

/// A binary that selects less imports the retained context's remaining names,
/// and deletes the replica it no longer selects only as part of that accepted
/// import.
#[test]
fn a_narrower_selection_deletes_replicas_only_inside_an_accepted_import() {
    let meta = Meta {
        received: Some(record("phone", 1, &["x", "y"])),
        journal: None,
    };
    // The retained context still carries both; this binary selects `x`.
    let decision = decide(&meta, &snapshot("phone", 1, &["x"]));
    assert_eq!(deletes(&decision), ["y"]);
    let Decision::Import(steps) = &decision else {
        unreachable!()
    };
    assert!(matches!(steps[0], Step::Journal(_)));
    assert!(steps.contains(&Step::Record(record("phone", 1, &["x"]))));
}

/// A context already accepted holds a name the old binary did not select. The
/// binary that selects it imports that revision again; the same selection does
/// not.
#[test]
fn a_wider_selection_imports_the_accepted_revision_again() {
    let meta = Meta {
        received: Some(record("phone", 10, &["x"])),
        journal: None,
    };
    assert_eq!(
        decide(&meta, &snapshot("phone", 10, &["x"])),
        Decision::Current
    );
    let decision = decide(&meta, &snapshot("phone", 10, &["x", "y"]));
    let Decision::Import(steps) = decision else {
        panic!("the added name was skipped");
    };
    assert!(steps.contains(&Step::Write("y".into())));
    assert!(steps.contains(&Step::Record(record("phone", 10, &["x", "y"]))));
    // An older revision is refused whatever it carries.
    assert_eq!(
        decide(&meta, &snapshot("phone", 9, &["x", "y"])),
        Decision::Current
    );
}

/// The journal of phone B's failed import is finished by the accepted phone
/// A's retained context, and nothing answers "up to date" until then.
#[test]
fn the_accepted_phone_finishes_a_replacement_phones_failed_import() {
    let meta = Meta {
        received: Some(record("a", 1, &["x", "y"])),
        journal: Some(Journal {
            sender: Some("b".into()),
            revision: 2,
            names: vec!["x".into(), "y".into()],
        }),
    };
    let decision = decide(&meta, &snapshot("a", 1, &["x", "y"]));
    let Decision::Import(steps) = &decision else {
        panic!("phone A's context was acknowledged without finishing the import");
    };
    assert!(steps.contains(&Step::Record(record("a", 1, &["x", "y"]))));
    assert_eq!(steps.last(), Some(&Step::Clear));
    // B's older context cannot finish B's newer journal, and is not dropped as done.
    assert_eq!(
        decide(&meta, &snapshot("b", 1, &["x", "y"])),
        Decision::Blocked
    );
    // B's own retained context finishes it and retires A.
    let Decision::Import(steps) = decide(&meta, &snapshot("b", 2, &["x", "y"])) else {
        panic!("phone B's retained context must finish its import");
    };
    assert!(steps
        .iter()
        .any(|s| matches!(s, Step::Record(r) if r.sender == "b" && r.retired == ["a".to_owned()])));
    // A retired phone is refused, but not acknowledged, while a journal stands.
    let mut retired = meta.clone();
    retired.received.as_mut().unwrap().retired = vec!["old".into()];
    assert_eq!(
        decide(&retired, &snapshot("old", 9, &["x"])),
        Decision::Blocked
    );
    retired.journal = None;
    assert_eq!(
        decide(&retired, &snapshot("old", 9, &["x"])),
        Decision::Current
    );
}

/// A new phone is always imported, and the oldest retired identity is
/// forgotten to make room.
#[test]
fn a_phone_past_the_retired_bound_is_imported() {
    let mut meta = Meta::default();
    for n in 0..RETIRED + 8 {
        let decision = decide(&meta, &snapshot(&format!("phone-{n}"), 1, &["x"]));
        let Decision::Import(steps) = decision else {
            panic!("phone {n} was refused");
        };
        for step in steps {
            match step {
                Step::Record(record) => meta.received = Some(record),
                Step::Journal(_)
                | Step::Invalidate
                | Step::Write(_)
                | Step::Delete(_)
                | Step::Generation
                | Step::Clear => {}
            }
        }
    }
    let record = meta.received.as_ref().unwrap();
    assert_eq!(record.retired.len(), RETIRED);
    assert_eq!(record.retired[0], "phone-7");
    assert_eq!(record.sender, format!("phone-{}", RETIRED + 7));
    // One still remembered stays refused.
    assert_eq!(
        decide(&meta, &snapshot("phone-38", 2, &["x"])),
        Decision::Current
    );
}

/// Caches expire before the first value changes and again before the journal
/// clears, for an import and for a retirement.
#[test]
fn caches_expire_before_values_change_and_before_the_journal_clears() {
    let meta = Meta {
        received: Some(record("phone", 1, &["x", "y"])),
        journal: None,
    };
    let Decision::Import(import) = decide(&meta, &snapshot("phone", 2, &["x"])) else {
        panic!("a newer revision is imported");
    };
    let Decision::Import(retirement) = retire(&meta, &[]) else {
        panic!("a binary that selects nothing retires its replicas");
    };
    assert_eq!(deletes(&Decision::Import(retirement.clone())), ["x", "y"]);
    for steps in [import, retirement] {
        assert_ordered(&steps);
        assert_eq!(steps[1], Step::Invalidate, "{steps:?}");
        assert_eq!(
            &steps[steps.len() - 3..],
            [Step::Generation, Step::Invalidate, Step::Clear]
        );
    }
    assert_eq!(retire(&Meta::default(), &[]), Decision::Current);
}

/// The phone's revisions, over every order of snapshot, reserve, hand-over,
/// refusal and process death, three sends deep. A revision is spent only on a
/// snapshot that was handed to the transport, or whose process died before it
/// could say; and no revision ever numbers two hand-overs.
#[test]
fn a_revision_is_never_spent_on_a_refused_send_or_handed_over_twice() {
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    struct Phone {
        sent: Option<Sent>,
        /// The snapshot this process holds: its candidate, and whether it is reserved.
        held: Option<(Sent, bool)>,
        handed: Vec<u64>,
        /// Revisions spent by a process that died between reserve and hand-over.
        lost: Vec<u64>,
        sends: u8,
    }
    let start = Phone {
        sent: None,
        held: None,
        handed: Vec::new(),
        lost: Vec::new(),
        sends: 0,
    };
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([start]);
    while let Some(phone) = queue.pop_front() {
        if !seen.insert(phone.clone()) {
            continue;
        }
        let spent = phone.sent.as_ref().map_or(0, |s| s.revision);
        let mut accounted: Vec<u64> = phone.handed.iter().chain(&phone.lost).copied().collect();
        if let Some((held, true)) = &phone.held {
            accounted.push(held.revision);
        }
        accounted.sort();
        assert_eq!(
            accounted,
            (1..=spent).collect::<Vec<_>>(),
            "a revision was spent without leaving, or left twice: {phone:?}"
        );
        match &phone.held {
            None if phone.sends < 3 => {
                let next = candidate(phone.sent.as_ref(), "phone").unwrap();
                queue.push_back(Phone {
                    held: Some((next, false)),
                    sends: phone.sends + 1,
                    ..phone.clone()
                });
            }
            None => {}
            Some((held, false)) => {
                // Oversized, or no watch: the snapshot is dropped unspent.
                queue.push_back(Phone {
                    held: None,
                    ..phone.clone()
                });
                let spent = reserve(phone.sent.as_ref(), held).expect("the candidate reserves");
                queue.push_back(Phone {
                    sent: Some(spent),
                    held: Some((held.clone(), true)),
                    ..phone.clone()
                });
            }
            Some((held, true)) => {
                let mut handed = phone.handed.clone();
                handed.push(held.revision);
                queue.push_back(Phone {
                    held: None,
                    handed,
                    ..phone.clone()
                });
                // The transport refused it: the revision comes back.
                queue.push_back(Phone {
                    sent: release(phone.sent.as_ref().unwrap(), held.revision),
                    held: None,
                    ..phone.clone()
                });
                // The process died before the transport answered.
                let mut lost = phone.lost.clone();
                lost.push(held.revision);
                queue.push_back(Phone {
                    held: None,
                    lost,
                    ..phone.clone()
                });
            }
        }
    }
    assert!(seen.len() > 50, "{} states", seen.len());
    // A stale candidate, and a revision that is not the last, change nothing.
    let sent = Sent {
        sender: "phone".into(),
        revision: 4,
    };
    assert_eq!(reserve(Some(&sent), &sent), None);
    assert_eq!(release(&sent, 3), None);
    assert_eq!(release(&sent, 4).unwrap().revision, 3);
}
