//! Removed drawings on retained backends, including a replay through Raster.
use super::through::SWAP;
use super::*;

fn backend(canvas: bool) -> Box<dyn Backend> {
    if canvas {
        Box::new(crate::canvas::Recorder::new())
    } else {
        Box::new(crate::gpu::Gpu::new().expect("a GPU"))
    }
}

/// The filter the tests give a surviving ancestor. Canvas's reader declines
/// a blur under half a device pixel (`canvas/filter.rs`), so with `cpu` the
/// painter runs the chain into a picture there as every other backend does.
fn opacity(cpu: bool) -> &'static str {
    match cpu {
        true => "opacity(0.9) blur(0.1px)",
        false => "opacity(0.9)",
    }
}

fn shrinking_source(effect: &str) -> String {
    SWAP.replace("height=\"100%\"", "height=(second ? 100 : 600)")
        .replace(
            "main navigationKey=",
            &format!("main testId=\"effect\" {effect} navigationKey="),
        )
}

#[test]
fn a_surviving_filter_includes_the_removed_branches_paint_bounds() {
    let mut p = phone_source(&shrinking_source("filter=\"opacity(0.9)\""));
    let before = p.frame().pixel(200, 500).unwrap();
    assert!(before.red() > 200 && before.green() < 30);
    tap(&mut p, "swap");
    after(&mut p, 0.0);
    assert!(through::through(&p));
    assert_eq!(p.frame().pixel(200, 500).unwrap(), before);
}

#[test]
fn a_surviving_plane_includes_the_removed_branches_paint_bounds() {
    let mut p = phone_source(&shrinking_source(
        "rotate=\"y 10deg\" transform-origin=\"0 0\"",
    ));
    let effect = id(&p, "effect");
    assert!(p.host().presented(effect).rotate_axis.is_3d());
    after(&mut p, 0.0);
    let before = p.frame().pixel(200, 500).unwrap();
    assert!(before.red() > 200 && before.green() < 30);
    tap(&mut p, "swap");
    after(&mut p, 0.0);
    assert!(through::through(&p));
    assert_eq!(p.frame().pixel(200, 500).unwrap(), before);
}

#[test]
fn a_surviving_filters_bounds_follow_live_descendant_placement() {
    let source = SHRINKING_PANELS.replace("EFFECT", "filter=\"opacity(0.9)\"")
        .replace("SCALER", "scale=(second ? 0.8 : 1) translate=(second ? \"30px 20px\" : \"0 0\") transform-origin=\"0 0\"");
    let mut p = phone_source(&source);
    tap(&mut p, "swap");
    after(&mut p, 0.0);
    let pixel = p.frame().pixel(200, 450).unwrap();
    assert!(through::through(&p));
    assert!(pixel.red() > 200 && pixel.green() < 30, "{pixel:?}");
}

#[test]
fn a_removed_row_keeps_its_page_placement() {
    let source = SWAP.replace(
        "main navigationKey=",
        "main overflow=\"scroll\" navigationKey=",
    );
    let mut p = phone_source(&source);
    p.brush.replace_backend(backend(false));
    after(&mut p, 0.0);
    assert_eq!(red(&mut p, 20, 200), 0);
    tap(&mut p, "swap");
    after(&mut p, 20.0);
    assert_eq!(red(&mut p, 20, 200), 0, "no paint left of the old route");
    assert!(red(&mut p, 200, 500) > 150);
}

#[test]
fn gpu_rows_replay_when_a_surviving_ancestor_gains_a_filter() {
    filtered_rows(false);
}

#[test]
fn canvas_rows_replay_when_a_surviving_ancestor_gains_a_filter() {
    filtered_rows(true);
}

