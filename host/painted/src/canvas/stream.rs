//! A stream's drawing apart from its definitions. A stream carries both:
//! what the frame draws (matrices, shapes, clips, references to kept rows,
//! slots and layers) and what the reader makes once and keeps (faces,
//! pictures, rows, slots' and layers' drawings, keyframes, frees). A paint
//! that draws what the last stream drew and defines nothing new (a touch
//! down that changes nothing shown, after a stream that defined rows) is no
//! new frame for the reader: no ingest, no replay, no redraw of the window.
use super::layer::{CLOCK, LAYER_FREE, LAYER_REF, LAYER_SET, TRACKS};
use super::*;
use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard};

/// Earlier frames kept by a driver, or by a stream the definition walker cannot split.
static STREAMS: Mutex<VecDeque<Vec<u32>>> = Mutex::new(VecDeque::new());

fn streams() -> MutexGuard<'static, VecDeque<Vec<u32>>> {
    STREAMS.lock().unwrap_or_else(|e| e.into_inner())
}

pub(super) fn keep(ops: Option<Vec<u32>>) {
    if let Some(ops) = ops {
        streams().push_back(ops);
    }
}

/// Forget unread streams and pictures before a new reader reuses their IDs.
#[cfg(any(target_os = "android", test))]
pub(super) fn reader_replaced() {
    FINISHED.with(|f| *f.borrow_mut() = None);
    streams().clear();
    with_pending(|p| p.clear());
}

pub(super) fn next_stream() -> Option<Vec<u32>> {
    streams().pop_front()
}

pub(super) fn waiting() -> bool {
    !streams().is_empty()
}

/// A turn can itself have queued older frames; its result follows them.
pub(super) fn after_turn(latest: Option<Vec<u32>>) -> Option<Vec<u32>> {
    let mut pending = streams();
    if pending.is_empty() {
        return latest;
    }
    pending.extend(latest);
    pending.pop_front()
}

/// Where the op at `i` ends, or `None` for a malformed stream.
pub(super) fn end(b: &[u32], i: usize) -> Option<usize> {
    let at = |k: usize| b.get(i + k).map(|v| *v as usize);
    // A path's tagged segments from its count at `p`.
    let path = |p: usize| -> Option<usize> {
        let n = *b.get(p)? as usize;
        let mut k = p + 1;
        for _ in 0..n {
            k += match *b.get(k)? {
                0 | 1 => 3,
                2 => 7,
                _ => 1,
            };
        }
        Some(k)
    };
    let words = |len: usize| len.div_ceil(4);
    Some(match *b.get(i)? {
        MATRIX => i + 7,
        MATRIX3 => i + 10,
        RRECT => i + 14,
        PATH => path(i + 3)?,
        CLIP_RRECT => i + 13,
        CLIP_PATH => path(i + 2)?,
        RESTORE | ROW_END | GROUP_END => i + 1,
        LAYER | IMAGE_FREE | ROW_FREE | GROUP_BEGIN | SLOT | LAYER_FREE | LAYER_REF => i + 2,
        IMAGE => i + 6,
        GLYPHS => i + 6 + 3 * at(5)?,
        FONT => i + 5 + words(at(4)?),
        FONT_AXES => {
            let len = i + 5 + 2 * at(4)?;
            len + 1 + words(*b.get(len)? as usize)
        }
        IMAGE_DEF => i + 4,
        STROKE => path(i + 6)?,
        IMAGE_RRECT => i + 18,
        DASH => i + 3 + at(2)?,
        clip::CLIP_UNION => i + 2 + at(1)?,
        clip::MASK_LAYER => i + 1,
        picture::IMAGE_TINT | picture::IMAGE_RENDERING => i + 2,
        gradient::SHADER => i + 2 + at(1)?,
        glyphs::BLUR => i + 2,
        filter::FILTER => i + 2 + at(1)?,
        filter::FILTER_END => i + 1,
        ANIMATED => i + 3 + words(at(2)?),
        BACKDROP => i + 14 + 2 * at(1)?,
        NATIVE => i + 16 + words(at(15)?),
        40 => i + 27, // SHADOW
        ROW_BEGIN => i + 6 + at(5)?,
        ROW_DRAW => i + 4,
        ROW_DRAW_THROUGH => i + 8,
        RING => i + 26,
        SLOT_SET | LAYER_SET | TRACKS => i + 3 + at(2)?,
        CLOCK => i + 3,
        shared::STATE | shared::SET => i + 2 + at(1)?,
        shared::DRAW => i + 3,
        _ => return None,
    })
}

/// Retained drawing that samples or contains the world must stay in HWUI.
pub(super) fn needs_world(b: &[u32]) -> bool {
    let mut i = 0;
    while i < b.len() {
        let Some(j) = end(b, i).filter(|j| *j <= b.len()) else {
            return true;
        };
        match b[i] {
            BACKDROP => return true,
            NATIVE if b[i + 2] & 255 == 3 => return true,
            ROW_BEGIN if needs_world(&b[i + 6..j - 1]) => return true,
            SLOT_SET if needs_world(&b[i + 3..j]) => return true,
            LAYER_SET if needs_world(&b[i + 20..j]) => return true,
            shared::SET | shared::DRAW => return true,
            _ => (),
        }
        i = j;
    }
    false
}

/// Whether op `op` defines something the reader keeps (rather than draws).
fn defines(op: u32) -> bool {
    matches!(
        op,
        FONT | FONT_AXES
            | IMAGE_DEF
            | IMAGE_FREE
            | ANIMATED
            | ROW_BEGIN
            | ROW_FREE
            | SLOT_SET
            | LAYER_SET
            | TRACKS
            | LAYER_FREE
            | CLOCK
            | shared::STATE
            | shared::SET
    )
}

/// The stream's drawing (its definitions left out), and whether it defines
/// anything; `None` for a stream this walk does not know (never skipped).
pub fn drawing(b: &[u32]) -> Option<(Vec<u32>, bool)> {
    let mut out = Vec::with_capacity(b.len());
    let mut defs = false;
    let mut i = 0;
    while i < b.len() {
        let j = end(b, i).filter(|j| *j <= b.len())?;
        if defines(b[i]) {
            defs = true;
        } else {
            out.extend_from_slice(&b[i..j]);
        }
        i = j;
    }
    Some((out, defs))
}

