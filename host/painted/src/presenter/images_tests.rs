//! A picture's download retry as the presenter places it: a picture under a
//! `display: none` box (no painted box) is not on screen, so a failed
//! download of it is not asked again until it shows.
use super::*;
use crate::image::LoadEvent;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// A server answering 503 to every request, counting them; its origin.
fn busy_server(asked: Arc<AtomicUsize>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            asked.fetch_add(1, Ordering::SeqCst);
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            while reader.read_line(&mut line).is_ok_and(|n| n > 2) {
                line.clear();
            }
            let _ = stream
                .write_all(b"HTTP/1.1 503 Busy\r\ncontent-length: 0\r\nconnection: close\r\n\r\n");
        }
    });
    origin
}

#[test]
fn a_hidden_pictures_failed_download_is_asked_again_only_once_it_shows() {
    let asked = Arc::new(AtomicUsize::new(0));
    let origin = busy_server(asked.clone());
    let source = format!(
        r#"component App
  state shown = false
  action reveal
    shown = true
  view
    column width=300 height=300
      button "Show" press=reveal testId="show" width=100 height=40
      box display=(shown ? "flex" : "none") width=200 height=200
        image "{origin}/busy.png" width=200 height=200 testId="picture"
"#
    );
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(&source),
        (),
        (300., 300.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let settle = |p: &mut Presenter<()>, ms: u64| {
        let end = std::time::Instant::now() + Duration::from_millis(ms);
        let mut t = 16.;
        while std::time::Instant::now() < end {
            p.poll_images();
            t += 16.;
            p.advance(t);
            p.frame();
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    // Hidden: asked for once (a first load), it fails, and nothing more is asked or due.
    settle(&mut p, 1500);
    let hidden = asked.load(Ordering::SeqCst);
    assert!(hidden <= 1, "{hidden} asks while hidden");
    assert_eq!(
        p.image_turn_due_in_ms(),
        None,
        "no retry is due for a picture with no painted box"
    );
    // Shown: it is on screen, so its failed download is asked again (within the test's backoff of milliseconds).
    let k = p.host().kernel();
    let show = k.node_by_key(k.find_by_test_id("show")[0]).unwrap().id;
    assert!(p
        .host_mut()
        .dispatch_at(show, exact_runner::Event::Press, 0.)
        .is_none());
    settle(&mut p, 1500);
    assert!(
        asked.load(Ordering::SeqCst) > hidden + 1,
        "asked again once shown: {} after {hidden}",
        asked.load(Ordering::SeqCst)
    );
}

/// A server answering every request with a 400 by 400 PNG, counting them; its origin.
fn picture_server(asked: Arc<AtomicUsize>) -> String {
    picture_server_gated(asked, None)
}

fn picture_server_gated(
    asked: Arc<AtomicUsize>,
    mut gate: Option<std::sync::mpsc::Receiver<()>>,
) -> String {
    let mut body = Vec::new();
    let mut encoder = png::Encoder::new(&mut body, 400, 400);
    encoder.set_color(png::ColorType::Rgba);
    let mut writer = encoder.write_header().unwrap();
    writer
        .write_image_data(&[0, 90, 200, 255].repeat(400 * 400))
        .unwrap();
    writer.finish().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            asked.fetch_add(1, Ordering::SeqCst);
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            while reader.read_line(&mut line).is_ok_and(|n| n > 2) {
                line.clear();
            }
            if let Some(gate) = gate.take() {
                if gate.recv().is_err() {
                    return;
                }
            }
            let head = format!(
                "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
        }
    });
    origin
}

#[test]
fn a_landed_picture_owes_one_paint_and_no_idle_deadline() {
    let (release, gate) = std::sync::mpsc::channel();
    let origin = picture_server_gated(Arc::new(AtomicUsize::new(0)), Some(gate));
    let source =
        format!("component App\n  view\n    image \"{origin}/paint.png\" width=20 height=20\n");
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(&source),
        (),
        (40., 40.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.frame();
    assert!(!p.pictures_need_paint());
    release.send(()).unwrap();
    assert!(p.wait_images(Duration::from_secs(30)));
    assert!(p.pictures_need_paint());
    assert!(p.dirty());
    let pixels = p.frame();
    assert_eq!(pixels.pixel(10, 10).unwrap().blue(), 200);
    assert!(!p.pictures_need_paint());
    assert!(!p.dirty());
    assert!(!p.poll_images(), "no duplicate arrival");
    assert!(!p.pictures_need_paint());
    assert_eq!(p.wake_due_ms(), None, "no new idle wake");
}

#[test]
fn an_image_hears_load_once_a_source_when_its_first_picture_lands() {
    let asked = Arc::new(AtomicUsize::new(0));
    let origin = picture_server(asked.clone());
    let source = format!(
        r#"component App
  state loads = 0
  state big = false
  state name = "a"
  action loaded
    loads = loads + 1
  action grow
    big = true
  action other
    name = "b"
  action first
    name = "a"
  view
    column width=300 height=300
      button "Grow" press=grow testId="grow" width=100 height=30
      button "Other" press=other testId="other" width=100 height=30
      button "First" press=first testId="first" width=100 height=30
      image "{origin}/" + name + ".png" width=(big ? 200 : 20) height=(big ? 200 : 20) load=loaded testId="picture"
"#
    );
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(&source),
        (),
        (300., 300.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let mut t = 16.;
    let mut settle = |p: &mut Presenter<()>, ms: u64| {
        let end = std::time::Instant::now() + Duration::from_millis(ms);
        while std::time::Instant::now() < end {
            p.poll_images();
            t += 16.;
            p.advance(t);
            p.frame();
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    let loads = |p: &mut Presenter<()>| {
        let state = crate::agent::handle(p, r#"{"op":"state"}"#);
        let state: serde_json::Value = serde_json::from_str(&state).unwrap();
        state["slots"]["loads"].as_f64().unwrap()
    };
    let press = |p: &mut Presenter<()>, test_id: &str| {
        let k = p.host().kernel();
        let id = k.node_by_key(k.find_by_test_id(test_id)[0]).unwrap().id;
        assert!(p
            .host_mut()
            .dispatch_at(id, exact_runner::Event::Press, 0.)
            .is_none());
    };
    settle(&mut p, 800);
    assert_eq!(loads(&mut p), 1., "its first picture sends load");
    // Decoded again for a larger box: the same source, no second load.
    let first = p.images.bitmaps.values().next().unwrap().clone();
    press(&mut p, "grow");
    settle(&mut p, 500);
    let again = p.images.bitmaps.values().next().unwrap().clone();
    assert!(!Arc::ptr_eq(&first, &again), "decoded again");
    assert_eq!(loads(&mut p), 1., "a re-decode is not a second load");
    // Another source: its own load.
    press(&mut p, "other");
    settle(&mut p, 800);
    assert_eq!(loads(&mut p), 2., "a new source's picture sends load");
    // The first source again, its body from the cache: assigned again, so its own load.
    let fetched = asked.load(Ordering::SeqCst);
    press(&mut p, "first");
    settle(&mut p, 800);
    assert_eq!(asked.load(Ordering::SeqCst), fetched, "from the cache");
    assert_eq!(
        loads(&mut p),
        3.,
        "a cached source assigned again sends load"
    );
}

#[test]
fn a_load_waiting_is_sent_only_to_the_node_and_source_it_landed_for_and_owes_a_turn() {
    let source = r#"component App
  state loads = 0
  action loaded
    loads = loads + 1
  view
    column width=300 height=300
      image "pictures/a.png" width=20 height=20 load=loaded testId="picture"
"#;
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(source),
        (),
        (300., 300.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let k = p.host().kernel();
    let node = k.node_by_key(k.find_by_test_id("picture")[0]).unwrap();
    let (id, key) = (node.id, node.key);
    let packed = u64::from(key.index) | (u64::from(key.generation) << 32);
    let assignment = p.images.assignment(id).unwrap();
    let event = |key: u64, source: &str, assignment: u64| LoadEvent {
        id,
        key,
        source: source.into(),
        assignment,
        error: None,
    };
    let loads = |p: &mut Presenter<()>| {
        let state = crate::agent::handle(p, r#"{"op":"state"}"#);
        let state: serde_json::Value = serde_json::from_str(&state).unwrap();
        state["slots"]["loads"].as_f64().unwrap()
    };
    // Landed for another source (the node's changed since), another node at this id, or another
    // assignment of the source: not sent.
    p.images
        .load_events
        .push(event(packed, "pictures/old.png", assignment));
    p.images
        .load_events
        .push(event(packed + 1, "pictures/a.png", assignment));
    p.images
        .load_events
        .push(event(packed, "pictures/a.png", assignment + 1));
    assert_eq!(
        p.image_turn_due_in_ms(),
        Some(0.0),
        "a waiting load owes a turn at once"
    );
    p.apply_reports(Vec::new());
    assert_eq!(loads(&mut p), 0.);
    // (The missing file's own `error` may wait: no `load` does.)
    assert!(!p.images.load_events.iter().any(|e| e.error.is_none()));
    // Its own node and source: sent.
    p.images
        .load_events
        .push(event(packed, "pictures/a.png", assignment));
    p.apply_reports(Vec::new());
    assert_eq!(loads(&mut p), 1.);
}

#[test]
fn a_load_is_not_sent_once_an_earlier_loads_action_changed_its_source() {
    // Two pictures land in one pass; the first's load changes the second's source.
    let source = r#"component App
  state second = "pictures/b.png"
  state loads = 0
  action first
    second = "pictures/c.png"
  action loaded
    loads = loads + 1
  view
    column width=300 height=300
      image "pictures/a.png" width=20 height=20 load=first testId="one"
      image second width=20 height=20 load=loaded testId="two"
"#;
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(source),
        (),
        (300., 300.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let event = |p: &Presenter<()>, test_id: &str, source: &str| {
        let k = p.host().kernel();
        let node = k.node_by_key(k.find_by_test_id(test_id)[0]).unwrap();
        LoadEvent {
            id: node.id,
            key: u64::from(node.key.index) | (u64::from(node.key.generation) << 32),
            source: source.to_owned(),
            assignment: p.images.assignment(node.id).unwrap(),
            error: None,
        }
    };
    let one = event(&p, "one", "pictures/a.png");
    let two = event(&p, "two", "pictures/b.png");
    p.images.load_events.extend([one, two]);
    p.apply_reports(Vec::new());
    let state = crate::agent::handle(&mut p, r#"{"op":"state"}"#);
    let state: serde_json::Value = serde_json::from_str(&state).unwrap();
    assert_eq!(
        state["slots"]["second"], "pictures/c.png",
        "the first's load ran"
    );
    assert_eq!(state["slots"]["loads"], 0., "{state}");
}

#[test]
fn a_load_waiting_is_dropped_when_actions_change_its_source_away_and_back() {
    // Three loads in one pass: the first's action moves the third to another
    // source, the second's moves it back. The third's old load is for an
    // assignment that has gone.
    let source = r#"component App
  state third = "pictures/a.png"
  state loads = 0
  action away
    third = "pictures/b.png"
  action back
    third = "pictures/a.png"
  action loaded
    loads = loads + 1
  view
    column width=300 height=300
      image "pictures/one.png" width=20 height=20 load=away testId="one"
      image "pictures/two.png" width=20 height=20 load=back testId="two"
      image third width=20 height=20 load=loaded testId="three"
"#;
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(source),
        (),
        (300., 300.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let event = |p: &Presenter<()>, test_id: &str, source: &str| {
        let k = p.host().kernel();
        let node = k.node_by_key(k.find_by_test_id(test_id)[0]).unwrap();
        LoadEvent {
            id: node.id,
            key: u64::from(node.key.index) | (u64::from(node.key.generation) << 32),
            source: source.to_owned(),
            assignment: p.images.assignment(node.id).unwrap(),
            error: None,
        }
    };
    let events = [
        event(&p, "one", "pictures/one.png"),
        event(&p, "two", "pictures/two.png"),
        event(&p, "three", "pictures/a.png"),
    ];
    p.images.load_events.extend(events);
    p.apply_reports(Vec::new());
    let state = crate::agent::handle(&mut p, r#"{"op":"state"}"#);
    let state: serde_json::Value = serde_json::from_str(&state).unwrap();
    assert_eq!(state["slots"]["third"], "pictures/a.png", "away and back");
    assert_eq!(state["slots"]["loads"], 0., "{state}");
}

/// What a scripted server answers a path's `n`th request with.
enum Reply {
    Status(u16, &'static str),
    Picture,
    Text,
}

/// A server answering each request as `answer(path, n)` says, counting them; its origin.
fn scripted_server(
    asked: Arc<std::sync::Mutex<Vec<String>>>,
    answer: impl Fn(&str, usize) -> Reply + Send + Sync + 'static,
) -> String {
    let mut picture = Vec::new();
    let mut encoder = png::Encoder::new(&mut picture, 8, 8);
    encoder.set_color(png::ColorType::Rgba);
    let mut writer = encoder.write_header().unwrap();
    writer
        .write_image_data(&[200, 40, 40, 255].repeat(64))
        .unwrap();
    writer.finish().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let answer = Arc::new(answer);
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            let _ = reader.read_line(&mut line);
            let path = line.split_whitespace().nth(1).unwrap_or("/").to_owned();
            let mut header = String::new();
            while reader.read_line(&mut header).is_ok_and(|n| n > 2) {
                header.clear();
            }
            let n = {
                let mut asked = asked.lock().unwrap();
                let n = asked.iter().filter(|p| **p == path).count();
                asked.push(path.clone());
                n
            };
            let (head, body): (String, Vec<u8>) = match answer(&path, n) {
                Reply::Status(code, extra) => (format!("{code} Nope\r\n{extra}"), Vec::new()),
                Reply::Picture => ("200 OK\r\n".into(), picture.clone()),
                Reply::Text => ("200 OK\r\n".into(), b"hello, not a picture".to_vec()),
            };
            let _ = stream.write_all(
                format!(
                    "HTTP/1.1 {head}content-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                )
                .as_bytes(),
            );
            let _ = stream.write_all(&body);
        }
    });
    origin
}

/// An app whose picture hears both events, and can be hidden and given another source.
fn error_app(source: &str) -> Presenter<()> {
    let source = format!(
        r#"component App
  state errors = 0
  state loads = 0
  state message = ""
  state shown = true
  state name = "a"
  action failed(m: string)
    errors = errors + 1
    message = m
  action loaded
    loads = loads + 1
  action toggle
    shown = not shown
  action other
    name = "b"
  view
    column width=300 height=300
      button "Toggle" press=toggle testId="toggle" width=100 height=30
      button "Other" press=other testId="other" width=100 height=30
      box display=(shown ? "flex" : "none") width=200 height=200
        image "{source}" + name + ".png" width=200 height=200 load=loaded error=failed testId="picture"
"#
    );
    let (p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(&source),
        (),
        (300., 300.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p
}
fn run(p: &mut Presenter<()>, ms: u64) {
    let end = std::time::Instant::now() + Duration::from_millis(ms);
    while std::time::Instant::now() < end {
        p.poll_images();
        let now = p.host().now() + 16.;
        p.advance(now);
        p.frame();
        std::thread::sleep(Duration::from_millis(5));
    }
}
/// Runs it until `done` (a generous bound: a busy machine is slow), then once more for anything after.
fn until(p: &mut Presenter<()>, done: impl Fn(&serde_json::Value) -> bool) -> serde_json::Value {
    let started = std::time::Instant::now();
    while !done(&slots(p)) {
        assert!(started.elapsed() < Duration::from_secs(30), "{}", slots(p));
        run(p, 10);
    }
    run(p, 100);
    slots(p)
}
fn slots(p: &mut Presenter<()>) -> serde_json::Value {
    let state = crate::agent::handle(p, r#"{"op":"state"}"#);
    serde_json::from_str::<serde_json::Value>(&state).unwrap()["slots"].clone()
}
fn press(p: &mut Presenter<()>, test_id: &str) {
    let k = p.host().kernel();
    let id = k.node_by_key(k.find_by_test_id(test_id)[0]).unwrap().id;
    let now = p.host().now();
    assert!(p
        .host_mut()
        .dispatch_at(id, exact_runner::Event::Press, now)
        .is_none());
}

#[test]
fn a_picture_that_cannot_load_hears_error_once_with_its_reason() {
    let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
    let origin = scripted_server(asked.clone(), |path, n| match path {
        p if p.starts_with("/flaky/") && n < 2 => Reply::Status(503, ""),
        p if p.starts_with("/flaky/") => Reply::Picture,
        p if p.starts_with("/text/") => Reply::Text,
        p if p.starts_with("/moved/") => {
            Reply::Status(302, "location: ftp://cdn.example/x.png?ex=1&hm=secret\r\n")
        }
        p if p.starts_with("/bad/") => {
            Reply::Status(302, "location: //bad:port/a.png?hm=secret#f\r\n")
        }
        _ => Reply::Status(404, ""),
    });
    let errored = |s: &serde_json::Value| s["errors"].as_f64() == Some(1.);
    // Asking again cannot mend a 404: its error at once, with the reason.
    let mut p = error_app(&format!("{origin}/gone/"));
    let s = until(&mut p, errored);
    assert_eq!(
        (s["errors"].as_f64(), s["loads"].as_f64()),
        (Some(1.), Some(0.)),
        "{s}"
    );
    assert_eq!(s["message"], "HTTP 404");

    // A busy server that answers in the end: its load, and no error.
    let mut p = error_app(&format!("{origin}/flaky/"));
    let s = until(&mut p, |s| s["loads"].as_f64() == Some(1.));
    assert_eq!(s["errors"].as_f64(), Some(0.), "{s}");

    // A body this host cannot decode, and redirects to links it cannot
    // follow, quoted without their queries.
    let mut p = error_app(&format!("{origin}/text/"));
    let s = until(&mut p, errored);
    assert!(
        s["message"]
            .as_str()
            .unwrap()
            .starts_with("not an image this host decodes"),
        "{s}"
    );
    let mut p = error_app(&format!("{origin}/moved/"));
    let s = until(&mut p, errored);
    assert_eq!(s["message"], "a redirect to ftp://cdn.example/x.png", "{s}");
    let mut p = error_app(&format!("{origin}/bad/"));
    let s = until(&mut p, errored);
    assert!(!s["message"].as_str().unwrap().contains("hm="), "{s}");

    // A bundled picture that is not there.
    let mut p = error_app("pictures/");
    until(&mut p, errored);
}

/// A source over the 4096-byte cap is refused: its `error` once, though
/// every sync gives it again.
#[test]
fn an_over_long_source_errs_once() {
    let long = format!("pictures/{}", "x".repeat(5000));
    let mut p = error_app(&long);
    let s = until(&mut p, |s| s["errors"].as_f64() == Some(1.));
    assert_eq!(s["message"], "a source over 4096 bytes");
    run(&mut p, 200);
    assert_eq!(slots(&mut p)["errors"].as_f64(), Some(1.));
}

#[test]
fn a_failure_that_asking_again_may_mend_errs_once_its_showings_retries_are_spent() {
    let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
    let origin = scripted_server(asked.clone(), |path, _| match path {
        p if p.ends_with("/b.png") => Reply::Picture,
        _ => Reply::Status(503, ""),
    });
    let mut p = error_app(&format!("{origin}/dead/"));
    // Some retries in, it leaves the screen: no error, and its count starts over.
    let started = std::time::Instant::now();
    while asked.lock().unwrap().len() < 3 {
        assert!(started.elapsed() < Duration::from_secs(10));
        run(&mut p, 10);
    }
    press(&mut p, "toggle");
    run(&mut p, 300);
    let hidden = asked.lock().unwrap().len();
    run(&mut p, 400);
    assert_eq!(
        asked.lock().unwrap().len(),
        hidden,
        "nothing asked off screen"
    );
    assert_eq!(
        slots(&mut p)["errors"].as_f64(),
        Some(0.),
        "no error off screen"
    );
    // Shown again: a whole ladder, then one error.
    press(&mut p, "toggle");
    let s = until(&mut p, |s| s["errors"].as_f64() == Some(1.));
    assert_eq!(
        asked.lock().unwrap().len(),
        hidden + 6,
        "{:?}",
        asked.lock().unwrap()
    );
    assert_eq!(
        (s["errors"].as_f64(), s["loads"].as_f64()),
        (Some(1.), Some(0.)),
        "{s}"
    );
    assert_eq!(s["message"], "HTTP 503");
    // After its error, nothing more is asked for it; another source loads.
    run(&mut p, 300);
    assert_eq!(asked.lock().unwrap().len(), hidden + 6);
    press(&mut p, "other");
    let s = until(&mut p, |s| s["loads"].as_f64() == Some(1.));
    assert_eq!(
        (s["errors"].as_f64(), s["loads"].as_f64()),
        (Some(1.), Some(1.)),
        "{s}"
    );
}

/// An over-long source, a symbol, then the same over-long source: a new
/// assignment, so its `error` again.
#[test]
fn an_over_long_source_given_again_after_a_symbol_errs_again() {
    let long = format!("pictures/{}", "x".repeat(5000));
    let source = format!(
        r#"component App
  state errors = 0
  state shown = "{long}"
  action failed(m: string)
    errors = errors + 1
  action symbol
    shown = "symbol:back"
  action back
    shown = "{long}"
  view
    column width=300 height=300
      button "Symbol" press=symbol testId="symbol" width=100 height=30
      button "Back" press=back testId="back" width=100 height=30
      image shown width=40 height=40 error=failed testId="picture"
"#
    );
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(&source),
        (),
        (300., 300.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    until(&mut p, |s| s["errors"].as_f64() == Some(1.));
    press(&mut p, "symbol");
    run(&mut p, 100);
    press(&mut p, "back");
    until(&mut p, |s| s["errors"].as_f64() == Some(2.));
}

/// A video's poster loads as an image's picture does and paints in its box,
/// placed by `object-fit` (`contain`: 150×150 in the middle here), until the player shows its source's first frame; the video's natural
/// size stays its video's (LLP 1042.001 D5).
#[test]
fn a_poster_paints_until_the_first_frame_and_sizes_nothing() {
    let origin = picture_server(Arc::new(AtomicUsize::new(0)));
    let source = format!(
        "component App\n  view\n    video src=\"a.mp4\" poster=\"{origin}/poster.png\" testId=\"video\"\n"
    );
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(&source),
        (),
        (400., 400.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.frame();
    assert!(p.wait_images(Duration::from_secs(30)));
    let pixels = p.frame();
    assert_eq!(pixels.pixel(150, 75).unwrap().blue(), 200, "the poster");
    let key = p.host.kernel().find_by_test_id("video")[0];
    let node = p.host.kernel().node_by_key(key).unwrap();
    // HTML's 150 high before metadata, not the poster's 400.
    assert_eq!(node.frame.height, 150.);
    let (id, session) = (node.id, p.brush.native_session);
    assert!(p.reader_media(id, key, session, "", "", r#"{"frameReady":true}"#, 1.));
    let pixels = p.frame();
    assert_ne!(
        pixels.pixel(10, 10).unwrap().blue(),
        200,
        "gone at the first frame"
    );
}

#[test]
fn svg_posters_use_the_video_fit_when_decoded_and_painted() {
    let poster = "data:image/svg+xml,%3Csvg%20xmlns='http://www.w3.org/2000/svg'%20viewBox='0%200%20400%20100'%3E%3Cpath%20d='M0%200H400V100H0Z'/%3E%3C/svg%3E";
    let source = format!(
        r#"component App
  view
    row background-color="white"
      video src="a.mp4" poster="{poster}" width=160 height=80 testId="default"
      video src="a.mp4" poster="{poster}" width=160 height=80 object-fit="contain" testId="contain"
      video src="a.mp4" poster="{poster}" width=160 height=80 object-fit="unset" testId="clear"
"#
    );
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(&source),
        (),
        (480., 80.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.frame();
    assert!(p.wait_images(Duration::from_secs(30)));
    let decoded = ["default", "contain", "clear"].map(|name| {
        let k = p.host().kernel();
        let id = k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id;
        let bitmap = &p.images.bitmaps[&id];
        (bitmap.width(), bitmap.height())
    });
    let frame = p.frame();
    let ink = [80, 240, 400].map(|x| {
        (0..80)
            .filter(|y| frame.pixel(x, *y).unwrap().red() < 128)
            .collect::<Vec<_>>()
    });
    assert_eq!(
        (decoded, ink),
        (
            [(160, 40), (160, 40), (160, 80)],
            std::array::from_fn(|_| (20..60).collect())
        )
    );
}

/// The browser suite supplies measured SVG ink from its own table.
#[test]
#[ignore = "host/web/tests/symbol-size.test.mjs supplies Chromium's SVG measurements"]
fn template_placement_matches_browser() {
    crate::app::show_neutral_look();
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(&std::env::var("EXACT_SYMBOL_ORACLE").expect("Chromium oracle table"))
            .unwrap();
    assert!(!cases.is_empty());
    // Bound the CPU clip masks while preserving Chromium's 840 px width for vw.
    let failures: Vec<_> = cases.chunks(64).flat_map(compare_placement).collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn compare_placement(cases: &[serde_json::Value]) -> Vec<String> {
    let mut source = String::from("component App\n");
    for (i, case) in cases.iter().enumerate() {
        source.push_str(&format!("  state position{i} = {}\n", case["position"]));
    }
    source.push_str(
        "  view\n    column width=640 gap=0 background-color=\"white\" color=\"black\"\n",
    );
    for (i, case) in cases.iter().enumerate() {
        if i % 4 == 0 {
            source.push_str("      row width=640 height=80\n");
        }
        let (w, h) = (case["w"].as_f64().unwrap(), case["h"].as_f64().unwrap());
        let bottom = h * if case["overflow"] == true { 2. } else { 1. };
        let sf = case["sf"].as_str().unwrap();
        let fit = case["fit"].as_str().unwrap();
        if let Some(kind) = case["kind"].as_str() {
            let tag = if kind == "video" { "video" } else { "image" };
            let fit = if fit.is_empty() {
                String::new()
            } else {
                format!("object-fit=\"{fit}\"")
            };
            source.push_str(&format!("        {tag} \"oracle.png\" width=160 height=80 font-size={} {fit} object-position=position{i} testId=\"i{i}\"\n", case["font"].as_u64().unwrap_or(20)));
        } else {
            source.push_str(&format!("        image \"symbol:icon|{sf}|0 0 {w} {h}|M0 0H{w}V{bottom}H0Z|\" width=160 height=80 font-size={} object-fit=\"{fit}\" object-position=position{i} testId=\"i{i}\"\n", case["font"].as_u64().unwrap_or(20)));
        }
    }
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(&source),
        (),
        (840., cases.len().div_ceil(4) as f32 * 80.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    for (i, _) in cases
        .iter()
        .enumerate()
        .filter(|(_, c)| c["kind"] == "bitmap")
    {
        let k = p.host().kernel();
        let id = k
            .node_by_key(k.find_by_test_id(&format!("i{i}"))[0])
            .unwrap()
            .id;
        let mut pixels = tiny_skia::Pixmap::new(40, 20).unwrap();
        pixels.fill(tiny_skia::Color::BLACK);
        let gate = exact_raster::Gate::new();
        let charge = gate
            .session()
            .reserve_allocation(40 * 20 * 4)
            .unwrap()
            .charge();
        p.images.bitmaps.insert(
            id,
            Arc::new(crate::image::Bitmap::new(pixels, (40, 20), charge)),
        );
    }
    let frame = p.frame().clone();
    let mut failures = Vec::new();
    for (i, case) in cases.iter().enumerate() {
        let k = p.host().kernel();
        let node = k
            .node_by_key(k.find_by_test_id(&format!("i{i}"))[0])
            .unwrap();
        let env = k.env_of(node.id);
        let viewport = if case["kind"].is_string() {
            crate::paint::object_fit(
                (
                    case["w"].as_u64().unwrap() as u32,
                    case["h"].as_u64().unwrap() as u32,
                ),
                crate::paint::object_fit_of(&node),
                &node.style.object_position,
                (0., 0., 160., 80.),
                &env,
            )
        } else {
            exact_kernel::object::placed(
                crate::paint::look::symbol_natural(&node),
                node.style.object_fit,
                &node.style.object_position,
                (0., 0., 160., 80.),
                &env,
            )
        }
        .unwrap();
        let (x, y, w, h) = if case["kind"].is_string() {
            viewport
        } else {
            let path = exact_kernel::symbol_path_in(
                node.props
                    .str(exact_kernel::PropId::ImageSource)
                    .unwrap()
                    .strip_prefix("symbol:")
                    .unwrap(),
                exact_kernel::SymbolSet::Web,
            )
            .unwrap();
            let ts = crate::paint::fitted(path.view_box, viewport);
            (
                ts.tx,
                ts.ty,
                case["w"].as_f64().unwrap() as f32 * ts.sx,
                case["h"].as_f64().unwrap() as f32
                    * ts.sy
                    * if case["overflow"] == true { 2. } else { 1. },
            )
        };
        let projected = [
            x.max(viewport.0).max(0.),
            y.max(viewport.1).max(0.),
            (x + w).min(viewport.0 + viewport.2).min(160.) - 1.,
            (y + h).min(viewport.1 + viewport.3).min(80.) - 1.,
        ];
        let expected = case["expected"].as_array().unwrap()[2..]
            .iter()
            .map(|v| v.as_f64().unwrap() as f32)
            .collect::<Vec<_>>();
        let mut ink: [f32; 4] = [160., 80., -1., -1.];
        for y in 0..80 {
            for x in 0..160 {
                if frame
                    .pixel(x + i as u32 % 4 * 160, y + i as u32 / 4 * 80)
                    .unwrap()
                    .demultiply()
                    .red()
                    < 128
                {
                    ink = [
                        ink[0].min(x as f32),
                        ink[1].min(y as f32),
                        ink[2].max(x as f32),
                        ink[3].max(y as f32),
                    ];
                }
            }
        }
        if case["kind"] == "video" {
            ink = projected;
        }
        if projected
            .iter()
            .zip(&expected)
            .any(|(a, b)| (a - b).abs() > 1.)
            || ink.iter().zip(&expected).any(|(a, b)| (a - b).abs() > 1.)
        {
            failures.push(format!("{case}: geometry {projected:?}, pixels {ink:?}"));
        }
    }
    failures
}

#[derive(Default)]
struct RoleCanvas(Vec<[u8; 3]>);

impl DataSource for RoleCanvas {
    fn query(
        &mut self,
        name: &str,
        _: &[exact_runner::Value],
    ) -> Result<exact_runner::Value, exact_runner::DataError> {
        Err(exact_runner::DataError::UnknownSource(name.into()))
    }

    fn canvas_surfaces(&self) -> Vec<(String, usize)> {
        vec![("ink".into(), 0)]
    }

    fn draw(
        &mut self,
        q: &exact_runner::DrawRequest<'_>,
        ctx: &exact_canvas::Context2d,
    ) -> exact_runner::Drawn {
        let c = q.current_color.unwrap();
        assert_eq!(c.a, 1.);
        self.0.push([c.r, c.g, c.b]);
        ctx.set_fill_style_str("currentColor");
        ctx.fill_rect(0., 0., 8., 8.);
        exact_runner::Drawn::Now(exact_runner::DrawReply {
            lists: ctx.take_lists(),
            ..Default::default()
        })
    }
}

fn picture_turn_roles(turn: impl Fn(&mut Presenter<RoleCanvas>)) {
    use exact_kernel::style::{roles, ColorValue};

    let plan = crate::test_plan(
        "component App\n  view\n    canvas surface=ink() color=\"CanvasText\" width=8 height=8\n",
    );
    let boot = |color| {
        let (p, error) = Presenter::boot_with_fixture_fonts(
            &plan,
            RoleCanvas::default(),
            (8., 8.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.host.runner().roles().set(&[(
            roles::role("CanvasText").unwrap(),
            ColorValue::Fixed(exact_kernel::Color(color)),
        )]);
        p
    };
    for bound in [false, true] {
        let mut first = boot(0xff0000ff);
        let mut second = boot(0x0000ffff);
        // A caller can already have another session bound; the turn restores it.
        let _other = bound.then(|| second.host.runner().bind_roles());
        turn(&mut first);
        turn(&mut second);
        assert_eq!(first.host.runner().data_ref().0, [[255, 0, 0]]);
        assert_eq!(second.host.runner().data_ref().0, [[0, 0, 255]]);
        for (p, expected) in [(&first, [255, 0, 0, 255]), (&second, [0, 0, 255, 255])] {
            let snapshots = p.host.canvas_snapshots().borrow();
            let pixels = &snapshots.values().next().unwrap().pixels;
            let color = pixels.pixel(4, 4).unwrap().demultiply();
            assert_eq!(
                [color.red(), color.green(), color.blue(), color.alpha()],
                expected
            );
        }
        if bound {
            let role = ColorValue::Role(roles::role("CanvasText").unwrap());
            assert_eq!(role.resolve(false).0, 0x0000ffff);
        }
    }
}

#[test]
fn waiting_for_pictures_binds_the_canvases_session_roles() {
    picture_turn_roles(|p| {
        p.wait_images(Duration::ZERO);
    });
}

#[test]
fn settling_pictures_binds_the_canvases_session_roles() {
    picture_turn_roles(|p| {
        p.settle_pictures(Duration::ZERO);
    });
}

#[test]
fn polling_a_canvas_picture_binds_the_canvases_session_roles() {
    picture_turn_roles(|p| {
        p.images
            .canvas_answers
            .push(("missing.png".into(), Err("not found".into())));
        assert!(p.poll_images());
    });
}
