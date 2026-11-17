//! LLP 1030.004 D2: a durable write announces its topic for the app's other
//! sessions, and a read in an answer watches it.
#![cfg(exact_js_engine)]
use super::storage::{call, Root, GRANTS};
use exact_runner::{DataSource, Store};

#[test]
fn a_write_announces_its_topics_and_a_read_watches_them() {
    let root = Root::new();
    let mut m = root.module();
    let mut store = Store::new(GRANTS, vec![]);
    m.activate().unwrap();
    let note = ["app:/data/note".to_string(), "app:/data".into()];

    // `file` writes the note, then reads it back.
    call(&mut m, &mut store, "file", "saved");
    assert_eq!(
        store.take_announced(),
        note,
        "the file and the directory that lists it"
    );
    store.take_topics();
    call(&mut m, &mut store, "read", "");
    assert_eq!(store.take_topics(), note);
    assert!(
        store.take_announced().is_empty(),
        "a read announces nothing"
    );

    call(&mut m, &mut store, "add", "row");
    assert_eq!(store.take_announced(), ["sqlite:app:/data/notes.db"]);
    store.take_topics();
    call(&mut m, &mut store, "types", "");
    assert_eq!(store.take_topics(), ["sqlite:app:/data/notes.db"]);
    assert!(
        store.take_announced().is_empty(),
        "a CREATE TABLE IF NOT EXISTS changes no rows, so two windows reading do not re-ask each other"
    );
}