/// Carry only retained definitions into a newer frame. Unknown streams stay whole.
fn carry_definitions(older: &[u32], newer: &mut Vec<u32>) -> bool {
    // The reader captures shared sources before any earlier definitions mutate them.
    if newer.first() == Some(&shared::STATE) {
        return false;
    }
    let mut definitions = Vec::new();
    let mut row_sets = false;
    let mut i = 0;
    while i < older.len() {
        if older[i] == shared::STATE {
            return false;
        }
        let Some(j) = end(older, i).filter(|j| *j <= older.len()) else {
            return false;
        };
        if defines(older[i]) {
            row_sets = match older[i] {
                ROW_BEGIN => older[i + 2] & (1 << 31) == 0,
                SLOT_SET | LAYER_SET | TRACKS | CLOCK => row_sets,
                _ => false,
            };
            definitions.extend_from_slice(&older[i..j]);
        } else if row_sets {
            // Android defers contiguous sets after an unshown row; keep their boundary.
            return false;
        }
        i = j;
    }
    if row_sets
        && newer
            .first()
            .is_some_and(|op| matches!(*op, SLOT_SET | LAYER_SET | TRACKS | CLOCK))
    {
        return false;
    }
    if !definitions.is_empty() {
        definitions.append(newer);
        *newer = definitions;
    }
    true
}

/// What a frame's recording learns of whether its GPU canvas may be presented directly (op 28's
/// direct bit): the frame's only canvas, filling the window, with nothing composited into it.
#[derive(Default)]
pub(super) struct Direct {
    /// The window, in points.
    pub(super) size: (f32, f32),
    /// Opacity layers open where the recording is.
    pub(super) isolation: usize,
    /// Where an eligible canvas's NATIVE op starts in the recording.
    pub(super) at: Option<usize>,
    /// GPU canvases recorded.
    pub(super) surfaces: usize,
    /// Whether a backdrop filter reads what is under it.
    pub(super) backdrop: bool,
}

impl Recorder {
    pub(super) fn finish_stream(&mut self) -> Result<Pixmap, String> {
        self.ops
            .extend(self.freed_rows.drain(..).flat_map(|id| [ROW_FREE, id]));
        self.layers_finish();
        if self.direct.surfaces == 1
            && !self.direct.backdrop
            && !self
                .kept
                .values()
                .any(|(ops, _)| needs_world(ops.get(3..).unwrap_or(&[u32::MAX])))
            && !self.slots.values().any(|ops| needs_world(ops))
            && !self.layers.needs_world()
        {
            if let Some(at) = self.direct.at {
                self.ops[at + 2] |= 1 << 8;
            }
        }
        let mut ops = std::mem::take(&mut self.ops);
        FINISHED.with(|f| {
            let mut pending = f.borrow_mut();
            let old = pending.take();
            #[cfg(target_os = "android")]
            let driven = agent::driven();
            #[cfg(not(target_os = "android"))]
            let driven = false;
            if let Some(old) = old {
                if driven || !carry_definitions(&old, &mut ops) {
                    keep(Some(old));
                }
            }
            *pending = Some(ops);
        });
        Pixmap::new(1, 1).ok_or_else(|| "placeholder".to_string())
    }
}

#[cfg(test)]
#[path = "bleed_tests.rs"]
mod bleed_tests;

#[cfg(test)]
#[path = "scroll_stream_tests.rs"]
mod scroll_stream_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sticky_heading_keeps_its_section_background_for_the_backdrop() {
        use crate::presenter::{PainterChoice, Presenter};