/// A chain Canvas's reader runs keeps the outgoing rows: the stream draws
/// them inside its `FILTER`.
#[test]
fn canvas_draws_the_outgoing_rows_through_a_surviving_ancestors_filter() {
    let source = SWAP.replace("main navigationKey=", "main filter=(second ? \"opacity(0.9)\" : \"none\") navigationKey=")
        .replace("          button press=swap", "          column overflow=\"scroll\" width=200 height=400\n            column\n              for i in [1, 2, 3, 4] key=i\n                column width=180 height=80 background-color=\"#00ff00\"\n          button press=swap");
    let mut p = phone_source(&source);
    p.brush.replace_backend(backend(true));
    after(&mut p, 0.0);
    p.host
        .dispatch_at(id(&p, "swap"), Event::Press, p.host().now());
    assert_eq!(p.after_commit(), None);
    after(&mut p, 0.0);
    assert!(through::through(&p));
    assert!(crate::canvas::filtered_row_draws() >= 4);
}

#[test]
fn gpu_rows_stay_inside_a_removed_sticky_containers_opacity() {
    sticky_rows(false, false);
    sticky_rows(false, true);
}

#[test]
fn canvas_rows_stay_inside_a_removed_sticky_containers_opacity() {
    sticky_rows(true, true);
}

fn sticky_rows(canvas: bool, filtered: bool) {
    let source = SWAP
        .replace("background-color=\"#ff0000\"", "")
        .replace("main navigationKey=", if filtered {
            if canvas {
                "main filter=(second ? \"opacity(0.9) blur(0.1px)\" : \"none\") navigationKey="
            } else {
                "main filter=(second ? \"opacity(0.9)\" : \"none\") navigationKey="
            }
        } else {
            "main navigationKey="
        })
        .replace("          button press=swap", "          column position=\"sticky\" top=0 opacity=0.5\n            column overflow=\"scroll\" width=200 height=240\n              column\n                for i in [1, 2, 3, 4] key=i\n                  column width=180 height=80 background-color=\"#ff0000\"\n          button press=swap");
    let mut p = phone_source(&source);
    p.brush.replace_backend(backend(canvas));
    after(&mut p, 0.0);
    assert!(p.brush.rows_referred().0 > 0);
    if !canvas {
        assert!(red(&mut p, 100, 150).abs_diff(128) <= 1);
    }
    p.host
        .dispatch_at(id(&p, "swap"), Event::Press, p.host().now());
    assert_eq!(p.after_commit(), None);
    after(&mut p, 0.0);
    assert!(through::through(&p));
    let frame = p.frame().clone();
    let pixels = if canvas {
        let [id, _, _] = crate::canvas::pending_pictures()
            .into_iter()
            .max_by_key(|p| p[0])
            .unwrap();
        let crate::canvas::Picture::Canvas(pixels) = crate::canvas::image(id).unwrap() else {
            panic!("the filtered outgoing drawing")
        };
        pixels
    } else {
        frame
    };
    let expected = if filtered { 115 } else { 128 };
    let ground = if filtered && !canvas { 25 } else { 0 };
    let pixel = pixels.pixel(100, 150).unwrap();
    assert!(
        pixel.red().abs_diff(expected + ground) <= 1 && pixel.green().abs_diff(ground) <= 1,
        "canvas={canvas}, filtered={filtered}: {pixel:?}"
    );
}

fn filtered_rows(canvas: bool) {
    let filter = format!(
        "main filter=(second ? \"{}\" : \"none\") navigationKey=",
        opacity(canvas)
    );
    let source = SWAP.replace("main navigationKey=", &filter)
        .replace("          button press=swap", "          column overflow=\"scroll\" width=200 height=400\n            column\n              for i in [1, 2, 3, 4] key=i\n                column width=180 height=80 background-color=\"#00ff00\"\n          button press=swap");
    {
        let mut p = phone_source(&source);
        p.brush.replace_backend(backend(canvas));
        after(&mut p, 0.0);
        assert!(p.brush.rows_referred().0 > 0);
        // The button is below the list; dispatch without hit targeting.
        p.host
            .dispatch_at(id(&p, "swap"), Event::Press, p.host().now());
        assert_eq!(p.after_commit(), None);
        after(&mut p, 0.0);
        assert!(through::through(&p));
        if canvas {
            let green = crate::canvas::pending_pictures()
                .into_iter()
                .any(|[id, _, _]| {
                    let picture = crate::canvas::image(id).unwrap();
                    picture
                        .as_ref()
                        .chunks_exact(4)
                        .filter(|px| px[1] > 200 && px[0] < 30)
                        .count()
                        > 1000
                });
            assert!(green, "the filtered outgoing rows reach Canvas");
        } else {
            let px = p.frame().pixel(100, 150).unwrap();
            assert!(
                px.green() > 200 && px.red() < 80,
                "the outgoing row: {px:?}"
            );
        }
    }
}

