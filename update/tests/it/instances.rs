//! Two instances of one app over one store: the first owns it, and the
//! second boots what it selects and writes nothing.
//!
//! @ref LLP 1026 D9 (selection is a stat and a read) / D11 (one owner)

use crate::support::*;
use exact_update::Check;

#[test]
#[cfg_attr(
    windows,
    ignore = "Windows durable store is not qualified (LLP 1026 D11a)"
)]
fn a_second_instance_boots_the_owners_selection_and_writes_nothing() {
    let temp = Temp::new("second-instance");
    let bundle = Bundle::new(4, b"plan four").asset("mark.png", b"a mark");
    let mut origin = Origin::of(&bundle);
    let mut store = open(&temp);
    let Ok(Check::Staged { entry, .. }) = origin.check(&mut store) else {
        panic!("the head should have staged");
    };
    drop(store);
    let mut first = open(&temp);
    first.boot_started().unwrap();
    let record = std::fs::read(temp.path().join("record.json")).unwrap();
    let temporary = temp.path().join("entries/.tmp-first-instance-download");
    std::fs::create_dir(&temporary).unwrap();

    let mut second = open(&temp);
    assert_eq!(second.select().entry.as_deref(), Some(entry.as_str()));
    assert_eq!(second.select().seq, 4);
    let prepared = second.prepare_selected().unwrap().expect("seq 4 boots");
    assert_eq!(&*prepared.plan, b"plan four");
    assert_eq!(
        prepared.assets.resolve("mark.png").unwrap().as_deref(),
        Some(&b"a mark"[..])
    );
    second.boot_started().unwrap();
    second.boot_succeeded(&prepared.generation).unwrap();
    let status = second.status();
    assert_eq!(status.stream, format!("release/{COHORT}"));
    assert_eq!(status.running_seq, 4);
    assert!(!status.staged);
    let refused = origin.check(&mut second).unwrap_err();
    assert!(refused.contains("another process owns"), "{refused}");
    assert_eq!(
        std::fs::read(temp.path().join("record.json")).unwrap(),
        record,
        "the second instance writes no record"
    );
    assert!(temporary.exists(), "the second instance sweeps nothing");
    drop(second);
    drop(first);
    assert!(open(&temp).select().entry.is_some());
}

/// The owner moves on while a second instance runs an entry it no longer
/// keeps: the entry stays until the second instance exits.
#[test]
#[cfg_attr(
    windows,
    ignore = "Windows durable store is not qualified (LLP 1026 D11a)"
)]
fn the_owner_keeps_an_entry_a_second_instance_runs() {
    let temp = Temp::new("second-instance-held");
    let four = Bundle::new(4, b"plan four").asset("mark.png", b"mark four");
    let mut origin = Origin::of(&four);
    let mut first = open(&temp);
    let Ok(Check::Staged { entry: held, .. }) = origin.check(&mut first) else {
        panic!("seq 4 should have staged");
    };

    let second = open(&temp);
    assert_eq!(second.select().entry.as_deref(), Some(held.as_str()));
    origin.serving(&Bundle::new(5, b"plan five").asset("mark.png", b"mark five"));
    assert!(matches!(
        origin.check(&mut first),
        Ok(Check::Staged { seq: 5, .. })
    ));
    let dir = temp.path().join("entries").join(&held);
    assert_eq!(std::fs::read(dir.join("app.plan")).unwrap(), b"plan four");
    assert_eq!(
        std::fs::read(dir.join("assets/mark.png")).unwrap(),
        b"mark four"
    );

    drop(second);
    assert!(matches!(
        origin.check(&mut first),
        Ok(Check::Current { .. })
    ));
    assert!(!dir.exists(), "collected once no instance runs it");
}

/// Repairing a damaged entry must respect a secondary reader's shared advisory
/// lock on `entries/<digest>/exact.json` instead of deleting or mutating the
/// directory out from under the reader.
#[test]
#[cfg_attr(
    windows,
    ignore = "Windows durable store is not qualified (LLP 1026 D11a)"
)]
fn write_entry_does_not_replace_an_entry_held_by_a_reader() {
    let temp = Temp::new("reader-lock-repair");
    let bundle = Bundle::new(4, b"plan four")
        .asset("keep.png", b"keep me")
        .asset("rot.png", b"original");
    let mut origin = Origin::of(&bundle);
    let mut owner = open(&temp);
    let Ok(Check::Staged { entry, .. }) = origin.check(&mut owner) else {
        panic!("seq 4 stages");
    };
    let reader = open(&temp);
    assert_eq!(reader.select().entry.as_deref(), Some(entry.as_str()));

    // Corrupt `app.plan` so the owner tries to repair the entry on its next check.
    let dir = temp.path().join("entries").join(&entry);
    std::fs::remove_file(dir.join("app.plan")).unwrap();
    std::fs::write(dir.join("app.plan"), b"corrupt!!").unwrap();

    let err = origin.check(&mut owner).unwrap_err();
    assert!(err.contains("another instance is using it"), "{err}");
    assert!(dir.join("exact.json").is_file());
    assert_eq!(
        std::fs::read(dir.join("assets/keep.png")).unwrap(),
        b"keep me"
    );

    // Once the reader exits, repair succeeds cleanly.
    drop(reader);
    assert!(origin.check(&mut owner).is_ok());
    assert_eq!(std::fs::read(dir.join("app.plan")).unwrap(), b"plan four");
}
