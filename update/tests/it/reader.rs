//! A selected generation's assets as a reader that opens files by path
//! reads them: under the set's directory, all checked before it boots.
//!
//! @ref LLP 1030.006 D1

use crate::support::*;
use exact_update::Check;

#[test]
#[cfg_attr(
    windows,
    ignore = "Windows durable store is not qualified (LLP 1026 D11a)"
)]
fn a_generation_is_checked_whole_under_its_directory() {
    let temp = Temp::new("reader-assets");
    let bundle = Bundle::new(4, b"plan four")
        .asset("assets/tone.wav", b"a tone")
        .asset("assets/page.html", b"a page");
    let mut origin = Origin::of(&bundle);
    let mut store = open(&temp);
    let Ok(Check::Staged { entry, .. }) = origin.check(&mut store) else {
        panic!("the head should have staged");
    };
    drop(store); // The preceding launch releases its exclusive store ownership.
    let mut launch = open(&temp);
    let prepared = launch.prepare_selected().unwrap().unwrap();
    let dir = temp.path().join("entries").join(&entry).join("assets");
    assert_eq!(prepared.assets.root(), dir);
    assert_eq!(
        std::fs::read(dir.join("assets/page.html")).unwrap(),
        b"a page"
    );
    prepared.assets.verify().unwrap();

    // A file nothing has resolved yet is checked too.
    std::fs::write(dir.join("assets/page.html"), b"b page").unwrap();
    let refusal = prepared.assets.verify().unwrap_err();
    assert!(refusal.contains("hashes to"), "unexpected: {refusal}");
}