#[test]
fn sticky_descendants_follow_a_surviving_scrollers_live_scroll() {
    let source = SWAP.replace("main navigationKey=", "main testId=\"port\" overflow=\"scroll\" navigationKey=")
        .replace("height=\"100%\"", "height=400")
        .replace("position=\"absolute\" left=40 top=100 right=0 bottom=0 background-color=\"#ff0000\"", "height=1000 position=\"relative\"")
        .replace("          button press=swap", "          column position=\"sticky\" top=100 height=100 background-color=\"#ff0000\"\n          button press=swap")
        .replace("      if second", "      column height=1000\n      if second");
    for filter in ["", "filter=\"opacity(0.9)\""] {
        let source = source.replace(
            "testId=\"original\"",
            &format!("testId=\"original\" {filter}"),
        );
        let mut p = phone_source(&source);
        let port = id(&p, "port");
        p.scroll.insert(port, (0.0, 100.0));
        p.dirty = true;
        let before = red(&mut p, 200, 150);
        assert!(before > 200);
        p.host
            .dispatch_at(id(&p, "swap"), Event::Press, p.host().now());
        assert_eq!(p.after_commit(), None);
        after(&mut p, 0.0);
        p.scroll.insert(port, (0.0, 200.0));
        p.dirty = true;
        assert!(red(&mut p, 200, 150).abs_diff(before) <= 1, "{filter}");
        assert_eq!(red(&mut p, 200, 250), 0, "{filter}");
    }
}

#[test]
fn sticky_replay_in_a_retained_row_follows_live_scroll() {
    let source = SWAP
        .replace("height=\"100%\"", "height=400")
        .replace(
            "position=\"absolute\" left=40 top=100 right=0 bottom=0",
            "position=\"sticky\" top=100 height=100",
        )
        .replace(
            "      if second",
            "      column height=1000\n      if second",
        );
    for canvas in [false, true] {
        let main = format!("main testId=\"port\" overflow=\"scroll\" filter=(second ? \"{}\" : \"none\") navigationKey=", opacity(canvas));
        let mut p = phone_source(&source.replace("main navigationKey=", &main));
        p.brush.replace_backend(backend(canvas));
        let port = id(&p, "port");
        p.scroll.insert(port, (0.0, 100.0));
        p.dirty = true;
        p.frame();
        p.host
            .dispatch_at(id(&p, "swap"), Event::Press, p.host().now());
        assert_eq!(p.after_commit(), None);
        after(&mut p, 0.0);
        p.scroll.insert(port, (0.0, 200.0));
        p.dirty = true;
        let frame = p.frame().clone();
        let pixels = if canvas {
            let [id, _, _] = crate::canvas::pending_pictures()
                .into_iter()
                .max_by_key(|p| p[0])
                .unwrap();
            let crate::canvas::Picture::Canvas(pixels) = crate::canvas::image(id).unwrap() else {
                panic!("a filtered drawing")
            };
            pixels
        } else {
            frame
        };
        assert!(
            pixels.pixel(200, 150).unwrap().red() > 200,
            "canvas={canvas}"
        );
        let ground = if canvas { 0 } else { 25 };
        assert!(
            pixels.pixel(200, 250).unwrap().red().abs_diff(ground) <= 1,
            "canvas={canvas}"
        );
    }
}
