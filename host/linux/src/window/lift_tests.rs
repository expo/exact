use super::super::received::items;
use super::*;
use std::ffi::OsStr;
use std::path::PathBuf;

fn types(text: &str, url: &str) -> Vec<&'static str> {
    let lifts = Lifts::default();
    lifts.held.set(Some(Press { seat: 1, serial: 2 }));
    let lift = Lift {
        id: 7,
        text: text.into(),
        url: url.into(),
        files: Vec::new(),
        assets: exact_painted::image::Assets::embedded(PathBuf::new()),
        app: None,
        picture: None,
        hotspot: (0., 0.),
    };
    assert!((lifts.lifter().lift)(lift), "a held press takes the lift");
    let Some(Command::Lift { offers, lift, .. }) = lifts.take() else {
        panic!("the lift waits to go");
    };
    assert_eq!(lift, 7, "the lift goes by the presenter's number");
    offers.iter().map(|(mime, _)| *mime).collect()
}

/// LLP 1094.000 D16: Wayland's `enter` names an offer, not its source, so
/// the window knows its own lift's offer by the lift's own type, unique in
/// and among processes: another client's offer, an enter before the lift's
/// own, or another lift's, is no lift's.
#[test]
fn the_windows_own_lift_is_known_by_its_own_type_alone() {
    let (first, second) = (own_type(), own_type());
    assert_ne!(first, second);
    let pid = std::process::id();
    assert!(
        first.starts_with(&format!("application/x-exact-lift-{pid}-")),
        "{first}"
    );
    let offer = |types: &[&str]| types.iter().map(|t| t.to_string()).collect::<Vec<_>>();
    let waiting = [first.as_str(), second.as_str()];
    let foreign = offer(&[TEXTS[0], URI_LIST]);
    assert_eq!(
        offered(waiting.into_iter(), &foreign),
        None,
        "another client's drag"
    );
    let another_process = offer(&[TEXTS[0], &format!("application/x-exact-lift-{}-0", pid + 1)]);
    assert_eq!(
        offered(waiting.into_iter(), &another_process),
        None,
        "another Exact app's"
    );
    assert_eq!(
        offered(waiting.into_iter(), &offer(&[TEXTS[0], &second])),
        Some(1)
    );
}

#[test]
fn a_lift_offers_its_text_and_its_link_by_their_own_types() {
    assert_eq!(types("Fuji", ""), TEXTS);
    assert_eq!(
        types("", "https://example.com/fuji"),
        [URI_LIST, MOZ_URL],
        "a link alone offers no text"
    );
    assert_eq!(
        types("Fuji", "https://example.com/fuji"),
        [URI_LIST, MOZ_URL, TEXTS[0], TEXTS[1]]
    );
}

#[test]
fn a_lift_with_no_left_press_held_is_declined() {
    let lifts = Lifts::default();
    let lift = Lift {
        id: 7,
        text: "Fuji".into(),
        url: String::new(),
        files: Vec::new(),
        assets: exact_painted::image::Assets::embedded(PathBuf::new()),
        app: None,
        picture: None,
        hotspot: (0., 0.),
    };
    assert!(!(lifts.lifter().lift)(lift));
    assert!(lifts.take().is_none());
}

/// Each URL D1 admits, a non-breaking space or U+0085 in it among them,
/// comes back whole from a drop of either link type on the app's own
/// window.
#[test]
fn a_link_comes_back_whole_from_either_type() {
    for url in [
        "https://example.com/fuji",
        "https://example.com/caf\u{e9}?q='#\u{85}",
        "https://example.com/a\u{a0}b\u{a0}",
        "https://example.com/\u{2003}",
    ] {
        assert_eq!(exact_plan::http_url_refusal(url), None, "D1 admits {url:?}");
        let listed = items(uri_list(&[] as &[PathBuf], url).as_bytes(), b"", b"");
        assert_eq!(listed.url, url, "from the URI list");
        let titled = items(b"", &moz_url(url), b"");
        assert_eq!(titled.url, url, "from the titled link");
    }
}

