//! @ref LLP 1123 D13 — a painted box's reach: its surface and its hits past its border box, its children and what
//! reads its geometry where they were.
use super::super::*;

const APP: &str = r##"component App
  view
    box testId="root" width="100%" height="100%" background-color="#ffffff" position="relative"
      box testId="clip" position="absolute" left=15 top=35 width=300 height=300 overflow="hidden"
        box testId="card" position="absolute" left=5 top=5 width=100 height=60 background-color="#ff0000"
          box testId="child" position="absolute" left=10 top=10 width=20 height=20 background-color="#0000ff"
"##;

fn boot() -> Presenter<()> {
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(APP),
        (),
        (400., 800.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.frame();
    p
}

fn id(p: &Presenter<()>, test_id: &str) -> ViewId {
    let kernel = p.host().kernel();
    kernel
        .node_by_key(kernel.find_by_test_id(test_id)[0])
        .unwrap()
        .id
}

fn pixel(p: &mut Presenter<()>, x: f32, y: f32) -> [u8; 3] {
    let c = p.frame().pixel(x as u32, y as u32).unwrap().demultiply();
    [c.red(), c.green(), c.blue()]
}

const WHITE: [u8; 3] = [255, 255, 255];
const RED: [u8; 3] = [255, 0, 0];
const BLUE: [u8; 3] = [0, 0, 255];

/// The card's border box is (20, 40, 100, 60); reaching 10, 5, 15 and 20 (top right bottom left) it paints and takes
/// hits over (0, 30, 125, 85), inside its ancestor's clip, which begins at (15, 35).
#[test]
fn a_reaching_box_paints_and_takes_hits_past_its_border_box() {
    let mut p = boot();
    let (card, child, root) = (id(&p, "card"), id(&p, "child"), id(&p, "root"));
    let still = p.still();
    let empty = p.frame().data().to_vec();
    assert_eq!(pixel(&mut p, 17., 37.), WHITE);
    assert_eq!(p.hit(17., 37.), Some(id(&p, "clip")));
    assert!(p.set_box_reach(card, Some([10., 5., 15., 20.])));
    assert!(
        !p.set_box_reach(card, Some([10., 5., 15., 20.])),
        "the same reach is no change"
    );
    assert!(
        p.still() != still,
        "a frame that only scrolls would not show it"
    );
    // Its fill on each edge of the reached frame, and nothing past it
    for (x, y) in [(17., 37.), (122., 50.), (60., 112.), (17., 112.)] {
        assert_eq!(pixel(&mut p, x, y), RED, "at ({x}, {y})");
    }
    for (x, y) in [(127., 50.), (60., 117.)] {
        assert_eq!(pixel(&mut p, x, y), WHITE, "past the reach at ({x}, {y})");
    }
    // Its ancestor's clip still holds: nothing of it left of 15 or above 35
    assert_eq!(pixel(&mut p, 10., 50.), WHITE);
    assert_eq!(pixel(&mut p, 50., 32.), WHITE);
    assert_eq!(p.hit(10., 50.), Some(root));
    // Its hits in the strip, its child where it was laid out, and its geometry its laid-out box
    assert_eq!(p.hit(17., 37.), Some(card));
    assert_eq!(p.hit(122., 50.), Some(card));
    assert_eq!(pixel(&mut p, 40., 60.), BLUE);
    assert_eq!(p.hit(40., 60.), Some(child));
    assert_eq!(p.rect_of(card), Some((20., 40., 100., 60.)));
    assert_eq!(p.rect_of(child), Some((30., 50., 20., 20.)));
    // Cleared, the frame is the one it painted before
    assert!(p.set_box_reach(card, None));
    assert_eq!(p.hit(17., 37.), Some(id(&p, "clip")));
    assert!(p.frame().data() == empty, "as before the reach");
}
