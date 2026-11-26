//! A drag out of the window reads the app's shipped files as the selected
//! update has them (LLP 1094.000 D13): the files it replaces from its
//! verified bytes, and those it removes as absent, though the embedded root
//! still holds them.
use super::*;
use exact_painted::presenter::drag_out::{Lift, Lifter, Offers};
use exact_painted::presenter::PointerKind;
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn a_drag_reads_shipped_files_as_the_selected_update_has_them() {
    let plan = |label: &str| {
        contract::compile(&format!(
            "component App\n  view\n    box testId=\"card\" draggable=true dataTransfer=DataTransfer(files=[\"assets/notes.txt\", \"assets/gone.txt\", \"assets/kept.txt\"]) width=170 height=50\n      text \"{label}\"\n"
        ))
        .unwrap()
        .encode()
    };
    let (baked, selected) = (plan("baked"), plan("selected"));
    let dir = crate::TempDir(
        std::env::temp_dir().join(format!("exact-linux-selected-drag-{}", std::process::id())),
    );
    let _ = std::fs::remove_dir_all(&dir);
    let embedded = dir.join("embedded");
    std::fs::create_dir_all(embedded.join("assets")).unwrap();
    for name in ["notes.txt", "gone.txt", "kept.txt"] {
        std::fs::write(embedded.join("assets").join(name), "embedded").unwrap();
    }
    let store = dir.join("store");
    std::fs::create_dir_all(&store).unwrap();
    let client = stage(
        &store,
        &baked,
        &selected,
        &[
            ("assets/notes.txt", b"updated".to_vec()),
            ("assets/kept.txt", b"embedded".to_vec()),
        ],
    );
    let mut config = selected_config(&embedded, &baked, client);
    let (mut p, error) = boot_presenter::<Named>(&mut config, (390.0, 844.0)).unwrap();
    assert!(error.is_none(), "{error:?}");
    let lifts: Rc<RefCell<Vec<Lift>>> = Rc::default();
    let heard = lifts.clone();
    p.set_lifter(Some(Lifter {
        offers: Offers {
            text: true,
            url: true,
            files: true,
        },
        lift: Box::new(move |lift| {
            heard.borrow_mut().push(lift);
            true
        }),
    }));
    p.set_pointer_kind(PointerKind::Mouse);
    p.frame();
    // The card is the view's first box, at the window's top left
    let (x, y, t) = (10., 10., p.host().now() + 10.);
    p.pointer_down(x, y, t).unwrap();
    for step in 1..=4 {
        p.pointer_move(x + 10. * step as f32, y, t + 16. * step as f64)
            .unwrap();
    }
    let lifts = lifts.borrow();
    let [lift] = lifts.as_slice() else {
        panic!("one lift, not {}", lifts.len());
    };
    assert_eq!(
        lift.files,
        ["assets/notes.txt", "assets/gone.txt", "assets/kept.txt"]
    );
    let read = |name: &str| {
        lift.assets
            .read_asset(name)
            .map(|bytes| bytes.map(|b| b.to_vec()))
    };
    assert_eq!(read("assets/notes.txt"), Ok(Some(b"updated".to_vec())));
    assert_eq!(read("assets/gone.txt"), Ok(None), "the update removed it");
    assert_eq!(read("assets/kept.txt"), Ok(Some(b"embedded".to_vec())));
    drop(p);
    let _ = std::fs::remove_dir_all(dir);
}