#[test]
fn a_uri_list_names_each_file_then_the_link_and_a_drop_reads_it_back() {
    let raw = PathBuf::from(OsStr::from_bytes(b"/tmp/caf\xe9 #1.jpg"));
    let list = uri_list(
        &[PathBuf::from("/srv/app/assets/caltrain.png"), raw.clone()],
        "https://example.com/fuji",
    );
    assert_eq!(
        list,
        "file:///srv/app/assets/caltrain.png\r\nfile:///tmp/caf%E9%20%231.jpg\r\nhttps://example.com/fuji\r\n"
    );
    let dropped = items(list.as_bytes(), b"", b"");
    assert_eq!(
        dropped.paths,
        [PathBuf::from("/srv/app/assets/caltrain.png"), raw]
    );
    assert_eq!(dropped.url, "https://example.com/fuji");
}

fn picture(width: u32, height: u32, scale: f32) -> LiftPicture {
    LiftPicture {
        width,
        height,
        scale,
        rgba: [0x10, 0x20, 0x30, 0x40].repeat((width * height) as usize),
    }
}

/// The ARGB pixel at (`x`, `y`) of `icon`.
fn pixel(icon: &Icon, x: i32, y: i32) -> u32 {
    let at = ((y * icon.width + x) * 4) as usize;
    u32::from_ne_bytes(icon.argb[at..at + 4].try_into().unwrap())
}

#[test]
fn an_icon_at_an_integer_scale_grows_to_whole_points_around_the_press() {
    // 21 by 10 pixels at 2, held 5 pixels in and 4 down: the press is 3 points in once a pixel is gained before it
    let icon = Icon::of(&picture(21, 10, 2.), (2.5, 2.));
    assert_eq!((icon.width, icon.height, icon.scale), (22, 10, Some(2)));
    assert_eq!((icon.size, icon.offset), ((11, 5), (-3, -2)));
    assert_eq!(pixel(&icon, 0, 0), 0, "the gained column is clear");
    assert_eq!(pixel(&icon, 1, 0), 0x40102030, "premultiplied ARGB");
    assert_eq!(
        pixel(&icon, 21, 9),
        0x40102030,
        "the picture's last pixel is its own"
    );
}

#[test]
fn an_icon_at_a_fractional_scale_is_shown_at_its_size_in_points() {
    let icon = Icon::of(&picture(30, 15, 1.5), (4., 4.));
    assert_eq!(icon.scale, None, "a viewport shows it");
    assert_eq!(
        icon.offset,
        (-4, -4),
        "held 6 pixels in, whole points already"
    );
    assert_eq!(icon.size, (20, 10));
    assert_eq!((icon.width, icon.height), (30, 15));
    // Held 4.5 pixels in, taken as 5: a pixel gained before puts the press 4 points in
    let held = Icon::of(&picture(31, 15, 1.5), (3., 0.5));
    assert_eq!(held.offset, (-4, -1));
    assert_eq!(
        (held.width, held.size.0),
        (33, 22),
        "the picture and the gained pixel, to whole points"
    );
}

