//! A task's native firing waits in its queue while a batch applies or owner
//! work is in flight (`deliverNativeTimer`), and `releaseWaitingFirings` asks
//! for it again once none of that holds it. The release runs from each hold's
//! `didSet`, so a hold added to `ownerInFlight` without one would leave a
//! waiting firing unasked: `applying` and every term of `ownerInFlight` are
//! stored properties whose `didSet` calls the release.

#[test]
fn every_hold_on_a_native_firing_releases_it_as_it_ends() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Sources/ExactKit/Session.swift");
    let src = std::fs::read_to_string(&path).unwrap();
    let line = src
        .lines()
        .find(|l| l.trim_start().starts_with("var ownerInFlight: Bool {"))
        .expect("ExactSession.ownerInFlight");
    let body = &line[line.find('{').unwrap() + 1..line.rfind('}').unwrap()];
    let mut holds: Vec<&str> = body
        .split("||")
        .map(|term| {
            term.trim()
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .next()
                .unwrap()
        })
        .collect();
    holds.push("applying");
    assert!(holds.len() >= 7, "ownerInFlight's terms: {holds:?}");
    for hold in holds {
        let declaration = src
            .lines()
            .find(|l| {
                let t = l.trim_start();
                (t.starts_with("var ") || t.starts_with("private(set) var "))
                    && t.contains(&format!("var {hold} "))
            })
            .unwrap_or_else(|| panic!("{hold} is no stored property of ExactSession"));
        assert!(
            declaration.contains("didSet { releaseWaitingFirings() }"),
            "{hold} holds a native firing but its declaration does not release it: {}",
            declaration.trim()
        );
    }
}
