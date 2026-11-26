use super::received::tests::{read_all, reading};
use super::*;
use std::io::Write;
use std::time::{Duration, Instant};

/// A reading whose URI list names `uris` and whose text is `text`, all
/// arrived, with the drop's number `id`.
fn arrived(id: u32, uris: &str, text: &str) -> Reading {
    let (mut reading, [mut u, link, mut t, key]) = reading();
    reading.id = id;
    u.write_all(uris.as_bytes()).unwrap();
    t.write_all(text.as_bytes()).unwrap();
    drop((u, link, t, key));
    read_all(&mut reading, Instant::now() + Duration::from_secs(60), &[]);
    reading
}

fn reading_stage(reading: Reading) -> Progress {
    Progress {
        stage: Stage::Reading(reading),
        cut: false,
    }
}

/// What `settle` hands over, by place and drop number, with how many files
/// each carries and whether a check kept them.
fn settled(
    drops: &mut [Progress],
    check: &mut dyn FnMut(Vec<PathBuf>) -> Answer<Checked>,
) -> (Vec<(usize, u32, usize, bool)>, Vec<String>) {
    let lines = std::cell::RefCell::new(Vec::new());
    let landings = settle(drops.iter_mut(), true, check, &|line| {
        lines.borrow_mut().push(line)
    });
    let landed = landings
        .into_iter()
        .map(|(at, l)| {
            let files = l.checked.as_ref().map_or(l.dropped.paths.len(), Vec::len);
            (at, l.id, files, l.checked.is_some())
        })
        .collect();
    (landed, lines.into_inner())
}

/// A file of this test's, checked as a sandbox's drop is.
fn checked_file(name: &str) -> (tempfile::TempDir, Checked) {
    let folder = tempfile::tempdir().unwrap();
    let path = std::fs::canonicalize(folder.path()).unwrap().join(name);
    std::fs::write(&path, name).unwrap();
    let checked = Checked::of(&path).unwrap();
    (folder, checked)
}

/// A drop whose URI list came and whose portal answer had not when another
/// drop cut its wait lands at once, its files left out with no check begun,
/// and the drop released behind it lands after it.
#[test]
fn a_cut_drop_still_waiting_on_the_portal_lands_at_once_without_its_files() {
    let mut first = arrived(1, "file:///home/a.txt\r\n", "Fuji");
    first.retrieved = Some(Arc::new(Mutex::new(None)));
    assert!(!first.complete(), "its portal answer is still to come");
    let mut drops = [reading_stage(first), reading_stage(arrived(2, "", "Etna"))];
    drops[0].cut(&|_| {});
    let (landed, lines) = settled(&mut drops, &mut |_| {
        panic!("no check begins for a cut drop")
    });
    assert_eq!(landed, [(0, 1, 0, false), (1, 2, 0, false)]);
    assert_eq!(
        lines,
        ["drop: 1 files were left out: another drop came before the sandbox's check of them began"]
    );
}

/// A check that ended before another drop cut the wait keeps its files,
/// though the listener had not yet heard it; one still running then is cut
/// short, and a running check holds the drop released after it.
#[test]
fn a_cut_drop_keeps_a_check_that_ended_and_lands_before_the_next() {
    let (_folder, kept) = checked_file("notes.txt");
    let answer: Answer<Checked> = Arc::new(Mutex::new(None));
    let worker = answer.clone();
    let mut drops = [
        reading_stage(arrived(1, "file:///home/notes.txt\r\n", "")),
        reading_stage(arrived(2, "", "Etna")),
    ];
    let mut asked = Vec::new();
    let (landed, _) = settled(&mut drops, &mut |paths| {
        asked.push(paths);
        answer.clone()
    });
    assert_eq!(asked, [vec![PathBuf::from("/home/notes.txt")]]);
    assert!(
        landed.is_empty(),
        "a running check holds the drop behind it"
    );
    *worker.lock().unwrap() = Some(vec![kept]);
    drops[0].cut(&|line| panic!("a check that ended is not cut: {line}"));
    let (landed, _) = settled(&mut drops, &mut |_| unreachable!());
    assert_eq!(landed, [(0, 1, 1, true), (1, 2, 0, false)]);

    let mut drops = [reading_stage(arrived(3, "file:///home/notes.txt\r\n", ""))];
    let running: Answer<Checked> = Arc::new(Mutex::new(None));
    let (landed, _) = settled(&mut drops, &mut |_| running.clone());
    assert!(landed.is_empty());
    let lines = std::cell::RefCell::new(Vec::new());
    drops[0].cut(&|line| lines.borrow_mut().push(line));
    let (landed, _) = settled(&mut drops, &mut |_| unreachable!());
    assert_eq!(landed, [(0, 3, 0, true)], "cut short, it delivers none");
    assert_eq!(
        lines.into_inner().len(),
        1,
        "the journal hears of the file left out"
    );
    *running.lock().unwrap() = Some(Vec::new());
}