        let (mut p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan(
                r#"component App
  view
    scroll width=200 height=100
      column
        box width=200 height=5
        column width=200 background-color="blue"
          box position="sticky" top=0 width=200 height=20 backdrop-filter="saturate(0)"
          box width=200 height=150
          box width=200 height=25
"#,
            ),
            (),
            (200., 120.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Canvas,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.frame();
        let ops = FINISHED.with(|f| f.borrow_mut().take()).expect("a frame");
        let (mut row, mut background, mut backdrop) = (None, None, None);
        let mut at = 0;
        while at < ops.len() {
            match ops[at] {
                ROW_BEGIN => {
                    row = Some(ops[at + 1]);
                    at += 6;
                    continue;
                }
                ROW_END => row = None,
                RRECT if ops[at + 1] == 0xff0000ff => background = row,
                BACKDROP => backdrop = row,
                _ => {}
            }
            at = end(&ops, at).expect("a valid stream");
        }
        assert!(
            backdrop.is_some(),
            "the reader draws the heading's backdrop"
        );
        assert_eq!(backdrop, background, "the reader samples only its own row");
        let section = backdrop.unwrap();
        for _ in 0..2 {
            p.repaint();
            p.frame();
            let ops = FINISHED.with(|f| f.borrow_mut().take()).expect("a frame");
            let mut at = 0;
            let mut drawn = false;
            while at < ops.len() {
                assert_ne!(ops[at], ROW_BEGIN, "an unchanged section keeps its row");
                drawn |= ops[at] == ROW_DRAW && ops[at + 1] == section;
                at = end(&ops, at).expect("a valid stream");
            }
            assert!(drawn, "the reader still draws the section");
        }
    }

    #[test]
    fn memory_trim_frees_canvas_rows_without_freeing_their_replacements() {
        trimmed_rows("memory");
    }

    #[test]
    fn hidden_trim_frees_canvas_rows_without_freeing_their_replacements() {
        trimmed_rows("hidden");
    }

    fn trimmed_rows(trim: &str) {
        use crate::presenter::{PainterChoice, Presenter};
        use std::collections::HashSet;

        for trim in ["none", trim] {
            for remove in [true, false] {
                let (mut p, error) = Presenter::boot_with_fixture_fonts(
                    &crate::test_plan(
                        r##"component App
  state visible = true
  action remove
    visible = false
  view
    column
      box testId="remove" press=remove width=40 height=20
      if visible
        scroll width=200 height=200
          column
            box width=200 height=50 background-color="#123456"
            box width=200 height=50 background-color="#abcdef"
"##,
                    ),
                    (),
                    (240., 240.),
                    1.,
                    std::path::PathBuf::new(),
                    PainterChoice::Canvas,
                )
                .unwrap();
                assert!(error.is_none(), "{error:?}");
                let mut rows = HashSet::new();
                let mut freeing = Vec::new();
                let mut frame = |p: &mut Presenter<()>| {
                    p.frame();
                    let ops = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
                    // ExactView applies ROW_FREE when the following stream arrives.
                    for id in freeing.drain(..) {
                        assert!(
                            rows.remove(&id),
                            "{trim}, remove={remove}: unknown row {id}"
                        );
                    }
                    let mut at = 0;
                    while at < ops.len() {
                        match ops[at] {
                            ROW_BEGIN => {
                                assert!(rows.insert(ops[at + 1]), "a live row ID was reused");
                            }
                            ROW_FREE => freeing.push(ops[at + 1]),
                            ROW_DRAW => assert!(
                                rows.contains(&ops[at + 1]),
                                "{trim}, remove={remove}: drawing a freed row"
                            ),
                            _ => (),
                        }
                        at = end(&ops, at).expect("valid Canvas stream");
                    }
                    (rows.len(), freeing.len())
                };
                assert_eq!(frame(&mut p), (2, 0));
                match trim {
                    "memory" => p.memory_warning(),
                    "hidden" => {
                        let page = p.host().runner().page();
                        assert!(p
                            .set_page(exact_runner::Page {
                                hidden: true,
                                ..page
                            })
                            .is_none());
                        assert!(p.set_page(page).is_none());
                    }
                    _ => (),
                }
                if remove {
                    let k = p.host().kernel();
                    let id = k.node_by_key(k.find_by_test_id("remove")[0]).unwrap().id;
                    assert!(p
                        .host_mut()
                        .dispatch_at(id, exact_runner::Event::Press, 0.)
                        .is_none());
                    assert!(p.after_commit().is_none());
                }
                let trimmed = trim != "none";
                assert_eq!(
                    frame(&mut p),
                    (
                        if trimmed && !remove { 4 } else { 2 },
                        if trimmed || remove { 2 } else { 0 }
                    ),
                    "{trim}, remove={remove}: row frees must reach the reader"
                );
                assert_eq!(frame(&mut p), (if remove { 0 } else { 2 }, 0));
                assert_eq!(frame(&mut p), (if remove { 0 } else { 2 }, 0));
            }
        }
    }

    #[test]
    fn entering_shared_state_after_an_ordinary_frame_preserves_both_boundaries() {
        let old = vec![IMAGE_DEF, 7, 10, 10, IMAGE, 7, 0, 0, 10, 10];
        let state = vec![shared::STATE, 5, 0, 0, 1, 9, 0, shared::DRAW, 9, 0];
        let mut new = state.clone();
        assert!(!carry_definitions(&old, &mut new));
        assert_eq!(
            new, state,
            "STATE must remain first before reader mutations"
        );
    }

    fn media_records(ops: &[u32]) -> Vec<(f32, f32, serde_json::Value)> {
        let mut players = Vec::new();
        let mut at = 0;
        while at < ops.len() {
            if ops[at] == NATIVE && ops[at + 2] == 0 {
                let len = ops[at + 15] as usize;
                let bytes: Vec<_> = ops[at + 16..at + 16 + len.div_ceil(4)]
                    .iter()
                    .flat_map(|word| word.to_le_bytes())
                    .take(len)
                    .collect();
                let props = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap();
                players.push((
                    f32::from_bits(ops[at + 5]),
                    f32::from_bits(ops[at + 6]),
                    props,
                ));
            }
            at = end(ops, at).expect("valid Canvas stream");
        }
        players
    }

    #[test]
    fn boxless_audio_is_recorded_without_a_box() {
        use crate::presenter::{PainterChoice, Presenter};

        let source = r#"component App
  view
    column width=160 height=160 overflow="hidden"
      audio "assets/radio.mp3" autoplay=true
      audio "assets/hidden-controls.mp3" controls=true width=0 height=0 visibility="hidden" paused=false
      column display="none"
        column
          audio "assets/hidden.mp3" controls=true autoplay=true width=60 height=40
          video "assets/hidden.mp4" width=60 height=40
      column opacity=0
        column opacity=0
          audio "assets/transparent.mp3" autoplay=true
          video "assets/transparent.mp4" width=60 height=40
      column rotate="y 30deg" width=60 height=40
        audio "assets/rotated.mp3" paused=false
      column rotate="y 30deg" width=60 height=0
        audio "assets/flat.mp3" paused=false
      column filter="blur(2px)" width=60 height=40
        audio "assets/filtered.mp3" paused=false
      video "assets/visible.mp4" width=60 height=40
"#;
        let (mut p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan(source),
            (),
            (200., 200.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Canvas,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.frame();
        let ops = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
        let players = media_records(&ops);
        assert_eq!(players.len(), 8, "{players:?}");
        let player = |src: &str| {
            players
                .iter()
                .find(|(_, _, props)| props["src"] == src)
                .unwrap_or_else(|| panic!("missing {src}: {players:?}"))
        };
        let (w, h, audio) = player("assets/radio.mp3");
        assert_eq!((*w, *h), (0., 0.));
        assert_eq!(audio["audio"], true);
        assert_eq!(audio["controls"], false);
        assert_eq!(audio["autoplay"], true);
        let (w, h, hidden_controls) = player("assets/hidden-controls.mp3");
        assert_eq!((*w, *h), (0., 0.));
        assert_eq!(hidden_controls["controls"], true);
        assert_eq!(hidden_controls["paused"], false);
        let (w, h, hidden) = player("assets/hidden.mp3");
        assert_eq!((*w, *h), (0., 0.));
        assert_eq!(hidden["controls"], true);
        let (w, h, transparent) = player("assets/transparent.mp3");
        assert_eq!((*w, *h), (0., 0.));
        assert_eq!(transparent["audio"], true);
        for src in [
            "assets/rotated.mp3",
            "assets/flat.mp3",
            "assets/filtered.mp3",
        ] {
            let (w, h, audio) = player(src);
            assert_eq!((*w, *h), (0., 0.));
            assert_eq!(audio["audio"], true);
            assert_eq!(audio["paused"], false);
            assert_eq!(audio["session"], transparent["session"]);
        }
        let (w, h, video) = player("assets/visible.mp4");
        assert_eq!((*w, *h), (60., 40.));
        assert_eq!(video["audio"], false);
    }

    /// LLP 1013.002 D5: a hidden mover's video is named held, so the reader
    /// keeps its player, whether it hid at boot or when its destination went;
    /// its audio is a boxless one.
    #[test]
    fn a_hidden_movers_video_is_held() {
        use crate::presenter::{PainterChoice, Presenter};

        let source = r#"component App
  state tab = "card"
  action leave
    tab = "other"
  view
    column width=160 height=160
      button "Leave" press=leave testId="leave"
      if tab == "card"
        box id="card" width=80 height=60
      column portal="nowhere"
        video "assets/boot.mp4" autoplay=true muted=true width=60 height=40
        audio "assets/boot.mp3" autoplay=true
      column portal="card"
        video "assets/moved.mp4" width=60 height=40
"#;
        let (mut p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan(source),
            (),
            (200., 200.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Canvas,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let players = |p: &mut Presenter<()>| {
            p.frame();
            let ops = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
            let mut named = Vec::new();
            let mut at = 0;
            while at < ops.len() {
                if ops[at] == NATIVE && ops[at + 2] & 255 == 0 {
                    let len = ops[at + 15] as usize;
                    let bytes: Vec<_> = ops[at + 16..at + 16 + len.div_ceil(4)]
                        .iter()
                        .flat_map(|word| word.to_le_bytes())
                        .take(len)
                        .collect();
                    let props = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap();
                    let src = props["src"].as_str().unwrap().to_owned();
                    named.push((src, ops[at + 2] >> 9, f32::from_bits(ops[at + 5])));
                }
                at = end(&ops, at).expect("valid Canvas stream");
            }
            named.sort_by(|a, b| a.0.cmp(&b.0));
            named
        };
        let named = |src: &str, held: u32, w: f32| (src.to_owned(), held, w);
        assert_eq!(
            players(&mut p),
            [
                named("assets/boot.mp3", 0, 0.),
                named("assets/boot.mp4", 1, 0.),
                named("assets/moved.mp4", 0, 60.),
            ]
        );
        let kernel = p.host().kernel();
        let leave = kernel
            .node_by_key(kernel.find_by_test_id("leave")[0])
            .unwrap()
            .id;
        p.tap(leave).unwrap();
        assert_eq!(
            players(&mut p),
            [
                named("assets/boot.mp3", 0, 0.),
                named("assets/boot.mp4", 1, 0.),
                named("assets/moved.mp4", 1, 0.),
            ]
        );
    }

    #[test]
    fn boxless_audio_plays_from_a_closed_dialog() {
        use crate::presenter::{PainterChoice, Presenter};

        let source = r#"component App
  state paused = true
  action play
    paused = false
  view
    column
      button "Play" press=play testId="play"
      dialog id="station"
        audio "assets/radio.mp3" paused=paused
        button "Close" commandfor="station" command="close" testId="close"
"#;
        let (mut p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan(source),
            (),
            (200., 200.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Canvas,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let id = |p: &Presenter<()>, name| {
            let kernel = p.host().kernel();
            kernel
                .node_by_key(kernel.find_by_test_id(name)[0])
                .unwrap()
                .id
        };
        let play = id(&p, "play");
        let close = id(&p, "close");
        for paused in [true, false] {
            if !paused {
                p.tap(play).unwrap();
            }
            p.frame();
            let ops = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
            let players = media_records(&ops);
            assert_eq!(players.len(), 1, "{players:?}");
            let (w, h, audio) = &players[0];
            assert_eq!((*w, *h), (0., 0.));
            assert_eq!(audio["src"], "assets/radio.mp3");
            assert_eq!(audio["paused"], paused);
            assert!(p.tap(close).is_err(), "the dialog's controls are hidden");
        }
    }

    #[test]
    fn boxless_audio_survives_a_composite_canvas() {
        use crate::paint::{Painter, Presented, Scene};
        use crate::presenter::{PainterChoice, Presenter};

        let source = r#"component App
  view
    column
      canvas testId="outer" width=160 height=160
        canvas testId="inner" width=100 height=100
          audio "assets/radio.mp3" paused=false
      canvas testId="empty" width=160 height=0
        audio "assets/empty.mp3" paused=false
"#;
        let (p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan(source),
            (),
            (200., 200.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let kernel = p.host().kernel();
        let canvases = ["outer", "inner", "empty"].map(|name| {
            kernel
                .node_by_key(kernel.find_by_test_id(name)[0])
                .unwrap()
                .id
        });
        let mut painter = Painter::new(p.text().clone(), 1., Box::new(Recorder::new()));
        painter.composites.canvases.extend(canvases);
        let scene = Scene {
            kernel,
            roots: &kernel.roots(),
            hidden: &|_| false,
            presented: &|_| Presented::IDENTITY,
            scaled_presented: &Default::default(),
            paths: &|_| None,
            filters: &|_| None,
            scroll: &Default::default(),
            page: (0., 0.),
            images: &Default::default(),
            focus: None,
            selection: None,
            caret_upstream: false,
            pointer: None,
            controls: &Default::default(),
            chosen: &Default::default(),
            menu: None,
        };
        painter.paint(&scene, (200., 200.)).unwrap();
        let ops = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
        let players = media_records(&ops);
        assert_eq!(players.len(), 2, "{players:?}");
        for ((w, h, audio), src) in players.iter().zip(["assets/radio.mp3", "assets/empty.mp3"]) {
            assert_eq!((*w, *h), (0., 0.));
            assert_eq!(audio["src"], src);
            assert_eq!(audio["paused"], false);
            assert_eq!(audio["session"], painter.native_session);
        }
    }

    #[test]
    fn iframe_sandbox_and_allocation_survive_the_canvas_stream() {
        use crate::presenter::{PainterChoice, Presenter};

        for sandbox in [None, Some(""), Some("allow-scripts")] {
            let attribute = sandbox.map_or(String::new(), |s| format!(" sandbox=\"{s}\""));
            let source = format!(
                "component App\n  view\n    iframe src=\"https://example.com/\" width=100 height=100 testId=\"frame\"{attribute}\n"
            );
            let (mut p, error) = Presenter::boot_with_fixture_fonts(
                &crate::test_plan(&source),
                (),
                (200., 200.),
                1.,
                std::path::PathBuf::new(),
                PainterChoice::Canvas,
            )
            .unwrap();
            assert!(error.is_none(), "{error:?}");
            p.frame();
            let ops = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
            let mut at = 0;
            let mut frames = Vec::new();
            while at < ops.len() {
                if ops[at] == NATIVE && ops[at + 2] == 1 {
                    let len = ops[at + 15] as usize;
                    let bytes: Vec<_> = ops[at + 16..at + 16 + len.div_ceil(4)]
                        .iter()
                        .flat_map(|word| word.to_le_bytes())
                        .take(len)
                        .collect();
                    frames.push(serde_json::from_slice::<serde_json::Value>(&bytes).unwrap());
                }
                at = end(&ops, at).expect("valid Canvas stream");
            }
            assert_eq!(frames.len(), 1);
            assert_eq!(frames[0]["src"], "https://example.com/");
            assert_eq!(frames[0].get("sandbox"), Some(&serde_json::json!(sandbox)));
            let key = p.host().kernel().find_by_test_id("frame")[0];
            assert_eq!(frames[0]["slot"], key.index);
            assert_eq!(frames[0]["generation"], key.generation);
            assert_eq!(frames[0]["session"], p.brush.native_session);
        }
    }

    #[test]
    fn cached_route_headings_follow_focus_and_touch_mode() {
        use crate::presenter::{PainterChoice, PointerKind, Presenter};

        let source = r##"routes nav
  tab home "/"
    detail "/detail"
component App
  action open
    nav = push(nav, "/detail")
  action back
    nav = back(nav)
  view
    main navigationKey=`${top(nav).id}` navigationBack="back" width=400 height=500 accent-color="#ed1234"
      for t in nav.tabs key = t.name
        for e in t.stack key = e.id
          column navigationKey=`${e.id}` position="absolute" inset=0
            if e.name == "home"
              button testId="open" press=open width=100 height=40
                text "Open"
            if e.name == "detail"
              header overflow-y="auto" width=200 height=120
                text "Detail" role="heading" testId="title" width=160 height=50
                box tabindex=-1 testId="other" width=160 height=50
              button id="back" press=back width=100 height=40
                text "Back"
"##;
        let (mut p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan(source),
            (),
            (400., 500.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Canvas,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        (p.brush.touch_mode, p.brush.pointer_modality) = (true, true);
        p.set_pointer_kind(PointerKind::Touch);
        let id = |p: &Presenter<()>, name: &str| {
            let k = p.host().kernel();
            k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
        };
        let mut rows = HashMap::new();
        let mut frame = |p: &mut Presenter<()>| {
            p.frame();
            let ops = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
            let (mut at, mut rings, mut recorded) = (0, 0, 0);
            while at < ops.len() {
                let next = end(&ops, at).expect("valid Canvas stream");
                match ops[at] {
                    ROW_BEGIN => {
                        let mut i = at + 6;
                        let mut count = 0;
                        while i < next {
                            count += usize::from(ops[i] == RING && ops[i + 1] == 0xffed1234);
                            i = end(&ops, i).expect("valid row");
                        }
                        rows.insert(ops[at + 1], count);
                        recorded += 1;
                    }
                    ROW_DRAW => rings += rows[&ops[at + 1]],
                    ROW_FREE => {
                        rows.remove(&ops[at + 1]);
                    }
                    RING if ops[at + 1] == 0xffed1234 => rings += 1,
                    _ => {}
                }
                at = next;
            }
            (rings, recorded)
        };
        frame(&mut p);
        p.tap(id(&p, "open")).unwrap();
        let title = id(&p, "title");
        assert_eq!(p.focus(), Some(title));
        assert_eq!(frame(&mut p).0, 0, "touch arrival leaves no ring");
        assert_eq!(frame(&mut p), (0, 0), "the touch frame is kept");
        p.key_down("Shift", p.host().now());
        assert_eq!(frame(&mut p), (1, 1), "Shift records the heading's ring");
        assert_eq!(frame(&mut p), (1, 0), "the ring frame is kept");
        let other = id(&p, "other");
        assert!(p.set_focus(Some(other), p.host().now()).is_none());
        assert_eq!(frame(&mut p), (1, 2), "both focus rows change");
        assert_eq!(frame(&mut p), (1, 0));
        assert!(p.set_focus(None, p.host().now()).is_none());
        assert_eq!(frame(&mut p), (0, 1), "blur removes the kept ring");
        assert!(p.set_focus(Some(title), p.host().now()).is_none());
        assert_eq!(frame(&mut p), (1, 1));
        let now = p.host().now();
        p.pointer_down(300., 400., now).unwrap();
        assert_eq!(frame(&mut p), (0, 1), "touch removes the kept ring");
        p.pointer_up(300., 400., now + 1.).unwrap();
        assert_eq!(frame(&mut p), (0, 0));
    }

    #[test]
    fn a_hosted_hit_names_the_native_contents_painted_origin() {
        use crate::presenter::{PainterChoice, Presenter};
        for scale in [1., 2., 2.75] {
            let (mut p, error) = Presenter::boot_with_fixture_fonts(
                &crate::test_plan(r##"component App
  view
    view margin=10.25 padding=12 width=350 height=350
      test-map testId="map" width=200 height=200 padding=20 border-width=3 border-style="solid" translate="4px 7px"
"##),
                (), (400., 400.), scale, std::path::PathBuf::new(), PainterChoice::Canvas,
            ).unwrap();
            assert!(error.is_none(), "{error:?}");
            p.frame();
            let ops = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
            let mut matrix = [1., 0., 0., 1., 0., 0.];
            let mut native = None;
            let mut at = 0;
            while at < ops.len() {
                if ops[at] == MATRIX {
                    matrix = std::array::from_fn(|i| f32::from_bits(ops[at + 1 + i]));
                }
                if ops[at] == NATIVE {
                    let rect: [f32; 4] = std::array::from_fn(|i| f32::from_bits(ops[at + 3 + i]));
                    let x = matrix[0] * rect[0] + matrix[2] * rect[1] + matrix[4];
                    let y = matrix[1] * rect[0] + matrix[3] * rect[1] + matrix[5];
                    native = Some((ops[at + 1], x, y, rect[2], rect[3]));
                }
                at = end(&ops, at).unwrap();
            }
            let (id, x, y, width, height) = native.expect("the module's op 28");
            // Its border box ends on a whole pixel, so at 2.75 its content is half a pixel wider.
            let grown = if scale == 2.75 { 0.5 / 2.75 } else { 0. };
            assert_eq!((width, height), (200. + grown, 200. + grown));
            let words = p.reader_module(x + 5. * scale, y + 5. * scale, scale);
            assert_eq!(words[0], id as i32);
            let origin = [words[10], words[11]].map(|v| f32::from_bits(v as u32));
            assert_eq!(origin, [(x + 0.5).floor(), (y + 0.5).floor()]);
            let border = [words[1], words[2]].map(|v| f32::from_bits(v as u32));
            assert!((x - border[0] - 23. * scale).abs() < 0.01);
            assert!((y - border[1] - 23. * scale).abs() < 0.01);
        }
    }

    #[test]
    fn turned_boxes_record_shapes_without_cpu_pictures() {
        use crate::presenter::{PainterChoice, Presenter};
        let (mut p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan("component App\n  view\n    view perspective=600 width=200 height=100 margin=100\n      view rotate=\"y 40deg\" width=200 height=100 background-color=\"#ff0000\"\n        box width=0.1 height=2 background-color=\"blue\"\n        text \"Turned card\" font-size=18 color=\"white\" margin=12\n"),
            (), (400., 400.), 3., std::path::PathBuf::new(), PainterChoice::Canvas,
        ).unwrap();
        assert!(error.is_none(), "{error:?}");
        p.frame();
        let ops = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
        let mut at = 0;
        let mut shapes = 0;
        let mut glyphs = 0;
        let mut planes = 0;
        let mut divider = None;
        while at < ops.len() {
            assert_ne!(
                ops[at], IMAGE_DEF,
                "a turned solid box must not rasterize a CPU picture"
            );
            shapes += usize::from(ops[at] == RRECT);
            if ops[at] == RRECT && f32::from_bits(ops[at + 5]) == 2. {
                divider = Some(f32::from_bits(ops[at + 4]));
            }
            glyphs += usize::from(ops[at] == GLYPHS);
            planes += usize::from(ops[at] == MATRIX3);
            at = end(&ops, at).expect("valid stream");
        }
        assert!(shapes > 0, "the turned box records its fill");
        assert!(glyphs > 0, "the turned box records its text");
        assert!(planes > 0, "the draws use a projective matrix");
        assert_eq!(divider, Some(0.1));
    }

    #[test]
    fn a_turned_software_island_preserves_the_layout_box() {
        let alpha = software_island_edge("", 248);
        assert!((50..100).contains(&alpha), "edge coverage: {alpha}");
    }

    #[test]
    fn a_nested_software_island_keeps_its_fractional_edge_pixels() {
        let alpha = software_island_edge("rotate=\"x 5deg\" overflow=\"hidden\"", 256);
        assert!((50..100).contains(&alpha), "edge coverage: {alpha}");
    }

    #[test]
    fn a_translated_row_paints_a_software_child_scrolled_into_view() {
        let mut p = canvas_fixture(
            r#"component App
  view
    scroll width=200 height=80 scrollTop=200 translate="1px 0px"
      column
        box height=400 width=200 position="relative"
          box position="absolute" top=250 width=10 height=10 background-color="red" filter="brightness(1)" translate="0px 0px 1px"
"#,
            1.,
        );
        p.frame();
        let image = software_picture();
        assert_eq!(image.pixel(5, 5).unwrap().red(), 255);
    }

    #[test]
    fn a_nested_warp_paints_the_last_column_of_its_backing() {
        let mut p = canvas_fixture(
            r#"component App
  view
    box scale=2 transform-origin="0 0"
      box width=100.375 height=20 position="relative" overflow="hidden" scale=0.5 transform-origin="0 0" translate="0px 0px 1px"
        box position="absolute" left=100 top=4 width=0.375 height=8 translate="0px 0px 1px" background-color="red"
"#,
            2.,
        );
        p.frame();
        let image = software_picture();
        let y = 12;
        assert_eq!(image.width(), 201);
        assert!((0..200).all(|x| image.pixel(x, y).unwrap().alpha() == 0));
        assert!(image.pixel(200, y).unwrap().red() > 0);
    }

    #[test]
    fn a_software_island_hits_the_child_at_its_painted_left_edge() {
        let mut p = canvas_fixture(
            r#"component App
  view
    box width=40 height=20 margin-left=4.75 translate="0px 0px 1px" filter="brightness(1)"
      box width=10 height=20 background-color="red" testId="child"
"#,
            2.625,
        );
        p.frame();
        let image = software_picture();
        assert_eq!(image.pixel(2, 10).unwrap().red(), 255);
        let k = p.host().kernel();
        let child = k.node_by_key(k.find_by_test_id("child")[0]).unwrap().id;
        assert_eq!(p.hit(4.76, 5.), Some(child));
        assert_ne!(p.hit(4.74, 5.), Some(child));
    }

    fn canvas_fixture(source: &str, scale: f32) -> crate::presenter::Presenter<()> {
        use crate::presenter::{PainterChoice, Presenter};
        let (p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan(source),
            (),
            (220., 80.),
            scale,
            std::path::PathBuf::new(),
            PainterChoice::Canvas,
        )
        .unwrap();
        assert_eq!(error, None);
        p
    }

    fn software_picture() -> Arc<Pixmap> {
        let ops = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
        let mut at = 0;
        while at < ops.len() {
            match ops[at] {
                ROW_BEGIN => at += 6,
                IMAGE_DEF => match with_pending(|p| p.remove(&ops[at + 1])) {
                    Some(Picture::Canvas(pixels)) => return pixels,
                    _ => panic!("software pixels"),
                },
                _ => at = end(&ops, at).unwrap(),
            }
        }
        panic!("the software island must emit a picture");
    }

    #[test]
    fn a_scaled_background_and_its_hit_target_share_the_layout_width() {
        let mut p = canvas_fixture(
            r#"component App
  state count = 0
  action go
    count = count + 1
  view
    box width=0.1 height=2 scale=8 transform-origin="0 0" background-color="red" press=go testId="slab"
"#,
            2.625,
        );
        p.frame();
        let k = p.host().kernel();
        let slab = k.node_by_key(k.find_by_test_id("slab")[0]).unwrap().id;
        let ops = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
        let mut at = 0;
        let mut scale = 0.;
        let mut width = None;
        while at < ops.len() {
            match ops[at] {
                MATRIX => scale = f32::from_bits(ops[at + 1]),
                RRECT if ops[at + 1] == 0xffff0000 => {
                    width = Some(scale * f32::from_bits(ops[at + 4]));
                }
                _ => {}
            }
            at = end(&ops, at).unwrap();
        }
        assert!((width.unwrap() - 2.1).abs() < 0.0001);
        assert_eq!(p.hit(2.05 / 2.625, 1.), Some(slab));
        assert_ne!(p.hit(2.15 / 2.625, 1.), Some(slab));
    }

    #[test]
    fn a_scaled_surface_mask_and_clips_follow_its_contents() {
        let mut p = canvas_fixture(
            r#"component App
  view
    box width=40 height=20 margin-left=4 scale=4 transform-origin="0 0" overflow="hidden" background-color="red" mask-image="linear-gradient(black, transparent)" clip-path="inset(0px)"
      box width=44 height=10 background-color="blue" testId="child"
"#,
            2.625,
        );
        p.frame();
        let k = p.host().kernel();
        let child = k.node_by_key(k.find_by_test_id("child")[0]).unwrap().id;
        let ops = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
        let mut at = 0;
        let mut matrix = (0., 0.);
        let mut lefts = Vec::new();
        let mut masking = false;
        while at < ops.len() {
            let float = |i| f32::from_bits(ops[at + i]);
            match ops[at] {
                MATRIX => matrix = (float(1), float(5)),
                RRECT if masking || matches!(ops[at + 1], 0xffff0000 | 0xff0000ff) => {
                    lefts.push(matrix.0 * float(2) + matrix.1);
                }
                CLIP_RRECT => lefts.push(matrix.0 * float(1) + matrix.1),
                CLIP_PATH => lefts.push(matrix.0 * float(4) + matrix.1),
                clip::MASK_LAYER => masking = true,
                _ => {}
            }
            at = end(&ops, at).unwrap();
        }
        assert_eq!(lefts, [10.5; 5]);
        assert_eq!(p.hit(4.1, 5.), Some(child));
        assert_ne!(p.hit(164.1, 5.), Some(child));
    }

    fn software_island_edge(outer: &str, x: usize) -> u8 {
        use crate::presenter::{PainterChoice, Presenter};
        let source = r#"component App
  view
    box width=200 height=120 padding-left=0.195 perspective=600
      box width=160 height=100 OUTER
        box width=100.37 height=40 rotate="y 20deg" filter="brightness(1)" testId="turned"
          box width=100.37 height=40 background-color="red"
"#;
        let paint = |plan: &[u8], choice| {
            let (mut p, error) = Presenter::boot_with_fixture_fonts(
                plan,
                (),
                (200., 120.),
                2.625,
                std::path::PathBuf::new(),
                choice,
            )
            .unwrap();
            assert!(error.is_none(), "{error:?}");
            p.frame();
            let k = p.host().kernel();
            let node = k.node_by_key(k.find_by_test_id("turned")[0]).unwrap();
            assert_eq!((node.frame.x, node.frame.width), (0.195, 100.37));
            p.rect_of(node.id).unwrap()
        };
        let plan = crate::test_plan(&source.replace("OUTER", outer));
        assert_eq!(
            paint(&plan, PainterChoice::Canvas),
            paint(&plan, PainterChoice::Cpu)
        );
        let ops = FINISHED.with(|f| f.borrow_mut().take()).unwrap();
        let mut at = 0;
        while at < ops.len() {
            if ops[at] == IMAGE_DEF {
                let picture = with_pending(|p| p.remove(&ops[at + 1])).unwrap();
                let width = picture.width() as usize;
                let pixels = picture.as_ref().as_chunks::<4>().0;
                let y = 52;
                let pixel = pixels[y * width..][..width]
                    .get(x)
                    .copied()
                    .unwrap_or([0; 4]);
                assert_eq!(pixel, [pixel[3], 0, 0, pixel[3]]);
                return pixel[3];
            }
            at = end(&ops, at).unwrap();
        }
        panic!("the filter takes the software path");
    }

    #[test]
    fn rows_first_walked_at_zero_opacity_are_recorded_when_shown() {
        use crate::presenter::{PainterChoice, Presenter};

        fn rows() -> (Vec<u32>, Vec<u32>) {
            let ops = FINISHED
                .with(|f| f.borrow_mut().take())
                .expect("finished frame");
            let (mut recorded, mut drawn) = (Vec::new(), Vec::new());
            let mut at = 0;
            while at < ops.len() {
                match ops[at] {
                    ROW_BEGIN => recorded.push(ops[at + 1]),
                    ROW_DRAW => drawn.push(ops[at + 1]),
                    _ => {}
                }
                at = end(&ops, at).expect("valid stream");
            }
            (recorded, drawn)
        }

        let (mut p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan(
                r##"component App
  state shown = false
  action toggle
    shown = !shown
  view
    column
      button "Show" press=toggle testId="show" height=40
      box opacity=(shown ? 1 : 0) width=300 height=300
        scroll width=200 height=200
          column
            box height=50 width=200 background-color="#2468ac" testId="row"
            box height=50 width=200 background-color="#2468ac"
            box height=50 width=200 background-color="#2468ac"
"##,
            ),
            (),
            (400., 500.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.brush.replace_backend(Box::new(Recorder::new()));
        let k = p.host.kernel();
        let show = k.node_by_key(k.find_by_test_id("show")[0]).unwrap().id;
        let row = k.node_by_key(k.find_by_test_id("row")[0]).unwrap().id;
        for _ in 0..2 {
            p.frame();
            assert_eq!(rows(), (vec![], vec![]), "transparent rows draw nothing");
            assert!(p.boxes().iter().any(|b| b.id == row && b.pointer_hit));
            p.tap(show).unwrap();
            p.frame();
            let (recorded, drawn) = rows();
            assert_eq!(recorded.len(), 3, "each revealed row must be recorded");
            assert!(recorded.iter().all(|id| *id != 0));
            assert_eq!(drawn, recorded, "draw the recordings just defined");
            p.repaint();
            p.frame();
            assert_eq!(rows(), (vec![], drawn), "unchanged rows replay");
            p.tap(show).unwrap();
        }
    }

    #[test]
    fn definitions_apart() {
        let row = [ROW_BEGIN, 7, 0, 0, 0, 2, RESTORE, ROW_END];
        let mut with = vec![MATRIX, 0, 0, 0, 0, 0, 0];
        with.extend(row);
        with.extend([ROW_DRAW, 7, 0, 0, TRACKS, 3, 1, 9]);
        let (d, defs) = drawing(&with).expect("known");
        assert!(defs);
        assert_eq!(d, vec![MATRIX, 0, 0, 0, 0, 0, 0, ROW_DRAW, 7, 0, 0]);
        let (again, defs) = drawing(&d).expect("known");
        assert!(!defs);
        assert_eq!(again, d);
        assert!(drawing(&[99]).is_none());
        assert!(drawing(&[GLYPHS, 1, 0, 0, 0, 5]).is_none());
    }

    #[test]
    fn ordered_backdrops_keep_the_following_drawing_aligned() {
        use exact_kernel::style::{BackdropFilter, BackdropOp};
        let mut recorder = Recorder::new();
        let shape = Shape::new((0.0, 0.0, 40.0, 30.0), [4.0; 4]);
        let filter = BackdropFilter(vec![BackdropOp::Saturate(1.8), BackdropOp::Blur(4.0)]);
        recorder.backdrop_filter(&shape, &filter, Transform::identity());
        let backdrop_end = recorder.ops.len();
        assert!(drawing(&recorder.ops[..backdrop_end - 1]).is_none());
        recorder.fill(&shape, [255, 0, 0, 255], Transform::identity());
        let (ops, definitions) = drawing(&recorder.ops).expect("backdrop followed by fill");
        assert!(!definitions);
        assert_eq!(ops, recorder.ops);
        assert_eq!(ops[backdrop_end], RRECT);
        // Radius-only readers cannot silently misinterpret the list's count.
        assert_ne!(BACKDROP, 27);
        assert!(drawing(&[27, 0]).is_none());
    }

    #[test]
    fn a_face_with_axes_is_a_definition() {
        let ital = u32::from_be_bytes(*b"ital");
        // Key 1, index 0, weight 400, one axis, then a five-byte path.
        let face = [FONT_AXES, 1, 0, 400, 1, ital, 1f32.to_bits(), 5, 0, 0];
        let glyphs = [GLYPHS, 1, 0, 0, 0, 1, 7, 0, 0];
        let stream: Vec<u32> = face.iter().chain(&glyphs).copied().collect();
        let (d, defs) = drawing(&stream).expect("known");
        assert!(defs);
        assert_eq!(d, glyphs);
        assert!(drawing(&face[..8]).is_none(), "a cut path is malformed");
    }

    #[test]
    fn every_retained_definition_precedes_the_newer_drawing() {
        let mut definitions = vec![FONT, 1, 0, 400, 0, IMAGE_DEF, 2, 10, 10];
        definitions.extend([FONT_AXES, 3, 0, 400, 0, 0]);
        definitions.extend([ANIMATED, 2, 0, ROW_BEGIN, 4, 0, 10, 10, 2, RESTORE, ROW_END]);
        definitions.extend([SLOT_SET, 5, 2, SLOT, 2, LAYER_SET, 6, 17]);
        definitions.extend([0; 17]);
        definitions.extend([
            TRACKS, 6, 0, CLOCK, 7, 0, IMAGE_FREE, 8, ROW_FREE, 9, LAYER_FREE, 10,
        ]);
        let matrix = [MATRIX, 1, 0, 0, 1, 0, 0];
        let older: Vec<_> = matrix
            .into_iter()
            .chain(definitions.iter().copied())
            .collect();
        let drawing = vec![ROW_DRAW, 4, 20, 20, LAYER_REF, 6];
        let mut latest = drawing.clone();
        assert!(carry_definitions(&older, &mut latest));
        definitions.extend(drawing);
        assert_eq!(latest, definitions);
    }

    #[test]
    fn deferred_row_sets_keep_their_drawing_and_frame_boundaries() {
        let row = [ROW_BEGIN, 7, 0, 10, 10, 1, ROW_END];
        let sets = [
            vec![SLOT_SET, 8, 2, SLOT, 1],
            [vec![LAYER_SET, 9, 17], vec![0; 17]].concat(),
            vec![TRACKS, 9, 0],
            vec![CLOCK, 9, 0],
        ];
        for set in &sets {
            for boundary in [
                vec![ROW_DRAW, 7, 0, 0],
                vec![MATRIX, 1, 0, 0, 1, 0, 0],
                vec![GROUP_END],
            ] {
                let older = [row.as_slice(), &sets[0], &boundary, set, &[ROW_FREE, 7]].concat();
                let mut latest = vec![ROW_DRAW, 10, 0, 0];
                let before = latest.clone();
                assert!(!carry_definitions(&older, &mut latest));
                assert_eq!(latest, before, "a rejected carry is atomic");
            }
            // The end of the older stream is also a boundary for Android's deferred sets.
            let older = [row.as_slice(), &sets[0]].concat();
            let mut latest = set.clone();
            let before = latest.clone();
            assert!(!carry_definitions(&older, &mut latest));
            assert_eq!(latest, before);
        }
        // A shown row cannot defer its following sets.
        let mut shown = row;
        shown[2] = 1 << 31;
        let older = [shown.as_slice(), &[ROW_DRAW, 7, 0, 0]].concat();
        assert!(carry_definitions(&older, &mut sets[0].clone()));
        // A retained non-set ends the first row's scope, but the next unshown row opens one.
        let older = [
            row.as_slice(),
            &[FONT, 1, 0, 400, 0],
            row.as_slice(),
            &[ROW_DRAW, 7, 0, 0],
        ]
        .concat();
        assert!(!carry_definitions(&older, &mut Vec::new()));
        let older = [row.as_slice(), &[FONT, 1, 0, 400, 0, ROW_DRAW, 7, 0, 0]].concat();
        assert!(carry_definitions(&older, &mut sets[0].clone()));
    }

    #[test]
    fn an_unknown_or_cut_stream_never_partially_changes_the_latest_frame() {
        for older in [
            vec![99],
            vec![IMAGE_DEF, 1, 10, 10, 99],
            vec![IMAGE_DEF, 1, 10, 10, GLYPHS, 1, 0, 0, 0, 5],
        ] {
            let mut latest = vec![ROW_DRAW, 2, 0, 0];
            let before = latest.clone();
            assert!(!carry_definitions(&older, &mut latest));
            assert_eq!(latest, before);
        }
    }

    #[test]
    fn overwritten_recordings_keep_definitions_and_order_unsplittable_frames() {
        FINISHED.with(|f| *f.borrow_mut() = None);
        streams().clear();
        let mut recorder = Recorder::new();
        let face = vec![FONT, 1, 0, 400, 0];
        let matrix = [MATRIX, 1, 0, 0, 1, 0, 0];
        let glyphs = vec![GLYPHS, 1, 12, 0xff000000, 0, 1, 7, 0, 0];
        recorder.ops = face.iter().copied().chain(matrix).collect();
        recorder.finish_stream().unwrap();
        recorder.ops = glyphs.clone();
        recorder.finish_stream().unwrap();
        let expected: Vec<_> = face.into_iter().chain(glyphs).collect();
        assert_eq!(FINISHED.with(|f| f.borrow_mut().take()), Some(expected));
        assert!(!waiting());

        recorder.ops = matrix.to_vec();
        recorder.finish_stream().unwrap();
        recorder.ops = vec![ROW_DRAW, 2, 0, 0];
        recorder.finish_stream().unwrap();
        assert_eq!(
            FINISHED.with(|f| f.borrow_mut().take()),
            Some(vec![ROW_DRAW, 2, 0, 0])
        );

        recorder.ops = vec![IMAGE_DEF, 1, 10, 10];
        recorder.finish_stream().unwrap();
        recorder.ops = vec![IMAGE_FREE, 1, IMAGE_DEF, 2, 10, 10];
        recorder.finish_stream().unwrap();
        recorder.ops = vec![IMAGE, 2, 0, 0, 10, 10];
        recorder.finish_stream().unwrap();
        assert_eq!(
            FINISHED.with(|f| f.borrow_mut().take()),
            Some(vec![
                IMAGE_DEF, 1, 10, 10, IMAGE_FREE, 1, IMAGE_DEF, 2, 10, 10, IMAGE, 2, 0, 0, 10, 10,
            ])
        );

        for older in [
            vec![99],
            vec![IMAGE_DEF, 1, 10],
            vec![IMAGE_DEF, 1, 10, 10, 99],
        ] {
            recorder.ops = older.clone();
            recorder.finish_stream().unwrap();
            let latest = vec![ROW_DRAW, 2, 0, 0];
            recorder.ops = latest.clone();
            recorder.finish_stream().unwrap();
            assert!(waiting());
            let newest = FINISHED.with(|f| f.borrow_mut().take());
            assert_eq!(after_turn(newest), Some(older));
            assert_eq!(next_stream(), Some(latest));
            assert!(!waiting());
        }

        let deferred = vec![
            ROW_BEGIN, 7, 0, 10, 10, 1, ROW_END, SLOT_SET, 8, 2, SLOT, 1, ROW_DRAW, 7, 0, 0,
            SLOT_SET, 9, 2, SLOT, 2,
        ];
        recorder.ops = deferred.clone();
        recorder.finish_stream().unwrap();
        recorder.ops = vec![ROW_FREE, 7];
        recorder.finish_stream().unwrap();
        let newest = FINISHED.with(|f| f.borrow_mut().take());
        assert_eq!(after_turn(newest), Some(deferred));
        assert_eq!(next_stream(), Some(vec![ROW_FREE, 7]));
        assert!(!waiting());

        for op in [99, 98, 97] {
            recorder.ops = vec![op];
            recorder.finish_stream().unwrap();
        }
        let newest = FINISHED.with(|f| f.borrow_mut().take());
        assert_eq!(after_turn(newest), Some(vec![99]));
        assert_eq!(next_stream(), Some(vec![98]));
        assert_eq!(next_stream(), Some(vec![97]));
        assert!(!waiting());

        // Adoption discards both queued and finished old-reader definitions before IDs restart.
        let old = Picture::Canvas(Arc::new(Pixmap::new(1, 1).unwrap()));
        assert_eq!(recorder.image_id(&old), 1);
        recorder.finish_stream().unwrap();
        keep(FINISHED.with(|f| f.borrow_mut().take()));
        recorder.ops = vec![IMAGE, 1, 0, 0, 1, 1];
        recorder.finish_stream().unwrap();
        assert!(waiting());
        assert!(with_pending(|p| p.contains_key(&1)));
        reader_replaced();
        assert!(!waiting());
        assert!(FINISHED.with(|f| f.borrow().is_none()));
        assert!(with_pending(|p| p.is_empty()));

        let new = Picture::Canvas(Arc::new(Pixmap::new(2, 1).unwrap()));
        let mut recorder = Recorder::new();
        assert_eq!(recorder.image_id(&new), 1);
        recorder.finish_stream().unwrap();
        let newest = FINISHED.with(|f| f.borrow_mut().take());
        assert_eq!(after_turn(newest), Some(vec![IMAGE_DEF, 1, 2, 1]));
        assert_eq!(with_pending(|p| p.remove(&1)).unwrap().key(), new.key());
        assert!(next_stream().is_none());
    }
}
