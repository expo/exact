use super::super::items;
use super::*;
use std::ffi::OsStr;
use std::path::PathBuf;

fn types(text: &str, url: &str) -> Vec<&'static str> {
    let lifts = Lifts::default();
    lifts.held.set(Some(Press { seat: 1, serial: 2 }));
    let lift = Lift {
        text: text.into(),
        url: url.into(),
        files: Vec::new(),
        picture: None,
        hotspot: (0., 0.),
    };
    assert!((lifts.lifter().lift)(lift), "a held press takes the lift");
    let Some(Command::Lift { offers, .. }) = lifts.take() else {
        panic!("the lift waits to go");
    };
    offers.iter().map(|(mime, _)| *mime).collect()
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
        text: "Fuji".into(),
        url: String::new(),
        files: Vec::new(),
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