/// A key request the listener holds, its copies made: its pipe closes,
/// with no key, once its registration has not answered by 10 seconds after
/// the later of the request and the copies' end, and the registration is
/// told, so a key that comes later is stopped; a key that came in time goes
/// out with GTK's terminal NUL by that deadline.
#[test]
fn a_key_request_closes_by_its_deadline_whatever_its_registration_does() {
    let held = |registration: Registration, requested: Instant| {
        let staging = Staging::new(
            Vec::new(),
            exact_painted::image::Assets::embedded(PathBuf::new()),
            None,
        );
        staging.made(Default::default());
        let (reader, writer) = std::io::pipe().unwrap();
        assert!(super::super::nonblocking(&reader));
        let answer = Arc::new(Mutex::new(registration));
        let keyed = Keyed {
            pipe: std::fs::File::from(std::os::fd::OwnedFd::from(writer)),
            requested,
            staging,
            answer: answer.clone(),
        };
        (Keys { keyed: vec![keyed] }, reader, answer)
    };
    let open = |reader: &mut std::io::PipeReader| {
        let mut byte = [0; 1];
        !matches!(reader.read(&mut byte), Ok(0))
    };
    let now = Instant::now();
    let expired = now + DEADLINE + Duration::from_secs(1);

    let (mut keys, mut reader, answer) = held(Registration::Waiting, now);
    assert!(keys.settle(now).is_empty());
    assert!(open(&mut reader), "the registration still has time");
    assert!(keys.settle(expired).is_empty());
    assert!(
        !open(&mut reader),
        "its time ended: the target reads the pipe's end"
    );
    assert!(
        matches!(*answer.lock().unwrap(), Registration::Abandoned),
        "a key that comes later is stopped"
    );

    let later = now + Duration::from_secs(5);
    let (mut keys, mut reader, _) = held(Registration::Waiting, later);
    assert!(keys.settle(expired).is_empty());
    assert!(
        open(&mut reader),
        "a request after the copies has its own 10 seconds"
    );

    let transfer = exact_painted::file_transfer::Transfer {
        key: "1a2b".into(),
        connection: 1,
    };
    let (mut keys, _reader, _) = held(Registration::Answered(Some(transfer)), now);
    let mut sends = keys.settle(now);
    assert_eq!(sends.len(), 1);
    assert_eq!(&*sends[0].bytes, b"1a2b\0");
    assert!(sends[0].deadline > now + DEADLINE - Duration::from_secs(1));
    sends[0].key = None;
}

/// A key goes only by its deadline, though its pipe is ready after it, and
/// is stopped only when not all its characters went: the terminal NUL alone
/// failing, as when the target stops reading after the key, leaves a key the
/// receiver takes.
#[test]
fn a_key_goes_by_its_deadline_and_is_kept_once_its_characters_went() {
    let sending = |at: usize| {
        let (reader, writer) = std::io::pipe().unwrap();
        assert!(super::super::nonblocking(&writer));
        let sending = Sending {
            pipe: std::fs::File::from(std::os::fd::OwnedFd::from(writer)),
            bytes: b"1a2b\0".as_slice().into(),
            at,
            deadline: Instant::now() + Duration::from_secs(60),
            key: Some(exact_painted::file_transfer::Transfer {
                key: "1a2b".into(),
                connection: 1,
            }),
        };
        (sending, reader)
    };
    let now = Instant::now();
    let (mut whole, reader) = sending(4);
    drop(reader);
    whole.write(&|| now);
    assert_eq!(
        (whole.at, whole.deadline <= now),
        (4, true),
        "its NUL failed, which ends it"
    );
    assert_eq!(
        whole.unfinished_key(),
        None,
        "a key without its NUL is taken"
    );
    whole.key = None;
    let (mut cut, reader) = sending(2);
    drop(reader);
    cut.write(&|| now);
    assert_eq!(
        cut.unfinished_key().map(|t| t.key.as_str()),
        Some("1a2b"),
        "a key cut short is stopped"
    );
    cut.key = None;

    let (mut late, _reader) = sending(0);
    let deadline = late.deadline;
    late.write(&|| deadline);
    assert_eq!(late.at, 0, "a ready pipe hears nothing past the deadline");
    late.write(&|| now);
    assert_eq!(late.at, 5, "before it, the key and its NUL go in one write");

    // The deadline passes during a pass: the write after it does not start
    let (reader, writer) = std::io::pipe().unwrap();
    assert!(super::super::nonblocking(&writer));
    let mut long = Sending {
        pipe: std::fs::File::from(std::os::fd::OwnedFd::from(writer)),
        bytes: vec![b'.'; 1 << 20].into(),
        at: 0,
        deadline: now + Duration::from_secs(10),
        key: None,
    };
    let reads = std::cell::Cell::new(0);
    let deadline = long.deadline;
    let clock = || {
        reads.set(reads.get() + 1);
        if reads.get() == 1 {
            now
        } else {
            deadline
        }
    };
    long.write(&clock);
    assert!(
        long.at > 0 && long.at <= 1 << 16,
        "one write, then the clock passed the deadline: {}",
        long.at
    );
    drop(reader);
}
